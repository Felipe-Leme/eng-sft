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
        Self {
            sequence,
            timestamp: Utc::now(),
            payload,
        }
    }
}

pub trait EventStore<V, O> {
    /// Appends a new event to the end of the ledger.
    fn append(&mut self, record: EventRecord<V, O>) -> Result<(), StorageError>;

    /// Replays the event stream for auditing or state reconstruction.
    fn get_all(&self) -> Result<Vec<EventRecord<V, O>>, StorageError>;

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

    fn current_sequence(&self) -> u64 {
        self.ledger.last().map(|r| r.sequence).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use crate::domain::academic::{AcademicOutcome, Slate};

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
}
