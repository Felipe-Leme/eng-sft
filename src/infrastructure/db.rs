use std::str::FromStr;

use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use sqlx::{
    Pool, Row, Sqlite,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};

use crate::{
    domain::{
        crypto::HashedEventRecord,
        storage::{ElectionEvent, EventRecord, Infrastructure, PersistedElection},
    },
    error::StorageError,
};

pub type DbPool = Pool<Sqlite>;

#[derive(Clone)]
pub struct SqliteInfrastructure {
    pool: DbPool,
}

impl SqliteInfrastructure {
    #[cfg(test)]
    pub(crate) async fn close(&self) {
        self.pool.close().await;
    }
}

pub async fn init_db(database_url: &str) -> Result<SqliteInfrastructure, sqlx::Error> {
    let options = SqliteConnectOptions::from_str(database_url)?
        .create_if_missing(true)
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS elections (
            id TEXT PRIMARY KEY NOT NULL,
            manifest TEXT NOT NULL CHECK (json_valid(manifest)),
            status TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'closed')),
            outcome TEXT CHECK (outcome IS NULL OR json_valid(outcome)),
            created_at TEXT NOT NULL
        )
        "#,
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS election_events (
            election_id TEXT NOT NULL,
            sequence INTEGER NOT NULL CHECK (sequence > 0),
            event_type TEXT NOT NULL CHECK (event_type = 'vote_cast'),
            payload TEXT NOT NULL CHECK (json_valid(payload)),
            prev_hash TEXT NOT NULL CHECK (length(prev_hash) = 64),
            current_hash TEXT NOT NULL CHECK (length(current_hash) = 64),
            timestamp TEXT NOT NULL,
            PRIMARY KEY (election_id, sequence),
            FOREIGN KEY (election_id) REFERENCES elections(id) ON DELETE CASCADE
        )
        "#,
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS infrastructure_events (
            stream_id TEXT NOT NULL,
            sequence INTEGER NOT NULL CHECK (sequence > 0),
            record TEXT NOT NULL CHECK (json_valid(record)),
            PRIMARY KEY (stream_id, sequence)
        )
        "#,
    )
    .execute(&pool)
    .await?;

    Ok(SqliteInfrastructure { pool })
}

fn storage_read_error(error: impl ToString) -> StorageError {
    StorageError::ReadFailed(error.to_string())
}

fn storage_write_error(error: impl ToString) -> StorageError {
    StorageError::AppendFailed(error.to_string())
}

