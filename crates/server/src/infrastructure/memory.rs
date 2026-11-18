use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use serde_json::Value;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::{
    domain::{
        crypto::HashedEventRecord,
        storage::{EventRecord, Infrastructure, PersistedElection, PersistedElectionSummary},
    },
    error::StorageError,
};

struct MemoryState<V, O> {
    elections: HashMap<String, PersistedElection>,
    voters: HashSet<(String, Uuid)>,
    event_streams: HashMap<String, Vec<EventRecord<V, O>>>,
}

pub struct MemoryInfrastructure<V, O> {
    state: Arc<Mutex<MemoryState<V, O>>>,
}

impl<V, O> Clone for MemoryInfrastructure<V, O> {
    fn clone(&self) -> Self {
        Self {
            state: Arc::clone(&self.state),
        }
    }
}

impl<V, O> Default for MemoryInfrastructure<V, O> {
    fn default() -> Self {
        Self {
            state: Arc::new(Mutex::new(MemoryState {
                elections: HashMap::new(),
                voters: HashSet::new(),
                event_streams: HashMap::new(),
            })),
        }
    }
}

impl<V, O> MemoryInfrastructure<V, O> {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait::async_trait]
impl<V, O> Infrastructure<V, O> for MemoryInfrastructure<V, O>
where
    V: Clone + Send + Sync + 'static,
    O: Clone + Send + Sync + 'static,
{
    async fn create_election(
        &self,
        id: String,
        name: String,
        manifest: String,
    ) -> Result<(), StorageError> {
        let mut state = self.state.lock().await;
        if state.elections.contains_key(&id) {
            return Err(StorageError::ElectionAlreadyExists(id));
        }
        if state
            .elections
            .values()
            .any(|election| election.name.eq_ignore_ascii_case(&name))
        {
            return Err(StorageError::DuplicateElectionName);
        }
        state.elections.insert(
            id,
            PersistedElection {
                name,
                manifest,
                is_closed: false,
                outcome: None,
                vote_events: Vec::new(),
            },
        );
        Ok(())
    }

    async fn get_election(&self, id: &str) -> Result<Option<PersistedElection>, StorageError> {
        Ok(self.state.lock().await.elections.get(id).cloned())
    }

    async fn list_elections(&self) -> Result<Vec<PersistedElectionSummary>, StorageError> {
        let state = self.state.lock().await;
        let mut elections: Vec<_> = state
            .elections
            .iter()
            .map(|(id, election)| PersistedElectionSummary {
                id: id.clone(),
                name: election.name.clone(),
                manifest: election.manifest.clone(),
                is_closed: election.is_closed,
                outcome: election.outcome.clone(),
            })
            .collect();
        elections.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(elections)
    }

    async fn append_vote_event(
        &self,
        id: &str,
        voter_id: Uuid,
        event: HashedEventRecord<Value, Value>,
    ) -> Result<(), StorageError> {
        let mut state = self.state.lock().await;
        if state.voters.contains(&(id.to_owned(), voter_id)) {
            return Err(StorageError::VoterAlreadyVoted);
        }
        let election = state
            .elections
            .get_mut(id)
            .ok_or_else(|| StorageError::ElectionNotFound(id.to_owned()))?;
        if election.is_closed {
            return Err(StorageError::ElectionClosed);
        }

        let expected = election
            .vote_events
            .last()
            .map(|event| event.sequence.checked_add(1))
            .unwrap_or(Some(1))
            .ok_or_else(|| StorageError::AppendFailed("Vote event sequence overflow".to_owned()))?;
        if event.sequence != expected {
            return Err(StorageError::ConcurrencyConflict {
                expected,
                got: event.sequence,
            });
        }
        if !event
            .verify()
            .map_err(|error| StorageError::AppendFailed(error.to_string()))?
        {
            return Err(StorageError::AppendFailed(
                "Vote event hash is invalid".to_owned(),
            ));
        }
        let expected_previous = election
            .vote_events
            .last()
            .map(|record| record.current_hash.as_str())
            .unwrap_or(crate::domain::crypto::GENESIS_HASH);
        if event.prev_hash != expected_previous {
            return Err(StorageError::AppendFailed(
                "Vote event does not extend the current hash chain".to_owned(),
            ));
        }

        election.vote_events.push(event);
        state.voters.insert((id.to_owned(), voter_id));
        Ok(())
    }

    async fn close_election(
        &self,
        id: &str,
        expected_vote_sequence: u64,
        outcome: Value,
    ) -> Result<(), StorageError> {
        let mut state = self.state.lock().await;
        let election = state
            .elections
            .get_mut(id)
            .ok_or_else(|| StorageError::ElectionNotFound(id.to_owned()))?;
        if election.is_closed {
            return Err(StorageError::ElectionClosed);
        }
        let actual_sequence = election
            .vote_events
            .last()
            .map(|event| event.sequence)
            .unwrap_or(0);
        if actual_sequence != expected_vote_sequence {
            return Err(StorageError::ConcurrencyConflict {
                expected: expected_vote_sequence,
                got: actual_sequence,
            });
        }
        election.is_closed = true;
        election.outcome = Some(outcome);
        Ok(())
    }

    async fn append(&self, stream_id: &str, record: EventRecord<V, O>) -> Result<(), StorageError> {
        let mut state = self.state.lock().await;
        let ledger = state.event_streams.entry(stream_id.to_owned()).or_default();
        let expected = ledger
            .last()
            .map(|record| record.sequence.checked_add(1))
            .unwrap_or(Some(1))
            .ok_or_else(|| StorageError::AppendFailed("Event sequence overflow".to_owned()))?;
        if record.sequence != expected {
            return Err(StorageError::ConcurrencyConflict {
                expected,
                got: record.sequence,
            });
        }
        ledger.push(record);
        Ok(())
    }

    async fn get_all(&self, stream_id: &str) -> Result<Vec<EventRecord<V, O>>, StorageError> {
        Ok(self
            .state
            .lock()
            .await
            .event_streams
            .get(stream_id)
            .cloned()
            .unwrap_or_default())
    }

    async fn get_since(
        &self,
        stream_id: &str,
        sequence: u64,
    ) -> Result<Vec<EventRecord<V, O>>, StorageError> {
        Ok(self
            .get_all(stream_id)
            .await?
            .into_iter()
            .filter(|record| record.sequence >= sequence)
            .collect())
    }

    async fn current_sequence(&self, stream_id: &str) -> Result<u64, StorageError> {
        Ok(self
            .state
            .lock()
            .await
            .event_streams
            .get(stream_id)
            .and_then(|ledger| ledger.last())
            .map(|record| record.sequence)
            .unwrap_or(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        crypto::{GENESIS_HASH, HashedEventRecord},
        storage::ElectionEvent,
    };

    fn vote_event(sequence: u64, previous_hash: &str) -> HashedEventRecord<Value, Value> {
        HashedEventRecord::new(
            sequence,
            format!("2026-01-01T00:00:{sequence:02}Z"),
            previous_hash.to_owned(),
            ElectionEvent::VoteCast(serde_json::json!("A")),
        )
        .unwrap()
    }

    fn voter_id() -> Uuid {
        Uuid::from_u128(1)
    }

    #[tokio::test]
    async fn memory_infrastructure_enforces_sequential_event_appends() {
        let store = MemoryInfrastructure::<String, String>::new();

        store
            .append("test", EventRecord::new(1, ElectionEvent::ElectionCreated))
            .await
            .unwrap();
        let error = store
            .append("test", EventRecord::new(3, ElectionEvent::ElectionClosed))
            .await
            .unwrap_err();

        assert_eq!(
            error,
            StorageError::ConcurrencyConflict {
                expected: 2,
                got: 3
            }
        );
        assert_eq!(store.current_sequence("test").await.unwrap(), 1);
    }

    #[tokio::test]
    async fn memory_infrastructure_keeps_independent_streams() {
        let store = MemoryInfrastructure::<String, String>::new();
        for sequence in 1..4 {
            store
                .append(
                    "first",
                    EventRecord::new(sequence, ElectionEvent::VoteCast(sequence.to_string())),
                )
                .await
                .unwrap();
        }
        store
            .append(
                "second",
                EventRecord::new(1, ElectionEvent::ElectionCreated),
            )
            .await
            .unwrap();

        assert_eq!(store.get_since("first", 2).await.unwrap().len(), 2);
        assert_eq!(store.get_all("second").await.unwrap().len(), 1);
        assert_eq!(store.get_all("missing").await.unwrap().len(), 0);
    }

    #[tokio::test]
    async fn election_storage_validates_event_chains_and_closed_state() {
        let store = MemoryInfrastructure::<Value, Value>::new();
        let clone = store.clone();
        assert_eq!(store.get_election("missing").await.unwrap(), None);
        assert_eq!(
            store
                .close_election("missing", 0, serde_json::Value::Null)
                .await,
            Err(StorageError::ElectionNotFound("missing".to_owned()))
        );
        assert_eq!(
            store
                .append_vote_event("missing", voter_id(), vote_event(1, GENESIS_HASH))
                .await,
            Err(StorageError::ElectionNotFound("missing".to_owned()))
        );

        store
            .create_election(
                "election".to_owned(),
                "Election".to_owned(),
                "{}".to_owned(),
            )
            .await
            .unwrap();
        assert_eq!(
            clone
                .create_election("election".to_owned(), "Another".to_owned(), "{}".to_owned())
                .await,
            Err(StorageError::ElectionAlreadyExists("election".to_owned()))
        );
        assert_eq!(
            clone
                .create_election(
                    "another-id".to_owned(),
                    "eLECTION".to_owned(),
                    "{}".to_owned()
                )
                .await,
            Err(StorageError::DuplicateElectionName)
        );

        let mut invalid_hash = vote_event(1, GENESIS_HASH);
        invalid_hash.current_hash.push('0');
        assert_eq!(
            store
                .append_vote_event("election", voter_id(), invalid_hash)
                .await,
            Err(StorageError::AppendFailed(
                "Vote event hash is invalid".to_owned()
            ))
        );
        assert_eq!(
            store
                .append_vote_event("election", voter_id(), vote_event(2, GENESIS_HASH))
                .await,
            Err(StorageError::ConcurrencyConflict {
                expected: 1,
                got: 2,
            })
        );

        let incorrect_previous = "f".repeat(64);
        assert_eq!(
            store
                .append_vote_event("election", voter_id(), vote_event(1, &incorrect_previous))
                .await,
            Err(StorageError::AppendFailed(
                "Vote event does not extend the current hash chain".to_owned()
            ))
        );

        let event = vote_event(1, GENESIS_HASH);
        let current_hash = event.current_hash.clone();
        store
            .append_vote_event("election", voter_id(), event)
            .await
            .unwrap();
        assert_eq!(
            store
                .append_vote_event("election", voter_id(), vote_event(2, &current_hash),)
                .await,
            Err(StorageError::VoterAlreadyVoted)
        );
        assert_eq!(
            clone
                .close_election("election", 0, serde_json::Value::Null)
                .await,
            Err(StorageError::ConcurrencyConflict {
                expected: 0,
                got: 1,
            })
        );
        clone
            .close_election("election", 1, serde_json::json!({"Winner": "A"}))
            .await
            .unwrap();
        assert_eq!(
            store
                .close_election("election", 1, serde_json::Value::Null)
                .await,
            Err(StorageError::ElectionClosed)
        );
        assert_eq!(
            store
                .append_vote_event("election", Uuid::from_u128(2), vote_event(2, &current_hash))
                .await,
            Err(StorageError::ElectionClosed)
        );

        let persisted = store.get_election("election").await.unwrap().unwrap();
        assert!(persisted.is_closed);
        assert_eq!(persisted.vote_events.len(), 1);
        assert_eq!(persisted.outcome, Some(serde_json::json!({"Winner": "A"})));
    }

    #[tokio::test]
    async fn memory_storage_rejects_sequence_overflow() {
        let store = MemoryInfrastructure::<Value, Value>::new();
        store
            .create_election(
                "election".to_owned(),
                "Election".to_owned(),
                "{}".to_owned(),
            )
            .await
            .unwrap();
        {
            let mut state = store.state.lock().await;
            state
                .elections
                .get_mut("election")
                .unwrap()
                .vote_events
                .push(vote_event(u64::MAX, GENESIS_HASH));
            state.event_streams.insert(
                "stream".to_owned(),
                vec![EventRecord::new(
                    u64::MAX,
                    ElectionEvent::<Value, Value>::ElectionCreated,
                )],
            );
        }

        assert_eq!(
            store
                .append_vote_event("election", voter_id(), vote_event(u64::MAX, GENESIS_HASH))
                .await,
            Err(StorageError::AppendFailed(
                "Vote event sequence overflow".to_owned()
            ))
        );
        assert_eq!(
            store
                .append(
                    "stream",
                    EventRecord::new(u64::MAX, ElectionEvent::<Value, Value>::ElectionCreated,),
                )
                .await,
            Err(StorageError::AppendFailed(
                "Event sequence overflow".to_owned()
            ))
        );
    }
}
