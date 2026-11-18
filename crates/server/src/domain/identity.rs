use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use uuid::Uuid;

use crate::{
    domain::{
        election::{Closed, Election, Open},
        storage::{ElectionEvent, EventRecord, Infrastructure},
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

pub struct TalliedElection<S: TallyStrategy, I> {
    pub election: Election<S, Closed>,
    pub outcome: S::Outcome,
    pub store: I,
    pub stream_id: String,
}

pub struct AnonymousBallotBox<S: TallyStrategy, I> {
    election: Election<S, Open>,
    store: I,
    stream_id: String,
    allowlist: TokenAllowList,
}

impl<S, I> AnonymousBallotBox<S, I>
where
    S: TallyStrategy,
    S::Vote: Clone + Send + Sync + 'static,
    S::Outcome: Clone + Send + Sync + 'static,
    I: Infrastructure<S::Vote, S::Outcome>,
{
    pub async fn new(
        election: Election<S, Open>,
        store: I,
        allowlist: TokenAllowList,
    ) -> Result<Self, VotingFacadeError> {
        let stream_id = Uuid::new_v4().to_string();
        let seq = next_sequence(store.current_sequence(&stream_id).await?)?;
        store
            .append(
                &stream_id,
                EventRecord::new(seq, ElectionEvent::ElectionCreated),
            )
            .await?;

        Ok(Self {
            election,
            store,
            stream_id,
            allowlist,
        })
    }

    pub async fn cast_anonymous_vote(
        &mut self,
        token: &BlindToken,
        vote: S::Vote,
    ) -> Result<(), VotingFacadeError> {
        if !self.allowlist.valid_tokens.contains(token) {
            return Err(IdentityError::InvalidOrSpentToken.into());
        }

        self.election
            .validate_vote(&vote)
            .map_err(VotingFacadeError::Election)?;
        let sequence = self
            .store
            .current_sequence(&self.stream_id)
            .await?
            .checked_add(1)
            .ok_or_else(|| {
                VotingFacadeError::Storage(crate::error::StorageError::AppendFailed(
                    "Event sequence overflow".to_owned(),
                ))
            })?;
        self.store
            .append(
                &self.stream_id,
                EventRecord::new(sequence, ElectionEvent::VoteCast(vote.clone())),
            )
            .await?;
        self.election
            .record_vote(vote)
            .map_err(VotingFacadeError::Election)?;
        self.allowlist.consume(token);

        Ok(())
    }

    pub async fn close_and_tally(self) -> Result<TalliedElection<S, I>, VotingFacadeError> {
        let closed_election = self.election.close();
        let outcome = closed_election.compute_result()?;

        let seq_close = next_sequence(self.store.current_sequence(&self.stream_id).await?)?;
        self.store
            .append(
                &self.stream_id,
                EventRecord::new(seq_close, ElectionEvent::ElectionClosed),
            )
            .await?;

        let seq_tally = next_sequence(self.store.current_sequence(&self.stream_id).await?)?;
        self.store
            .append(
                &self.stream_id,
                EventRecord::new(seq_tally, ElectionEvent::TallyComputed(outcome.clone())),
            )
            .await?;

        Ok(TalliedElection {
            election: closed_election,
            outcome,
            store: self.store,
            stream_id: self.stream_id,
        })
    }
}

fn next_sequence(current: u64) -> Result<u64, VotingFacadeError> {
    current.checked_add(1).ok_or_else(|| {
        VotingFacadeError::Storage(crate::error::StorageError::AppendFailed(
            "Event sequence overflow".to_owned(),
        ))
    })
}

#[cfg(test)]
mod tests {
    use crate::domain::{
        academic::{AcademicElectionBuilder, AcademicOutcome, Slate},
        storage::{PersistedElection, PersistedElectionSummary},
    };
    use crate::error::StorageError;
    use crate::infrastructure::MemoryInfrastructure;
    use tokio::sync::Mutex;

    use super::*;

    struct FailOnceOnVoteStore {
        inner: MemoryInfrastructure<Slate, AcademicOutcome>,
        fail_next_vote: Mutex<bool>,
    }

    #[async_trait::async_trait]
    impl Infrastructure<Slate, AcademicOutcome> for FailOnceOnVoteStore {
        async fn create_election(
            &self,
            id: String,
            name: String,
            manifest: String,
        ) -> Result<(), StorageError> {
            self.inner.create_election(id, name, manifest).await
        }

        async fn get_election(&self, id: &str) -> Result<Option<PersistedElection>, StorageError> {
            self.inner.get_election(id).await
        }

        async fn list_elections(&self) -> Result<Vec<PersistedElectionSummary>, StorageError> {
            self.inner.list_elections().await
        }

        async fn append_vote_event(
            &self,
            id: &str,
            voter_id: uuid::Uuid,
            event: crate::domain::crypto::HashedEventRecord<serde_json::Value, serde_json::Value>,
        ) -> Result<(), StorageError> {
            self.inner.append_vote_event(id, voter_id, event).await
        }

        async fn close_election(
            &self,
            id: &str,
            expected_vote_sequence: u64,
            outcome: serde_json::Value,
        ) -> Result<(), StorageError> {
            self.inner
                .close_election(id, expected_vote_sequence, outcome)
                .await
        }

        async fn append(
            &self,
            stream_id: &str,
            record: EventRecord<Slate, AcademicOutcome>,
        ) -> Result<(), StorageError> {
            let fail = if matches!(&record.payload, ElectionEvent::VoteCast(_)) {
                let mut fail_next_vote = self.fail_next_vote.lock().await;
                let fail = *fail_next_vote;
                *fail_next_vote = false;
                fail
            } else {
                false
            };
            if fail {
                return Err(StorageError::AppendFailed("simulated failure".into()));
            }

            self.inner.append(stream_id, record).await
        }

        async fn get_all(
            &self,
            stream_id: &str,
        ) -> Result<Vec<EventRecord<Slate, AcademicOutcome>>, StorageError> {
            self.inner.get_all(stream_id).await
        }

        async fn get_since(
            &self,
            stream_id: &str,
            sequence: u64,
        ) -> Result<Vec<EventRecord<Slate, AcademicOutcome>>, StorageError> {
            self.inner.get_since(stream_id, sequence).await
        }

        async fn current_sequence(&self, stream_id: &str) -> Result<u64, StorageError> {
            self.inner.current_sequence(stream_id).await
        }
    }

    #[tokio::test]
    async fn test_full_anonymous_voting_flow() {
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
        let store = MemoryInfrastructure::new();
        let mut ballot_box = AnonymousBallotBox::new(election, store, allowlist)
            .await
            .unwrap();

        let xyz = Slate::new("XYZ").unwrap();
        ballot_box
            .cast_anonymous_vote(&token_a, xyz.clone())
            .await
            .unwrap();
        ballot_box
            .cast_anonymous_vote(&token_b, xyz.clone())
            .await
            .unwrap();

        assert_eq!(
            ballot_box.cast_anonymous_vote(&token_a, xyz.clone()).await,
            Err(VotingFacadeError::Identity(
                IdentityError::InvalidOrSpentToken
            ))
        );

        let forged = BlindToken::generate();
        assert_eq!(
            ballot_box.cast_anonymous_vote(&forged, xyz.clone()).await,
            Err(VotingFacadeError::Identity(
                IdentityError::InvalidOrSpentToken
            ))
        );

        let tally = ballot_box.close_and_tally().await.unwrap();
        assert_eq!(tally.outcome, AcademicOutcome::Winner(xyz));
        assert_eq!(
            tally.store.get_all(&tally.stream_id).await.unwrap().len(),
            5
        );
    }

    #[tokio::test]
    async fn invalid_vote_does_not_spend_token_or_write_an_event() {
        let voter = VoterId("ra000001".into());
        let mut registry = EligibilityRegistry::new(vec![voter.clone()]);
        let mut allowlist = TokenAllowList::new();
        let token = registry.issue_token(&voter, &mut allowlist).unwrap();
        let election = AcademicElectionBuilder::new()
            .add_slate("ABC")
            .build()
            .unwrap();
        let store = MemoryInfrastructure::new();
        let mut ballot_box = AnonymousBallotBox::new(election, store, allowlist)
            .await
            .unwrap();

        let invalid_slate = Slate::new("UNKNOWN").unwrap();
        assert!(matches!(
            ballot_box.cast_anonymous_vote(&token, invalid_slate).await,
            Err(VotingFacadeError::Election(
                crate::error::ElectionError::InvalidVote(_)
            ))
        ));

        let valid_slate = Slate::new("ABC").unwrap();
        ballot_box
            .cast_anonymous_vote(&token, valid_slate.clone())
            .await
            .unwrap();
        let tally = ballot_box.close_and_tally().await.unwrap();

        assert_eq!(tally.outcome, AcademicOutcome::Winner(valid_slate));
        assert_eq!(
            tally.store.get_all(&tally.stream_id).await.unwrap().len(),
            4
        );
    }

    #[tokio::test]
    async fn storage_failure_does_not_spend_token_or_accept_vote() {
        let voter = VoterId("ra000004".into());
        let mut registry = EligibilityRegistry::new(vec![voter.clone()]);
        let mut allowlist = TokenAllowList::new();
        let token = registry.issue_token(&voter, &mut allowlist).unwrap();
        let election = AcademicElectionBuilder::new()
            .add_slate("ABC")
            .build()
            .unwrap();
        let store = FailOnceOnVoteStore {
            inner: MemoryInfrastructure::new(),
            fail_next_vote: Mutex::new(true),
        };
        let mut ballot_box = AnonymousBallotBox::new(election, store, allowlist)
            .await
            .unwrap();
        let vote = Slate::new("ABC").unwrap();

        assert_eq!(
            ballot_box.cast_anonymous_vote(&token, vote.clone()).await,
            Err(VotingFacadeError::Storage(StorageError::AppendFailed(
                "simulated failure".into()
            )))
        );
        ballot_box
            .cast_anonymous_vote(&token, vote.clone())
            .await
            .unwrap();

        let tally = ballot_box.close_and_tally().await.unwrap();
        assert_eq!(tally.outcome, AcademicOutcome::Winner(vote));
        assert_eq!(
            tally.store.get_all(&tally.stream_id).await.unwrap().len(),
            4
        );
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

    #[test]
    fn event_sequence_increment_rejects_overflow() {
        assert_eq!(
            next_sequence(u64::MAX),
            Err(VotingFacadeError::Storage(StorageError::AppendFailed(
                "Event sequence overflow".into()
            )))
        );
    }
}
