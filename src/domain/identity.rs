use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    domain::{
        election::{Closed, Election, Open, VoteCommitError},
        storage::{ElectionEvent, EventRecord, EventStore},
        strategy::TallyStrategy,
    },
    error::{IdentityError, VotingFacadeError},
};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VoterId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BlindToken(Uuid);

impl BlindToken {
    pub fn generate() -> Self {
        Self(Uuid::new_v4())
    }
}

pub struct EligibilityRegistry {
    voters: HashMap<VoterId, bool>,
}

impl EligibilityRegistry {
    pub fn new(eligible_voters: Vec<VoterId>) -> Self {
        let voters = eligible_voters.into_iter().map(|id| (id, false)).collect();

        Self { voters }
    }

    pub fn issue_token(
        &mut self,
        voter_id: &VoterId,
        ballot_box_allowlist: &mut TokenAllowList,
    ) -> Result<BlindToken, IdentityError> {
        let claimed = self
            .voters
            .get_mut(voter_id)
            .ok_or(IdentityError::IneligibleVoter)?;

        if *claimed {
            return Err(IdentityError::TokenAlreadyIssued);
        }

        *claimed = true;
        let token = BlindToken::generate();
        ballot_box_allowlist.authorize(token.clone());

        Ok(token)
    }
}

#[derive(Default)]
pub struct TokenAllowList {
    valid_tokens: HashSet<BlindToken>,
}

impl TokenAllowList {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn authorize(&mut self, token: BlindToken) {
        self.valid_tokens.insert(token);
    }

    pub fn consume(&mut self, token: &BlindToken) -> bool {
        self.valid_tokens.remove(token)
    }
}

pub struct TalliedElection<S: TallyStrategy, E> {
    pub election: Election<S, Closed>,
    pub outcome: S::Outcome,
    pub store: E,
}

pub struct AnonymousBallotBox<S: TallyStrategy, E> {
    election: Election<S, Open>,
    store: E,
    allowlist: TokenAllowList,
}

