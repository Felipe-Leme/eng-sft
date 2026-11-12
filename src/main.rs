use std::fmt::Display;

use rust_decimal::{Decimal, dec};

use eng_sft::domain::{
    academic::{AcademicElectionBuilder, AcademicOutcome, Slate},
    budget::{BudgetElectionBuilder, BudgetOutcome},
    condo::{CondoElectionBuilder, CondoOutcome, CondoVote},
    crypto::{GENESIS_HASH, HashedEventRecord, verify_chain},
    identity::{AnonymousBallotBox, EligibilityRegistry, TokenAllowList, VoterId},
    merkle::MerkleTree,
    storage::{ElectionEvent, EventRecord, EventStore, MemoryEventStore},
};

fn print_section(title: &str) {
    println!("\n=== {title} ===");
}

fn show_expected_error<E: Display>(label: &str, result: Result<(), E>) {
    match result {
        Err(error) => println!("  [expected error] {label}: {error}"),
        Ok(()) => panic!("Expected {label}, but the operation succeeded"),
    }
}

fn print_academic_outcome(outcome: &AcademicOutcome) {
    match outcome {
        AcademicOutcome::Winner(slate) => println!("Result: {slate} wins"),
        AcademicOutcome::Tie(slates) => {
            let names = slates
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            println!("Result: tie between {names}");
        }
        AcademicOutcome::QuorumNotMet => println!("Result: quorum not met"),
    }
}

fn print_budget_outcome(outcome: &BudgetOutcome) {
    println!("Selected projects:");
    if outcome.selected_projects.is_empty() {
        println!("  (none)");
    } else {
        for project in &outcome.selected_projects {
            println!("  - {} ({})", project.name, project.cost);
        }
    }
    println!("Total spent: {}", outcome.total_spent);
}

fn print_condo_outcome(outcome: &CondoOutcome) {
    match outcome {
        CondoOutcome::Winner(option) => println!("Result: {option} wins"),
        CondoOutcome::Tie(options) => {
            println!("Result: tie between {}", options.join(", "));
        }
        CondoOutcome::QuorumNotMet => println!("Result: quorum not met"),
    }
}

