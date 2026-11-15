use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;

use crate::{
    domain::{
        crypto::{GENESIS_HASH, HashedEventRecord, verify_chain},
        factory::{ElectionFactory, OpenElection},
        manifest::Manifest,
        storage::{ElectionEvent, Infrastructure, PersistedElection},
    },
    error::StorageError,
};

#[derive(Debug, Error)]
pub enum ElectionServiceError {
    #[error("{0}")]
    Invalid(String),
    #[error("Election not found")]
    NotFound,
    #[error("Election is closed")]
    Closed,
    #[error(transparent)]
    Infrastructure(#[from] StorageError),
}

pub struct ElectionService<I> {
    infrastructure: I,
}

impl<I: Clone> Clone for ElectionService<I> {
    fn clone(&self) -> Self {
        Self {
            infrastructure: self.infrastructure.clone(),
        }
    }
}

impl<I> ElectionService<I>
where
    I: Infrastructure<Value, Value> + Clone + Send + Sync + 'static,
{
    pub fn new(infrastructure: I) -> Self {
        Self { infrastructure }
    }

    pub async fn create_election(
        &self,
        manifest: Manifest,
    ) -> Result<String, ElectionServiceError> {
        ElectionFactory::create(manifest.clone())
            .map_err(|error| ElectionServiceError::Invalid(error.to_string()))?;
        let id = Uuid::new_v4().to_string();
        let manifest = serde_json::to_string(&manifest)
            .map_err(|error| ElectionServiceError::Invalid(error.to_string()))?;
        self.infrastructure
            .create_election(id.clone(), manifest)
            .await?;
        Ok(id)
    }

    pub async fn cast_vote(&self, id: &str, vote: Value) -> Result<(), ElectionServiceError> {
        let stored = self.load_open_election(id).await?;
        let mut election = restore_election(&stored)?;
        election
            .cast_vote(vote.clone())
            .map_err(ElectionServiceError::Invalid)?;

        let sequence = stored
            .vote_events
            .last()
            .map(|record| record.sequence.checked_add(1))
            .unwrap_or(Some(1))
            .ok_or_else(|| {
                ElectionServiceError::Infrastructure(StorageError::AppendFailed(
                    "Vote event sequence overflow".to_owned(),
                ))
            })?;
        let previous_hash = stored
            .vote_events
            .last()
            .map(|record| record.current_hash.as_str())
            .unwrap_or(GENESIS_HASH)
            .to_owned();
        let event = HashedEventRecord::new(
            sequence,
            chrono::Utc::now().to_rfc3339(),
            previous_hash,
            ElectionEvent::<Value, Value>::VoteCast(vote),
        )
        .map_err(ElectionServiceError::Infrastructure)?;
        self.infrastructure.append_vote_event(id, event).await?;
        Ok(())
    }

    pub async fn close_election(&self, id: &str) -> Result<Value, ElectionServiceError> {
        let stored = self.load_open_election(id).await?;
        let expected_vote_sequence = stored
            .vote_events
            .last()
            .map(|event| event.sequence)
            .unwrap_or(0);
        let election = restore_election(&stored)?;
        let outcome = election.close().map_err(ElectionServiceError::Invalid)?;
        self.infrastructure
            .close_election(id, expected_vote_sequence, outcome.clone())
            .await?;
        Ok(outcome)
    }

    async fn load_open_election(
        &self,
        id: &str,
    ) -> Result<PersistedElection, ElectionServiceError> {
        let stored = self
            .infrastructure
            .get_election(id)
            .await?
            .ok_or(ElectionServiceError::NotFound)?;
        if stored.is_closed {
            return Err(ElectionServiceError::Closed);
        }
        verify_events(&stored.vote_events)?;
        Ok(stored)
    }
}

fn verify_events(events: &[HashedEventRecord<Value, Value>]) -> Result<(), ElectionServiceError> {
    let valid = verify_chain(events).map_err(ElectionServiceError::Infrastructure)?;
    if !valid {
        return Err(ElectionServiceError::Infrastructure(
            StorageError::AppendFailed("Election vote event chain is invalid".to_owned()),
        ));
    }
    Ok(())
}

fn restore_election(stored: &PersistedElection) -> Result<OpenElection, ElectionServiceError> {
    let manifest = serde_json::from_str(&stored.manifest).map_err(|error| {
        ElectionServiceError::Infrastructure(StorageError::ReadFailed(error.to_string()))
    })?;
    let mut election = ElectionFactory::create(manifest).map_err(|error| {
        ElectionServiceError::Infrastructure(StorageError::ReadFailed(error.to_string()))
    })?;
    for event in &stored.vote_events {
        let ElectionEvent::VoteCast(vote) = &event.payload else {
            return Err(ElectionServiceError::Infrastructure(
                StorageError::ReadFailed("Unsupported election event".to_owned()),
            ));
        };
        election.cast_vote(vote.clone()).map_err(|error| {
            ElectionServiceError::Infrastructure(StorageError::ReadFailed(error))
        })?;
    }
    Ok(election)
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::{
        domain::storage::Infrastructure,
        infrastructure::db::{SqliteInfrastructure, init_db},
    };

    #[tokio::test]
    async fn election_service_uses_sqlite_through_the_shared_infrastructure() {
        let database_path = std::env::temp_dir().join(format!("eng-sft-{}.db", Uuid::new_v4()));
        let database_url = format!("sqlite://{}", database_path.display());
        let infrastructure = init_db(&database_url).await.unwrap();
        let service = ElectionService::new(infrastructure.clone());
        let manifest = serde_json::from_value(serde_json::json!({
            "type": "academic",
            "slates": ["A", "B"],
            "minimum_quorum": 1,
            "votes": []
        }))
        .unwrap();
        let id = service.create_election(manifest).await.unwrap();
        service
            .cast_vote(&id, serde_json::json!("A"))
            .await
            .unwrap();

        let stale_close = <SqliteInfrastructure as Infrastructure<Value, Value>>::close_election(
            &infrastructure,
            &id,
            0,
            serde_json::Value::Null,
        )
        .await;
        assert_eq!(
            stale_close,
            Err(StorageError::ConcurrencyConflict {
                expected: 0,
                got: 1
            })
        );
        drop(service);
        drop(infrastructure);

        let infrastructure = init_db(&database_url).await.unwrap();
        let service = ElectionService::new(infrastructure.clone());
        let outcome = service.close_election(&id).await.unwrap();
        assert_eq!(outcome, serde_json::json!({"Winner": "A"}));

        let election = <_ as Infrastructure<Value, Value>>::get_election(&infrastructure, &id)
            .await
            .unwrap()
            .unwrap();
        assert!(election.is_closed);
        assert_eq!(election.outcome, Some(outcome));
        assert_eq!(election.vote_events.len(), 1);
        drop(service);
        drop(infrastructure);
        std::fs::remove_file(database_path).unwrap();
    }
}
