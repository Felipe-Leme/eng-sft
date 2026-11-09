use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::StorageError;

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

pub trait EventStore<V, O> {
    /// Appends a new event to the end of the ledger.
    fn append(&mut self, record: EventRecord<V, O>) -> Result<(), StorageError>;

    /// Replays the event stream for auditing or state reconstruction.
    fn get_all(&self) -> Result<Vec<EventRecord<V, O>>, StorageError>;

    /// Replays events starting from a given sequence number (inclusive).
    fn get_since(&self, sequence: u64) -> Result<Vec<EventRecord<V, O>>, StorageError>;

    /// Returns the current maximum sequence number or 0 if empty.
    fn current_sequence(&self) -> u64;
}

pub struct MemoryEventStore<V, O> {
    ledger: Vec<EventRecord<V, O>>,
}

impl<V, O> Default for MemoryEventStore<V, O> {
    fn default() -> Self {
        Self { ledger: Vec::new() }
    }
}

impl<V, O> MemoryEventStore<V, O> {
    pub fn new() -> Self {
        Self::default()
    }
}

impl<V: Clone, O: Clone> EventStore<V, O> for MemoryEventStore<V, O> {
    fn append(&mut self, record: EventRecord<V, O>) -> Result<(), StorageError> {
        let expected = self.current_sequence() + 1;

        if record.sequence != expected {
            return Err(StorageError::ConcurrencyConflict {
                expected,
                got: record.sequence,
            });
        }

        self.ledger.push(record);
        Ok(())
    }

    fn get_all(&self) -> Result<Vec<EventRecord<V, O>>, StorageError> {
        Ok(self.ledger.clone())
    }

    fn get_since(&self, sequence: u64) -> Result<Vec<EventRecord<V, O>>, StorageError> {
        Ok(self
            .ledger
            .iter()
            .filter(|r| r.sequence >= sequence)
            .cloned()
            .collect())
    }

    fn current_sequence(&self) -> u64 {
        self.ledger.last().map(|r| r.sequence).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_append_enforces_sequence() {
        let mut store: MemoryEventStore<String, String> = MemoryEventStore::new();

        let event1 = EventRecord::new(1, ElectionEvent::ElectionCreated);
        assert!(store.append(event1).is_ok());

        let event3 = EventRecord::new(3, ElectionEvent::ElectionClosed);
        let err = store.append(event3).unwrap_err();

        assert_eq!(
            err,
            StorageError::ConcurrencyConflict {
                expected: 2,
                got: 3
            }
        );
        assert_eq!(store.current_sequence(), 1);
    }

    #[test]
    fn test_successful_sequential_append() {
        let mut store: MemoryEventStore<String, String> = MemoryEventStore::new();

        assert!(
            store
                .append(EventRecord::new(1, ElectionEvent::ElectionCreated))
                .is_ok()
        );
        assert!(
            store
                .append(EventRecord::new(2, ElectionEvent::VoteCast("v1".into())))
                .is_ok()
        );
        assert!(
            store
                .append(EventRecord::new(3, ElectionEvent::ElectionClosed))
                .is_ok()
        );

        let events = store.get_all().unwrap();
        assert_eq!(events.len(), 3);
        assert_eq!(store.current_sequence(), 3);
    }

    #[test]
    fn test_append_rejects_duplicate_sequence() {
        let mut store: MemoryEventStore<String, String> = MemoryEventStore::new();

        assert!(
            store
                .append(EventRecord::new(1, ElectionEvent::ElectionCreated))
                .is_ok()
        );

        let err = store
            .append(EventRecord::new(1, ElectionEvent::VoteCast("v1".into())))
            .unwrap_err();

        assert_eq!(
            err,
            StorageError::ConcurrencyConflict {
                expected: 2,
                got: 1
            }
        );
    }

    #[test]
    fn test_get_since_filters_correctly() {
        let mut store: MemoryEventStore<String, String> = MemoryEventStore::new();

        for idx in 1..6 {
            let event = if idx == 1 {
                ElectionEvent::ElectionCreated
            } else {
                ElectionEvent::VoteCast(format!("v{}", idx))
            };
            store.append(EventRecord::new(idx, event)).unwrap();
        }

        let subset = store.get_since(3).unwrap();
        assert_eq!(subset.len(), 3);
        assert_eq!(subset[0].sequence, 3);
        assert_eq!(subset[2].sequence, 5);

        let empty = store.get_since(10).unwrap();
        assert!(empty.is_empty());

        let all = store.get_since(1).unwrap();
        assert_eq!(all.len(), 5);
    }
}