#[async_trait::async_trait]
impl<V, O> Infrastructure<V, O> for SqliteInfrastructure
where
    V: Clone + Send + Sync + Serialize + DeserializeOwned + 'static,
    O: Clone + Send + Sync + Serialize + DeserializeOwned + 'static,
{
    async fn create_election(&self, id: String, manifest: String) -> Result<(), StorageError> {
        sqlx::query(
            "INSERT INTO elections (id, manifest, status, created_at) VALUES (?, ?, 'open', ?)",
        )
        .bind(id)
        .bind(manifest)
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(storage_write_error)?;
        Ok(())
    }

    async fn get_election(&self, id: &str) -> Result<Option<PersistedElection>, StorageError> {
        let Some(row) = sqlx::query("SELECT manifest, status, outcome FROM elections WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(storage_read_error)?
        else {
            return Ok(None);
        };

        let manifest: String = row.try_get("manifest").map_err(storage_read_error)?;
        let status: String = row.try_get("status").map_err(storage_read_error)?;
        let outcome_json: Option<String> = row.try_get("outcome").map_err(storage_read_error)?;
        let outcome = outcome_json
            .map(|json| serde_json::from_str(&json).map_err(storage_read_error))
            .transpose()?;

        let event_rows = sqlx::query(
            r#"
            SELECT sequence, timestamp, payload, prev_hash, current_hash
            FROM election_events
            WHERE election_id = ? AND event_type = 'vote_cast'
            ORDER BY sequence
            "#,
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await
        .map_err(storage_read_error)?;
        let vote_events = event_rows
            .into_iter()
            .map(|event_row| {
                let sequence: i64 = event_row.try_get("sequence").map_err(storage_read_error)?;
                let sequence = u64::try_from(sequence).map_err(storage_read_error)?;
                let timestamp: String =
                    event_row.try_get("timestamp").map_err(storage_read_error)?;
                let payload: String = event_row.try_get("payload").map_err(storage_read_error)?;
                let vote = serde_json::from_str(&payload).map_err(storage_read_error)?;
                let prev_hash: String =
                    event_row.try_get("prev_hash").map_err(storage_read_error)?;
                let current_hash: String = event_row
                    .try_get("current_hash")
                    .map_err(storage_read_error)?;
                Ok(HashedEventRecord {
                    sequence,
                    timestamp,
                    prev_hash,
                    current_hash,
                    payload: ElectionEvent::VoteCast(vote),
                })
            })
            .collect::<Result<Vec<_>, StorageError>>()?;

        Ok(Some(PersistedElection {
            manifest,
            is_closed: status == "closed",
            outcome,
            vote_events,
        }))
    }

    async fn append_vote_event(
        &self,
        id: &str,
        event: HashedEventRecord<Value, Value>,
    ) -> Result<(), StorageError> {
        if !event.verify().map_err(storage_write_error)? {
            return Err(storage_write_error("Vote event hash is invalid"));
        }
        let mut transaction = self.pool.begin().await.map_err(storage_write_error)?;
        let status: Option<String> =
            sqlx::query_scalar("SELECT status FROM elections WHERE id = ?")
                .bind(id)
                .fetch_optional(&mut *transaction)
                .await
                .map_err(storage_write_error)?;
        match status.as_deref() {
            None => return Err(StorageError::ElectionNotFound(id.to_owned())),
            Some("closed") => return Err(StorageError::ElectionClosed),
            Some("open") => {}
            Some(other) => {
                return Err(StorageError::ReadFailed(format!(
                    "Unsupported election status: {other}"
                )));
            }
        }

        let previous: Option<(i64, String)> = sqlx::query_as(
            "SELECT sequence, current_hash FROM election_events WHERE election_id = ? ORDER BY sequence DESC LIMIT 1",
        )
        .bind(id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(storage_read_error)?;
        let expected = match previous.as_ref() {
            None => 1,
            Some((sequence, _)) => u64::try_from(*sequence)
                .map_err(storage_read_error)?
                .checked_add(1)
                .ok_or_else(|| storage_read_error("Vote event sequence overflow"))?,
        };
        if event.sequence != expected {
            return Err(StorageError::ConcurrencyConflict {
                expected,
                got: event.sequence,
            });
        }
        let expected_previous = previous
            .as_ref()
            .map(|(_, hash)| hash.as_str())
            .unwrap_or(crate::domain::crypto::GENESIS_HASH);
        if event.prev_hash != expected_previous {
            return Err(storage_write_error(
                "Vote event does not extend the current hash chain",
            ));
        }
        let ElectionEvent::VoteCast(vote) = &event.payload else {
            return Err(storage_write_error("Unsupported election event"));
        };
        let payload = serde_json::to_string(vote).map_err(storage_write_error)?;
        let sequence = i64::try_from(event.sequence).map_err(storage_write_error)?;
        sqlx::query(
            r#"
            INSERT INTO election_events
                (election_id, sequence, event_type, payload, prev_hash, current_hash, timestamp)
            VALUES (?, ?, 'vote_cast', ?, ?, ?, ?)
            "#,
        )
        .bind(id)
        .bind(sequence)
        .bind(payload)
        .bind(event.prev_hash)
        .bind(event.current_hash)
        .bind(event.timestamp)
        .execute(&mut *transaction)
        .await
        .map_err(storage_write_error)?;
        transaction.commit().await.map_err(storage_write_error)?;
        Ok(())
    }

    async fn close_election(
        &self,
        id: &str,
        expected_vote_sequence: u64,
        outcome: Value,
    ) -> Result<(), StorageError> {
        let outcome = serde_json::to_string(&outcome).map_err(storage_write_error)?;
        let expected = i64::try_from(expected_vote_sequence).map_err(storage_write_error)?;
        let result = sqlx::query(
            r#"
            UPDATE elections SET status = 'closed', outcome = ?
            WHERE id = ? AND status = 'open'
                AND (SELECT COALESCE(MAX(sequence), 0)
                     FROM election_events WHERE election_id = ?) = ?
            "#,
        )
        .bind(outcome)
        .bind(id)
        .bind(id)
        .bind(expected)
        .execute(&self.pool)
        .await
        .map_err(storage_write_error)?;
        if result.rows_affected() == 1 {
            return Ok(());
        }

        match <Self as Infrastructure<V, O>>::get_election(self, id).await? {
            None => Err(StorageError::ElectionNotFound(id.to_owned())),
            Some(election) if election.is_closed => Err(StorageError::ElectionClosed),
            Some(election) => Err(StorageError::ConcurrencyConflict {
                expected: expected_vote_sequence,
                got: election
                    .vote_events
                    .last()
                    .map(|event| event.sequence)
                    .unwrap_or(0),
            }),
        }
    }

    async fn append(&self, stream_id: &str, record: EventRecord<V, O>) -> Result<(), StorageError> {
        let record_json = serde_json::to_string(&record).map_err(storage_write_error)?;
        let mut transaction = self.pool.begin().await.map_err(storage_write_error)?;
        let current: Option<i64> = sqlx::query_scalar(
            "SELECT sequence FROM infrastructure_events WHERE stream_id = ? ORDER BY sequence DESC LIMIT 1",
        )
        .bind(stream_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(storage_read_error)?;
        let expected = match current {
            None => 1,
            Some(sequence) => u64::try_from(sequence)
                .map_err(storage_read_error)?
                .checked_add(1)
                .ok_or_else(|| storage_read_error("Event sequence overflow"))?,
        };
        if record.sequence != expected {
            return Err(StorageError::ConcurrencyConflict {
                expected,
                got: record.sequence,
            });
        }
        let sequence = i64::try_from(record.sequence).map_err(storage_write_error)?;
        sqlx::query(
            "INSERT INTO infrastructure_events (stream_id, sequence, record) VALUES (?, ?, ?)",
        )
        .bind(stream_id)
        .bind(sequence)
        .bind(record_json)
        .execute(&mut *transaction)
        .await
        .map_err(storage_write_error)?;
        transaction.commit().await.map_err(storage_write_error)?;
        Ok(())
    }

    async fn get_all(&self, stream_id: &str) -> Result<Vec<EventRecord<V, O>>, StorageError> {
        let records = sqlx::query_scalar::<_, String>(
            "SELECT record FROM infrastructure_events WHERE stream_id = ? ORDER BY sequence",
        )
        .bind(stream_id)
        .fetch_all(&self.pool)
        .await
        .map_err(storage_read_error)?;
        records
            .into_iter()
            .map(|record| serde_json::from_str(&record).map_err(storage_read_error))
            .collect()
    }

    async fn get_since(
        &self,
        stream_id: &str,
        sequence: u64,
    ) -> Result<Vec<EventRecord<V, O>>, StorageError> {
        let sequence =
            i64::try_from(sequence).map_err(|error| storage_read_error(error.to_string()))?;
        let records = sqlx::query_scalar::<_, String>(
            "SELECT record FROM infrastructure_events WHERE stream_id = ? AND sequence >= ? ORDER BY sequence",
        )
        .bind(stream_id)
        .bind(sequence)
        .fetch_all(&self.pool)
        .await
        .map_err(storage_read_error)?;
        records
            .into_iter()
            .map(|record| serde_json::from_str(&record).map_err(storage_read_error))
            .collect()
    }

    async fn current_sequence(&self, stream_id: &str) -> Result<u64, StorageError> {
        let sequence: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(sequence), 0) FROM infrastructure_events WHERE stream_id = ?",
        )
        .bind(stream_id)
        .fetch_one(&self.pool)
        .await
        .map_err(storage_read_error)?;
        u64::try_from(sequence).map_err(storage_read_error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::crypto::{GENESIS_HASH, HashedEventRecord};

    fn vote_event(sequence: u64, previous_hash: &str) -> HashedEventRecord<Value, Value> {
        HashedEventRecord::new(
            sequence,
            format!("2026-01-01T00:00:{sequence:02}Z"),
            previous_hash.to_owned(),
            ElectionEvent::VoteCast(serde_json::json!("A")),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn initializes_a_database_with_constrained_persistence_schema() {
        let infrastructure = init_db("sqlite::memory:").await.unwrap();

        let election_columns: Vec<String> =
            sqlx::query_scalar("SELECT name FROM pragma_table_info('elections')")
                .fetch_all(&infrastructure.pool)
                .await
                .unwrap();
        let event_columns: Vec<String> =
            sqlx::query_scalar("SELECT name FROM pragma_table_info('election_events')")
                .fetch_all(&infrastructure.pool)
                .await
                .unwrap();

        assert!(election_columns.contains(&"manifest".to_owned()));
        assert!(election_columns.contains(&"status".to_owned()));
        assert!(election_columns.contains(&"outcome".to_owned()));
        assert!(event_columns.contains(&"sequence".to_owned()));
        assert!(event_columns.contains(&"prev_hash".to_owned()));
        assert!(event_columns.contains(&"current_hash".to_owned()));
    }

    #[tokio::test]
    async fn sqlite_infrastructure_persists_generic_event_streams() {
        let infrastructure = init_db("sqlite::memory:").await.unwrap();
        infrastructure
            .append(
                "stream",
                EventRecord::<String, String>::new(1, ElectionEvent::ElectionCreated),
            )
            .await
            .unwrap();
        infrastructure
            .append(
                "stream",
                EventRecord::<String, String>::new(2, ElectionEvent::VoteCast("vote".to_owned())),
            )
            .await
            .unwrap();

        let records: Vec<EventRecord<String, String>> =
            infrastructure.get_since("stream", 2).await.unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].sequence, 2);
        assert_eq!(
            <SqliteInfrastructure as Infrastructure<String, String>>::current_sequence(
                &infrastructure,
                "stream"
            )
            .await
            .unwrap(),
            2
        );
    }

    #[tokio::test]
    async fn sqlite_election_storage_enforces_event_integrity_and_lifecycle() {
        let infrastructure = init_db("sqlite::memory:").await.unwrap();
        let store: &dyn Infrastructure<Value, Value> = &infrastructure;

        assert_eq!(store.get_election("missing").await.unwrap(), None);
        assert_eq!(
            store
                .append_vote_event("missing", vote_event(1, GENESIS_HASH))
                .await,
            Err(StorageError::ElectionNotFound("missing".to_owned()))
        );
        assert_eq!(
            store
                .close_election("missing", 0, serde_json::Value::Null)
                .await,
            Err(StorageError::ElectionNotFound("missing".to_owned()))
        );

        store
            .create_election(
                "election".to_owned(),
                r#"{"type":"academic","slates":["A"],"votes":[]}"#.to_owned(),
            )
            .await
            .unwrap();
        assert!(matches!(
            store
                .create_election(
                    "election".to_owned(),
                    r#"{"type":"academic","slates":["A"],"votes":[]}"#.to_owned(),
                )
                .await,
            Err(StorageError::AppendFailed(_))
        ));

        let unsupported_event = HashedEventRecord::new(
            1,
            "2026-01-01T00:00:01Z".to_owned(),
            GENESIS_HASH.to_owned(),
            ElectionEvent::<Value, Value>::ElectionCreated,
        )
        .unwrap();
        assert!(matches!(
            store.append_vote_event("election", unsupported_event).await,
            Err(StorageError::AppendFailed(_))
        ));

        let mut invalid_hash = vote_event(1, GENESIS_HASH);
        invalid_hash.current_hash.push('0');
        assert!(matches!(
            store.append_vote_event("election", invalid_hash).await,
            Err(StorageError::AppendFailed(_))
        ));
        assert_eq!(
            store
                .append_vote_event("election", vote_event(2, GENESIS_HASH))
                .await,
            Err(StorageError::ConcurrencyConflict {
                expected: 1,
                got: 2,
            })
        );
        let incorrect_previous = "f".repeat(64);
        assert!(matches!(
            store
                .append_vote_event("election", vote_event(1, &incorrect_previous))
                .await,
            Err(StorageError::AppendFailed(_))
        ));

        let event = vote_event(1, GENESIS_HASH);
        let current_hash = event.current_hash.clone();
        store.append_vote_event("election", event).await.unwrap();
        let persisted = store.get_election("election").await.unwrap().unwrap();
        assert_eq!(persisted.vote_events.len(), 1);
        assert_eq!(persisted.vote_events[0].current_hash, current_hash);

        assert!(matches!(
            store
                .close_election("election", u64::MAX, serde_json::Value::Null)
                .await,
            Err(StorageError::AppendFailed(_))
        ));
        assert_eq!(
            store
                .close_election("election", 0, serde_json::Value::Null)
                .await,
            Err(StorageError::ConcurrencyConflict {
                expected: 0,
                got: 1,
            })
        );
        store
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
                .append_vote_event("election", vote_event(2, &current_hash))
                .await,
            Err(StorageError::ElectionClosed)
        );
        assert!(
            store
                .get_election("election")
                .await
                .unwrap()
                .unwrap()
                .is_closed
        );

        for sequence in 1..=2 {
            store
                .append(
                    "stream",
                    EventRecord::<Value, Value>::new(
                        sequence,
                        ElectionEvent::VoteCast(serde_json::json!(sequence)),
                    ),
                )
                .await
                .unwrap();
        }
        assert_eq!(store.get_all("stream").await.unwrap().len(), 2);
        assert_eq!(store.get_since("stream", 2).await.unwrap().len(), 1);
        assert!(matches!(
            store.get_since("stream", u64::MAX).await,
            Err(StorageError::ReadFailed(_))
        ));
    }
}
