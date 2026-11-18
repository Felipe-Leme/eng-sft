use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;

use crate::{
    domain::{
        ElectionSummary, ElectionVote,
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
        name: String,
        manifest: Manifest,
    ) -> Result<String, ElectionServiceError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(ElectionServiceError::Invalid(
                "Election name cannot be empty".to_owned(),
            ));
        }
        ElectionFactory::create(manifest.clone())
            .map_err(|error| ElectionServiceError::Invalid(error.to_string()))?;
        let id = Uuid::new_v4().to_string();
        let manifest = serde_json::to_string(&manifest)
            .map_err(|error| ElectionServiceError::Invalid(error.to_string()))?;
        self.infrastructure
            .create_election(id.clone(), name.to_owned(), manifest)
            .await?;
        Ok(id)
    }

    pub async fn list_elections(&self) -> Result<Vec<ElectionSummary>, ElectionServiceError> {
        self.infrastructure
            .list_elections()
            .await?
            .into_iter()
            .map(|election| {
                Ok(ElectionSummary {
                    id: election.id,
                    name: election.name,
                    manifest: serde_json::from_str::<Manifest>(&election.manifest).map_err(
                        |error| {
                            ElectionServiceError::Infrastructure(StorageError::ReadFailed(
                                error.to_string(),
                            ))
                        },
                    )?,
                    is_closed: election.is_closed,
                    outcome: election.outcome,
                })
            })
            .collect()
    }

    pub async fn cast_vote(
        &self,
        id: &str,
        voter_id: Uuid,
        vote: ElectionVote,
    ) -> Result<(), ElectionServiceError> {
        let stored = self.load_open_election(id).await?;
        let mut election = restore_election(&stored)?;
        election
            .cast_vote(vote.clone())
            .map_err(ElectionServiceError::Invalid)?;
        let vote = serde_json::to_value(vote)
            .map_err(|error| ElectionServiceError::Invalid(error.to_string()))?;

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
        self.infrastructure
            .append_vote_event(id, voter_id, event)
            .await?;
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
    let manifest = serde_json::from_str::<Manifest>(&stored.manifest).map_err(|error| {
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
        let vote = serde_json::from_value::<ElectionVote>(vote.clone()).map_err(|error| {
            ElectionServiceError::Infrastructure(StorageError::ReadFailed(error.to_string()))
        })?;
        election.cast_vote(vote).map_err(|error| {
            ElectionServiceError::Infrastructure(StorageError::ReadFailed(error))
        })?;
    }
    Ok(election)
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::{
        domain::{
            ElectionVote,
            academic::Slate,
            crypto::HashedEventRecord,
            storage::{ElectionEvent, Infrastructure},
        },
        infrastructure::db::{SqliteInfrastructure, init_db},
    };

    fn academic_vote(choice: &str) -> ElectionVote {
        ElectionVote::Academic(Slate::new(choice).unwrap())
    }

    #[test]
    fn rejects_tampered_history_and_invalid_persisted_elections() {
        let valid_event = HashedEventRecord::new(
            1,
            "2026-01-01T00:00:01Z".to_owned(),
            GENESIS_HASH.to_owned(),
            ElectionEvent::<Value, Value>::VoteCast(serde_json::json!("A")),
        )
        .unwrap();
        let mut tampered_event = valid_event.clone();
        tampered_event.current_hash.push('0');
        assert!(matches!(
            verify_events(&[tampered_event]),
            Err(ElectionServiceError::Infrastructure(
                StorageError::AppendFailed(_)
            ))
        ));

        let invalid_manifest = PersistedElection {
            name: "invalid".to_owned(),
            manifest: "not JSON".to_owned(),
            is_closed: false,
            outcome: None,
            vote_events: Vec::new(),
        };
        assert!(matches!(
            restore_election(&invalid_manifest),
            Err(ElectionServiceError::Infrastructure(
                StorageError::ReadFailed(_)
            ))
        ));

        let unsupported_event = HashedEventRecord::new(
            1,
            "2026-01-01T00:00:01Z".to_owned(),
            GENESIS_HASH.to_owned(),
            ElectionEvent::<Value, Value>::ElectionCreated,
        )
        .unwrap();
        let unsupported_history = PersistedElection {
            name: "unsupported".to_owned(),
            manifest: r#"{"type":"academic","slates":["A"],"votes":[]}"#.to_owned(),
            is_closed: false,
            outcome: None,
            vote_events: vec![unsupported_event],
        };
        assert!(matches!(
            restore_election(&unsupported_history),
            Err(ElectionServiceError::Infrastructure(
                StorageError::ReadFailed(_)
            ))
        ));

        let invalid_vote = HashedEventRecord::new(
            1,
            "2026-01-01T00:00:01Z".to_owned(),
            GENESIS_HASH.to_owned(),
            ElectionEvent::<Value, Value>::VoteCast(serde_json::json!("unknown")),
        )
        .unwrap();
        let invalid_history = PersistedElection {
            name: "invalid vote".to_owned(),
            manifest: r#"{"type":"academic","slates":["A"],"votes":[]}"#.to_owned(),
            is_closed: false,
            outcome: None,
            vote_events: vec![invalid_vote],
        };
        assert!(matches!(
            restore_election(&invalid_history),
            Err(ElectionServiceError::Infrastructure(
                StorageError::ReadFailed(_)
            ))
        ));
    }

    #[tokio::test]
    async fn service_rejects_invalid_missing_and_closed_elections() {
        let infrastructure = crate::infrastructure::MemoryInfrastructure::<Value, Value>::new();
        let service = ElectionService::new(infrastructure);
        let invalid_manifest = serde_json::from_value(serde_json::json!({
            "type": "academic",
            "slates": [],
            "votes": []
        }))
        .unwrap();
        assert!(matches!(
            service
                .create_election("Invalid election".to_owned(), invalid_manifest)
                .await,
            Err(ElectionServiceError::Invalid(_))
        ));

        let manifest = serde_json::from_value(serde_json::json!({
            "type": "academic",
            "slates": ["A"],
            "votes": []
        }))
        .unwrap();
        let id = service
            .create_election("Election".to_owned(), manifest)
            .await
            .unwrap();
        let duplicate_manifest = serde_json::from_value(serde_json::json!({
            "type": "academic",
            "slates": ["A"],
            "votes": []
        }))
        .unwrap();
        assert!(matches!(
            service
                .create_election(" election ".to_owned(), duplicate_manifest)
                .await,
            Err(ElectionServiceError::Infrastructure(
                StorageError::DuplicateElectionName
            ))
        ));
        assert!(matches!(
            service
                .cast_vote("missing", Uuid::from_u128(1), academic_vote("A"))
                .await,
            Err(ElectionServiceError::NotFound)
        ));
        assert!(matches!(
            service
                .cast_vote(&id, Uuid::from_u128(1), academic_vote("unknown"))
                .await,
            Err(ElectionServiceError::Invalid(_))
        ));

        service
            .cast_vote(&id, Uuid::from_u128(1), academic_vote("A"))
            .await
            .unwrap();
        assert!(matches!(
            service
                .cast_vote(&id, Uuid::from_u128(1), academic_vote("A"))
                .await,
            Err(ElectionServiceError::Infrastructure(
                StorageError::VoterAlreadyVoted
            ))
        ));
        service.close_election(&id).await.unwrap();
        assert!(matches!(
            service
                .cast_vote(&id, Uuid::from_u128(1), academic_vote("A"))
                .await,
            Err(ElectionServiceError::Closed)
        ));
        assert!(matches!(
            service.close_election(&id).await,
            Err(ElectionServiceError::Closed)
        ));
    }

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
        let id = service
            .create_election("SQLite election".to_owned(), manifest)
            .await
            .unwrap();
        let elections = service.list_elections().await.unwrap();
        assert_eq!(elections.len(), 1);
        assert_eq!(elections[0].id, id);
        assert_eq!(elections[0].name, "SQLite election");
        assert!(!elections[0].is_closed);
        assert!(elections[0].outcome.is_none());
        service
            .cast_vote(&id, Uuid::from_u128(2), academic_vote("A"))
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
        infrastructure.close().await;
        drop(infrastructure);

        let infrastructure = init_db(&database_url).await.unwrap();
        let service = ElectionService::new(infrastructure.clone());
        let outcome = service.close_election(&id).await.unwrap();
        assert_eq!(outcome, serde_json::json!({"Winner": "A"}));
        let elections = service.list_elections().await.unwrap();
        assert!(elections[0].is_closed);
        assert_eq!(elections[0].outcome, Some(outcome.clone()));

        let election = <_ as Infrastructure<Value, Value>>::get_election(&infrastructure, &id)
            .await
            .unwrap()
            .unwrap();
        assert!(election.is_closed);
        assert_eq!(election.outcome, Some(outcome));
        assert_eq!(election.vote_events.len(), 1);
        drop(service);
        infrastructure.close().await;
        drop(infrastructure);
        std::fs::remove_file(database_path).unwrap();
    }
}
