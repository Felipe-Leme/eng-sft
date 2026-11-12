use eng_sft::{
    domain::{
        academic::{AcademicElectionBuilder, AcademicOutcome, Slate},
        budget::{BudgetElectionBuilder, BudgetOutcome, Project},
        condo::{CondoElectionBuilder, CondoOutcome, CondoVote},
        crypto::{GENESIS_HASH, HashedEventRecord, verify_chain},
        identity::{AnonymousBallotBox, EligibilityRegistry, TokenAllowList, VoterId},
        merkle::MerkleTree,
        storage::{ElectionEvent, EventRecord, EventStore, MemoryEventStore},
    },
    error::{ElectionError, IdentityError, VotingFacadeError},
};
use rust_decimal::dec;

#[test]
fn academic_election_rejects_unknown_slate_and_tallies_votes() {
    let mut election = AcademicElectionBuilder::new()
        .add_slate(" RED ")
        .add_slate("BLUE")
        .with_quorum(2)
        .build()
        .unwrap();
    let red = Slate::new("RED").unwrap();

    assert!(matches!(
        election.cast_vote(Slate::new("UNKNOWN").unwrap()),
        Err(ElectionError::InvalidVote(_))
    ));
    election.cast_vote(red.clone()).unwrap();
    election.cast_vote(red.clone()).unwrap();

    assert_eq!(
        election.close().compute_result().unwrap(),
        AcademicOutcome::Winner(red)
    );
}

#[test]
fn anonymous_voting_issues_one_use_tokens_and_returns_auditable_tally() {
    let voter = VoterId("student-1".into());
    let outsider = VoterId("outsider".into());
    let mut registry = EligibilityRegistry::new(vec![voter.clone()]);
    let mut allowlist = TokenAllowList::new();
    let token = registry.issue_token(&voter, &mut allowlist).unwrap();

    assert_eq!(
        registry.issue_token(&outsider, &mut allowlist),
        Err(IdentityError::IneligibleVoter)
    );
    assert_eq!(
        registry.issue_token(&voter, &mut allowlist),
        Err(IdentityError::TokenAlreadyIssued)
    );

    let election = AcademicElectionBuilder::new()
        .add_slate("RED")
        .add_slate("BLUE")
        .build()
        .unwrap();
    let mut ballot_box =
        AnonymousBallotBox::new(election, MemoryEventStore::new(), allowlist).unwrap();
    let red = Slate::new("RED").unwrap();
    ballot_box.cast_anonymous_vote(&token, red.clone()).unwrap();
    assert_eq!(
        ballot_box.cast_anonymous_vote(&token, red.clone()),
        Err(VotingFacadeError::Identity(
            IdentityError::InvalidOrSpentToken
        ))
    );

    let tally = ballot_box.close_and_tally().unwrap();
    assert_eq!(tally.outcome, AcademicOutcome::Winner(red));
    assert_eq!(tally.election.compute_result().unwrap(), tally.outcome);

    let events = tally.store.get_all().unwrap();
    assert_eq!(events.len(), 4);
    assert!(matches!(events[1].payload, ElectionEvent::VoteCast(_)));
    assert_eq!(events[3].sequence, 4);
}

#[test]
fn budget_election_respects_the_spending_ceiling() {
    let mut election = BudgetElectionBuilder::new()
        .add_project("Park", dec!(50))
        .add_project("Library", dec!(30))
        .with_ceiling(dec!(50))
        .build()
        .unwrap();
    election
        .cast_vote(vec!["Park".into(), "Library".into()])
        .unwrap();
    election
        .cast_vote(vec!["Park".into(), "Library".into()])
        .unwrap();

    let outcome = election.close().compute_result().unwrap();
    assert_eq!(outcome.total_spent, dec!(50));
    assert_eq!(
        outcome,
        BudgetOutcome {
            selected_projects: vec![Project {
                name: "Park".into(),
                cost: dec!(50),
            }],
            total_spent: dec!(50),
        }
    );
}

#[test]
fn condominium_election_uses_fractional_quorum_and_weighting() {
    let mut election = CondoElectionBuilder::new()
        .add_option("Repair")
        .add_option("Renovate")
        .with_quorum(dec!(0.8))
        .build()
        .unwrap();
    election
        .cast_vote(CondoVote {
            choice: "Repair".into(),
            fraction: dec!(0.6),
        })
        .unwrap();
    election
        .cast_vote(CondoVote {
            choice: "Renovate".into(),
            fraction: dec!(0.4),
        })
        .unwrap();

    assert_eq!(
        election.close().compute_result().unwrap(),
        CondoOutcome::Winner("Repair".into())
    );
}

#[test]
fn hashed_event_chain_and_merkle_proof_verify() {
    let payloads = [
        ElectionEvent::<String, String>::ElectionCreated,
        ElectionEvent::VoteCast("RED".into()),
    ];
    let mut previous_hash = GENESIS_HASH.to_owned();
    let records = payloads
        .into_iter()
        .enumerate()
        .map(|(index, payload)| {
            let record = HashedEventRecord::new(
                index as u64 + 1,
                format!("2026-01-01T00:00:{:02}Z", index + 1),
                previous_hash.clone(),
                payload,
            )
            .unwrap();
            previous_hash.clone_from(&record.current_hash);
            record
        })
        .collect::<Vec<_>>();

    assert!(verify_chain(&records).unwrap());

    let leaves: Vec<_> = records
        .iter()
        .map(|record| record.current_hash.as_bytes())
        .collect();
    let tree = MerkleTree::new(&leaves);
    let proof = tree.generate_proof(1).unwrap();
    assert!(proof.verify(&tree.root()));
}

#[test]
fn memory_event_store_enforces_sequential_appends() {
    let mut store = MemoryEventStore::<String, String>::new();
    store
        .append(EventRecord::new(1, ElectionEvent::ElectionCreated))
        .unwrap();

    let error = store
        .append(EventRecord::new(3, ElectionEvent::ElectionClosed))
        .unwrap_err();
    assert_eq!(
        error,
        eng_sft::error::StorageError::ConcurrencyConflict {
            expected: 2,
            got: 3,
        }
    );
    assert_eq!(store.current_sequence(), 1);
}
