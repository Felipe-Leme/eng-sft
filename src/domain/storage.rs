use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{domain::crypto::HashedEventRecord, error::StorageError};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ElectionEvent<V, O> {
    ElectionCreated,
    VoteCast(V),
    ElectionClosed,
    TallyComputed(O),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventRecord<V, O> {
    pub sequence: u64,
    pub timestamp: DateTime<Utc>,
    pub payload: ElectionEvent<V, O>,
}

impl<V, O> EventRecord<V, O> {
    pub fn new(sequence: u64, payload: ElectionEvent<V, O>) -> Self {
        Self::with_timestamp(sequence, Utc::now(), payload)
    }

    pub fn with_timestamp(
        sequence: u64,
        timestamp: DateTime<Utc>,
        payload: ElectionEvent<V, O>,
    ) -> Self {
        Self {
            sequence,
            timestamp,
            payload,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PersistedElection {
    pub manifest: String,
    pub is_closed: bool,
    pub outcome: Option<Value>,
    pub vote_events: Vec<HashedEventRecord<Value, Value>>,
}

#[async_trait::async_trait]
pub trait Infrastructure<V, O>: Send + Sync
where
    V: Clone + Send + Sync + 'static,
    O: Clone + Send + Sync + 'static,
{
    async fn create_election(&self, id: String, manifest: String) -> Result<(), StorageError>;

    async fn get_election(&self, id: &str) -> Result<Option<PersistedElection>, StorageError>;

    async fn append_vote_event(
        &self,
        id: &str,
        event: HashedEventRecord<Value, Value>,
    ) -> Result<(), StorageError>;

    async fn close_election(
        &self,
        id: &str,
        expected_vote_sequence: u64,
        outcome: Value,
    ) -> Result<(), StorageError>;

    async fn append(&self, stream_id: &str, record: EventRecord<V, O>) -> Result<(), StorageError>;

    async fn get_all(&self, stream_id: &str) -> Result<Vec<EventRecord<V, O>>, StorageError>;

    async fn get_since(
        &self,
        stream_id: &str,
        sequence: u64,
    ) -> Result<Vec<EventRecord<V, O>>, StorageError>;

    async fn current_sequence(&self, stream_id: &str) -> Result<u64, StorageError>;
}
