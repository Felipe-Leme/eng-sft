use crate::error::StorageError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ElectionEvent<V, O> {
    ElectionCreated,
    VoteCast(V),
    ElectionClosed,
    TallyComputed(O),
}

pub trait EventStore<V, O> {
    /// Appends a new event to the end of the ledger.
    fn append(&mut self, event: ElectionEvent<V, O>) -> Result<(), StorageError>;

    /// Replays the event stream for auditing or state reconstruction.
    fn get_all(&self) -> Result<Vec<ElectionEvent<V, O>>, StorageError>;
}

pub struct MemoryEventStore<V, O> {
    ledger: Vec<ElectionEvent<V, O>>,
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
    fn append(&mut self, event: ElectionEvent<V, O>) -> Result<(), StorageError> {
        self.ledger.push(event);
        Ok(())
    }

    fn get_all(&self) -> Result<Vec<ElectionEvent<V, O>>, StorageError> {
        Ok(self.ledger.clone())
    }
}

#[cfg(test)]
mod tests {
    use crate::domain::academic::{AcademicOutcome, Slate};

    use super::*;

    #[test]
    fn test_append_and_retrieve_events() {
        let mut store = MemoryEventStore::new();

        let red = Slate::new("RED").unwrap();

        assert!(store.append(ElectionEvent::ElectionCreated).is_ok());
        assert!(store.append(ElectionEvent::VoteCast(red.clone())).is_ok());
        assert!(store.append(ElectionEvent::ElectionClosed).is_ok());
        assert!(
            store
                .append(ElectionEvent::TallyComputed(AcademicOutcome::Winner(
                    red.clone()
                )))
                .is_ok()
        );

        let events = store.get_all().unwrap();

        assert_eq!(events.len(), 4);
        assert_eq!(events[0], ElectionEvent::ElectionCreated);
        assert_eq!(events[1], ElectionEvent::VoteCast(red.clone()));
        assert_eq!(events[2], ElectionEvent::ElectionClosed);
        assert_eq!(
            events[3],
            ElectionEvent::TallyComputed(AcademicOutcome::Winner(red))
        );
    }

    #[test]
    fn test_store_is_isolated_from_mutations() {
        let mut store: MemoryEventStore<u32, u32> = MemoryEventStore::new();
        store.append(ElectionEvent::ElectionCreated).unwrap();

        let mut retrieved_events = store.get_all().unwrap();
        retrieved_events.clear();

        let unaffected_events = store.get_all().unwrap();
        assert_eq!(unaffected_events.len(), 1);
    }
}