fn run_anonymous_voting_example() -> Result<(), Box<dyn std::error::Error>> {
    let voter = VoterId("ra000001".into());
    let mut registry = EligibilityRegistry::new(vec![voter.clone()]);
    let mut allowlist = TokenAllowList::new();
    let token = registry.issue_token(&voter, &mut allowlist)?;
    let election = AcademicElectionBuilder::new()
        .add_slate("RED")
        .add_slate("BLU")
        .build()?;
    let mut ballot_box = AnonymousBallotBox::new(election, MemoryEventStore::new(), allowlist)?;

    ballot_box.cast_anonymous_vote(&token, Slate::new("RED")?)?;
    show_expected_error(
        "spent anonymous token",
        ballot_box.cast_anonymous_vote(&token, Slate::new("BLU")?),
    );

    let tally = ballot_box.close_and_tally()?;
    print_academic_outcome(&tally.outcome);
    println!(
        "Anonymous election audit events: {}",
        tally.store.get_all()?.len()
    );

    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    print_section("Academic election");
    println!("Validation examples:");
    show_expected_error(
        "no-slate error",
        AcademicElectionBuilder::new().build().map(|_| ()),
    );
    show_expected_error(
        "invalid-slate-name error",
        AcademicElectionBuilder::new()
            .add_slate(" ")
            .build()
            .map(|_| ()),
    );

    let mut academic = AcademicElectionBuilder::new()
        .add_slate("RED")
        .add_slate("BLU")
        .with_quorum(1)
        .build()?;
    let red = Slate::new("RED")?;
    let blu = Slate::new("BLU")?;
    show_expected_error(
        "invalid-vote error",
        academic.cast_vote(Slate::new("GREEN")?),
    );
    academic.cast_vote(red.clone())?;
    academic.cast_vote(red.clone())?;
    academic.cast_vote(blu.clone())?;
    let academic_outcome = academic.close().compute_result()?;
    print_academic_outcome(&academic_outcome);

    let empty_academic = AcademicElectionBuilder::new().add_slate("RED").build()?;
    show_expected_error(
        "tally-failed error",
        empty_academic.close().compute_result().map(|_| ()),
    );

    let mut store: MemoryEventStore<Slate, AcademicOutcome> = MemoryEventStore::new();
    store.append(EventRecord::new(1, ElectionEvent::ElectionCreated))?;
    store.append(EventRecord::new(2, ElectionEvent::VoteCast(red.clone())))?;
    store.append(EventRecord::new(3, ElectionEvent::VoteCast(blu)))?;
    store.append(EventRecord::new(4, ElectionEvent::ElectionClosed))?;
    store.append(EventRecord::new(
        5,
        ElectionEvent::TallyComputed(academic_outcome),
    ))?;
    show_expected_error(
        "storage sequence-conflict error",
        store.append(EventRecord::new(7, ElectionEvent::ElectionClosed)),
    );
    println!("Audit log:");
    for event in store.get_all()? {
        println!("  #{:<2} {:?}", event.sequence, event.payload);
    }
    println!("Events from sequence 3: {}", store.get_since(3)?.len());

    print_section("Anonymous voting");
    run_anonymous_voting_example()?;

    print_section("Cryptographic audit");
    let audit_payloads = [
        ElectionEvent::<String, String>::ElectionCreated,
        ElectionEvent::VoteCast("RED".to_owned()),
        ElectionEvent::ElectionClosed,
        ElectionEvent::TallyComputed("RED".to_owned()),
    ];
    let mut hashed_events = Vec::with_capacity(audit_payloads.len());
    let mut previous_hash = GENESIS_HASH.to_owned();
    for (index, payload) in audit_payloads.into_iter().enumerate() {
        let record = HashedEventRecord::new(
            index as u64 + 1,
            format!("2026-01-01T00:00:{:02}Z", index + 1),
            previous_hash,
            payload,
        )?;
        previous_hash = record.current_hash.clone();
        hashed_events.push(record);
    }

    let chain_valid = verify_chain(&hashed_events)?;
    println!("Hash chain valid: {chain_valid}");
    if !chain_valid {
        return Err("Election audit hash chain failed verification".into());
    }

    let event_hashes: Vec<&[u8]> = hashed_events
        .iter()
        .map(|record| record.current_hash.as_bytes())
        .collect();
    let merkle_tree = MerkleTree::new(&event_hashes);
    let merkle_root = merkle_tree.root();
    let vote_proof = merkle_tree
        .generate_proof(1)
        .ok_or("Vote event missing from Merkle tree")?;
    let proof_valid = vote_proof.verify(&merkle_root);
    println!("Merkle root: {merkle_root}");
    println!("Vote event proof valid: {proof_valid}");
    if !proof_valid {
        return Err("Election vote Merkle proof failed verification".into());
    }

    print_section("Budget election");
    println!("Validation examples:");
    show_expected_error(
        "no-projects error",
        BudgetElectionBuilder::new()
            .with_ceiling(dec!(100))
            .build()
            .map(|_| ()),
    );
    show_expected_error(
        "negative-ceiling error",
        BudgetElectionBuilder::new()
            .add_project("Park", dec!(50))
            .with_ceiling(dec!(-1))
            .build()
            .map(|_| ()),
    );
    show_expected_error(
        "invalid-project-name error",
        BudgetElectionBuilder::new()
            .add_project(" ", dec!(50))
            .with_ceiling(dec!(100))
            .build()
            .map(|_| ()),
    );

    let mut budget = BudgetElectionBuilder::new()
        .add_project("Park", dec!(50))
        .add_project("Library", dec!(30))
        .with_ceiling(dec!(60))
        .build()?;
    show_expected_error(
        "invalid budget vote",
        budget.cast_vote(vec!["Unknown".to_owned()]),
    );
    budget.cast_vote(vec!["Park".to_owned(), "Library".to_owned()])?;
    budget.cast_vote(vec!["Library".to_owned(), "Park".to_owned()])?;
    let budget_outcome = budget.close().compute_result()?;
    print_budget_outcome(&budget_outcome);

    print_section("Condominium election");
    println!("Validation examples:");
    show_expected_error(
        "no-options error",
        CondoElectionBuilder::new().build().map(|_| ()),
    );
    show_expected_error(
        "out-of-range-quorum error",
        CondoElectionBuilder::new()
            .with_quorum(dec!(1.1))
            .add_option("Repair")
            .build()
            .map(|_| ()),
    );
    show_expected_error(
        "invalid-option-name error",
        CondoElectionBuilder::new()
            .add_option(" ")
            .build()
            .map(|_| ()),
    );

    let mut condo = CondoElectionBuilder::new()
        .add_option("Repair")
        .add_option("Renovate")
        .with_quorum(dec!(0.5))
        .build()?;
    show_expected_error(
        "invalid condominium vote",
        condo.cast_vote(CondoVote {
            choice: "Unknown".to_owned(),
            fraction: dec!(0.5),
        }),
    );
    show_expected_error(
        "invalid vote fraction",
        condo.cast_vote(CondoVote {
            choice: "Repair".to_owned(),
            fraction: Decimal::ZERO,
        }),
    );
    condo.cast_vote(CondoVote {
        choice: "Repair".to_owned(),
        fraction: dec!(0.3),
    })?;
    condo.cast_vote(CondoVote {
        choice: "Renovate".to_owned(),
        fraction: dec!(0.4),
    })?;
    let condo_outcome = condo.close().compute_result()?;
    print_condo_outcome(&condo_outcome);

    Ok(())
}