impl<S, E> AnonymousBallotBox<S, E>
where
    S: TallyStrategy,
    S::Vote: Clone,
    S::Outcome: Clone,
    E: EventStore<S::Vote, S::Outcome>,
{
    pub fn new(
        election: Election<S, Open>,
        mut store: E,
        allowlist: TokenAllowList,
    ) -> Result<Self, VotingFacadeError> {
        let seq = store.current_sequence() + 1;
        store.append(EventRecord::new(seq, ElectionEvent::ElectionCreated))?;

        Ok(Self {
            election,
            store,
            allowlist,
        })
    }

    pub fn cast_anonymous_vote(
        &mut self,
        token: &BlindToken,
        vote: S::Vote,
    ) -> Result<(), VotingFacadeError> {
        if !self.allowlist.valid_tokens.contains(token) {
            return Err(IdentityError::InvalidOrSpentToken.into());
        }

        self.election
            .cast_vote_with(vote, |vote| {
                let seq = self.store.current_sequence() + 1;
                self.store
                    .append(EventRecord::new(seq, ElectionEvent::VoteCast(vote.clone())))
            })
            .map_err(|error| match error {
                VoteCommitError::Election(error) => VotingFacadeError::Election(error),
                VoteCommitError::Commit(error) => VotingFacadeError::Storage(error),
            })?;
        self.allowlist.consume(token);

        Ok(())
    }

    pub fn close_and_tally(mut self) -> Result<TalliedElection<S, E>, VotingFacadeError> {
        let closed_election = self.election.close();
        let outcome = closed_election.compute_result()?;

        let seq_close = self.store.current_sequence() + 1;
        self.store
            .append(EventRecord::new(seq_close, ElectionEvent::ElectionClosed))?;

        let seq_tally = self.store.current_sequence() + 1;
        self.store.append(EventRecord::new(
            seq_tally,
            ElectionEvent::TallyComputed(outcome.clone()),
        ))?;

        Ok(TalliedElection {
            election: closed_election,
            outcome,
            store: self.store,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::domain::{
        academic::{AcademicElectionBuilder, AcademicOutcome, Slate},
        storage::MemoryEventStore,
    };
    use crate::error::StorageError;

    use super::*;

    struct FailOnceOnVoteStore {
        inner: MemoryEventStore<Slate, AcademicOutcome>,
        fail_next_vote: bool,
    }

    impl EventStore<Slate, AcademicOutcome> for FailOnceOnVoteStore {
        fn append(
            &mut self,
            record: EventRecord<Slate, AcademicOutcome>,
        ) -> Result<(), StorageError> {
            if self.fail_next_vote && matches!(&record.payload, ElectionEvent::VoteCast(_)) {
                self.fail_next_vote = false;
                return Err(StorageError::AppendFailed("simulated failure".into()));
            }

            self.inner.append(record)
        }

        fn get_all(&self) -> Result<Vec<EventRecord<Slate, AcademicOutcome>>, StorageError> {
            self.inner.get_all()
        }

        fn get_since(
            &self,
            sequence: u64,
        ) -> Result<Vec<EventRecord<Slate, AcademicOutcome>>, StorageError> {
            self.inner.get_since(sequence)
        }

        fn current_sequence(&self) -> u64 {
            self.inner.current_sequence()
        }
    }

    #[test]
    fn test_full_anonymous_voting_flow() {
        let student_a = VoterId("ra000000".into());
        let student_b = VoterId("ra111111".into());
        let outsider = VoterId("ra222222".into());

        let mut registry = EligibilityRegistry::new(vec![student_a.clone(), student_b.clone()]);
        let mut allowlist = TokenAllowList::new();

        let token_a = registry.issue_token(&student_a, &mut allowlist).unwrap();
        let token_b = registry.issue_token(&student_b, &mut allowlist).unwrap();

        assert_eq!(
            registry.issue_token(&outsider, &mut allowlist),
            Err(IdentityError::IneligibleVoter)
        );

        assert_eq!(
            registry.issue_token(&student_a, &mut allowlist),
            Err(IdentityError::TokenAlreadyIssued)
        );

        let election = AcademicElectionBuilder::new()
            .add_slate("ABC")
            .add_slate("XYZ")
            .with_quorum(2)
            .build()
            .unwrap();
        let store = MemoryEventStore::new();
        let mut ballot_box = AnonymousBallotBox::new(election, store, allowlist).unwrap();

        let xyz = Slate::new("XYZ").unwrap();
        ballot_box
            .cast_anonymous_vote(&token_a, xyz.clone())
            .unwrap();
        ballot_box
            .cast_anonymous_vote(&token_b, xyz.clone())
            .unwrap();

        assert_eq!(
            ballot_box.cast_anonymous_vote(&token_a, xyz.clone()),
            Err(VotingFacadeError::Identity(
                IdentityError::InvalidOrSpentToken
            ))
        );

        let forged = BlindToken::generate();
        assert_eq!(
            ballot_box.cast_anonymous_vote(&forged, xyz.clone()),
            Err(VotingFacadeError::Identity(
                IdentityError::InvalidOrSpentToken
            ))
        );

        let tally = ballot_box.close_and_tally().unwrap();
        assert_eq!(tally.outcome, AcademicOutcome::Winner(xyz));
        assert_eq!(tally.store.get_all().unwrap().len(), 5); // Created + 2 Votes + Closed + Tallied.
    }

    #[test]
    fn invalid_vote_does_not_spend_token_or_write_an_event() {
        let voter = VoterId("ra000001".into());
        let mut registry = EligibilityRegistry::new(vec![voter.clone()]);
        let mut allowlist = TokenAllowList::new();
        let token = registry.issue_token(&voter, &mut allowlist).unwrap();
        let election = AcademicElectionBuilder::new()
            .add_slate("ABC")
            .build()
            .unwrap();
        let store = MemoryEventStore::new();
        let mut ballot_box = AnonymousBallotBox::new(election, store, allowlist).unwrap();

        let invalid_slate = Slate::new("UNKNOWN").unwrap();
        assert!(matches!(
            ballot_box.cast_anonymous_vote(&token, invalid_slate),
            Err(VotingFacadeError::Election(
                crate::error::ElectionError::InvalidVote(_)
            ))
        ));

        let valid_slate = Slate::new("ABC").unwrap();
        ballot_box
            .cast_anonymous_vote(&token, valid_slate.clone())
            .unwrap();
        let tally = ballot_box.close_and_tally().unwrap();

        assert_eq!(tally.outcome, AcademicOutcome::Winner(valid_slate));
        assert_eq!(tally.store.get_all().unwrap().len(), 4);
    }

    #[test]
    fn storage_failure_does_not_spend_token_or_accept_vote() {
        let voter = VoterId("ra000004".into());
        let mut registry = EligibilityRegistry::new(vec![voter.clone()]);
        let mut allowlist = TokenAllowList::new();
        let token = registry.issue_token(&voter, &mut allowlist).unwrap();
        let election = AcademicElectionBuilder::new()
            .add_slate("ABC")
            .build()
            .unwrap();
        let store = FailOnceOnVoteStore {
            inner: MemoryEventStore::new(),
            fail_next_vote: true,
        };
        let mut ballot_box = AnonymousBallotBox::new(election, store, allowlist).unwrap();
        let vote = Slate::new("ABC").unwrap();

        assert_eq!(
            ballot_box.cast_anonymous_vote(&token, vote.clone()),
            Err(VotingFacadeError::Storage(StorageError::AppendFailed(
                "simulated failure".into()
            )))
        );
        ballot_box
            .cast_anonymous_vote(&token, vote.clone())
            .unwrap();

        let tally = ballot_box.close_and_tally().unwrap();
        assert_eq!(tally.outcome, AcademicOutcome::Winner(vote));
        assert_eq!(tally.store.get_all().unwrap().len(), 4);
    }

    #[test]
    fn token_serialization_round_trip_preserves_authorization() {
        let voter = VoterId("ra000002".into());
        let mut registry = EligibilityRegistry::new(vec![voter.clone()]);
        let mut allowlist = TokenAllowList::new();
        let token = registry.issue_token(&voter, &mut allowlist).unwrap();

        let serialized = serde_json::to_string(&token).unwrap();
        let restored: BlindToken = serde_json::from_str(&serialized).unwrap();

        assert_eq!(token, restored);
        assert!(allowlist.consume(&restored));
        assert!(!allowlist.consume(&token));
    }

    #[test]
    fn duplicate_eligible_voters_receive_only_one_token() {
        let voter = VoterId("ra000003".into());
        let mut registry = EligibilityRegistry::new(vec![voter.clone(), voter.clone()]);
        let mut allowlist = TokenAllowList::new();

        let token = registry.issue_token(&voter, &mut allowlist).unwrap();
        assert_eq!(
            registry.issue_token(&voter, &mut allowlist),
            Err(IdentityError::TokenAlreadyIssued)
        );
        assert!(allowlist.consume(&token));
    }
}
