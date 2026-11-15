use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{Row, SqliteConnection};
use uuid::Uuid;

use crate::{
    domain::{
        crypto::{GENESIS_HASH, HashedEventRecord, verify_chain},
        factory::ElectionFactory,
        manifest::Manifest,
        storage::ElectionEvent,
    },
    infrastructure::db::DbPool,
};

pub fn app_router(pool: DbPool) -> Router {
    Router::new()
        .route("/api/elections", post(create_election))
        .route("/api/elections/{id}/votes", post(cast_vote))
        .route("/api/elections/{id}/close", post(close_election))
        .with_state(pool)
}

#[derive(Serialize, Deserialize)]
pub struct CreateElectionResponse {
    pub id: String,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

#[derive(Debug)]
struct ApiError(StatusCode, String);

impl ApiError {
    fn internal(message: impl ToString) -> Self {
        Self(StatusCode::INTERNAL_SERVER_ERROR, message.to_string())
    }

    fn bad_request(message: impl ToString) -> Self {
        Self(StatusCode::BAD_REQUEST, message.to_string())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let Self(status, message) = self;
        (status, Json(ErrorResponse { error: message })).into_response()
    }
}

async fn create_election(
    State(pool): State<DbPool>,
    Json(manifest): Json<Manifest>,
) -> Result<impl IntoResponse, ApiError> {
    ElectionFactory::create(manifest.clone()).map_err(ApiError::bad_request)?;
    let id = Uuid::new_v4().to_string();
    let manifest = serde_json::to_string(&manifest).map_err(ApiError::internal)?;

    sqlx::query(
        "INSERT INTO elections (id, manifest, status, created_at) VALUES (?, ?, 'open', ?)",
    )
    .bind(&id)
    .bind(manifest)
    .bind(Utc::now().to_rfc3339())
    .execute(&pool)
    .await
    .map_err(ApiError::internal)?;

    Ok((StatusCode::CREATED, Json(CreateElectionResponse { id })))
}

async fn cast_vote(
    Path(id): Path<String>,
    State(pool): State<DbPool>,
    Json(vote): Json<Value>,
) -> Result<StatusCode, ApiError> {
    let mut transaction = pool.begin().await.map_err(ApiError::internal)?;
    let stored = load_election(&mut transaction, &id).await?;
    if stored.status != "open" {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "Election is closed".to_owned(),
        ));
    }

    let events = load_vote_events(&mut transaction, &id).await?;
    verify_events(&events)?;
    let mut election = restore_election(&stored.manifest, &events)?;
    election
        .cast_vote(vote.clone())
        .map_err(ApiError::bad_request)?;

    let sequence = events.last().map(|record| record.sequence + 1).unwrap_or(1);
    let previous_hash = events
        .last()
        .map(|record| record.current_hash.as_str())
        .unwrap_or(GENESIS_HASH)
        .to_owned();
    let timestamp = Utc::now().to_rfc3339();
    let event = HashedEventRecord::new(
        sequence,
        timestamp.clone(),
        previous_hash,
        ElectionEvent::<Value, Value>::VoteCast(vote.clone()),
    )
    .map_err(ApiError::internal)?;
    let sequence = i64::try_from(sequence).map_err(ApiError::internal)?;
    let payload = serde_json::to_string(&vote).map_err(ApiError::internal)?;

    sqlx::query(
        r#"
        INSERT INTO election_events
            (election_id, sequence, event_type, payload, prev_hash, current_hash, timestamp)
        VALUES (?, ?, 'vote_cast', ?, ?, ?, ?)
        "#,
    )
    .bind(&id)
    .bind(sequence)
    .bind(payload)
    .bind(event.prev_hash)
    .bind(event.current_hash)
    .bind(timestamp)
    .execute(&mut *transaction)
    .await
    .map_err(ApiError::internal)?;

    transaction.commit().await.map_err(ApiError::internal)?;
    Ok(StatusCode::OK)
}

async fn close_election(
    Path(id): Path<String>,
    State(pool): State<DbPool>,
) -> Result<Json<Value>, ApiError> {
    let mut transaction = pool.begin().await.map_err(ApiError::internal)?;
    let stored = load_election(&mut transaction, &id).await?;
    if stored.status != "open" {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "Election is closed".to_owned(),
        ));
    }

    let events = load_vote_events(&mut transaction, &id).await?;
    verify_events(&events)?;
    let election = restore_election(&stored.manifest, &events)?;
    let result = election.close().map_err(ApiError::bad_request)?;
    let outcome = serde_json::to_string(&result).map_err(ApiError::internal)?;

    sqlx::query("UPDATE elections SET status = 'closed', outcome = ? WHERE id = ?")
        .bind(outcome)
        .bind(&id)
        .execute(&mut *transaction)
        .await
        .map_err(ApiError::internal)?;

    transaction.commit().await.map_err(ApiError::internal)?;
    Ok(Json(result))
}

struct StoredElection {
    manifest: String,
    status: String,
}

async fn load_election(
    connection: &mut SqliteConnection,
    id: &str,
) -> Result<StoredElection, ApiError> {
    let row = sqlx::query("SELECT manifest, status FROM elections WHERE id = ?")
        .bind(id)
        .fetch_optional(connection)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError(StatusCode::NOT_FOUND, "Election not found".to_owned()))?;

    Ok(StoredElection {
        manifest: row.try_get("manifest").map_err(ApiError::internal)?,
        status: row.try_get("status").map_err(ApiError::internal)?,
    })
}

type VoteEvent = HashedEventRecord<Value, Value>;

async fn load_vote_events(
    connection: &mut SqliteConnection,
    election_id: &str,
) -> Result<Vec<VoteEvent>, ApiError> {
    let rows = sqlx::query(
        r#"
        SELECT sequence, timestamp, payload, prev_hash, current_hash
        FROM election_events
        WHERE election_id = ? AND event_type = 'vote_cast'
        ORDER BY sequence
        "#,
    )
    .bind(election_id)
    .fetch_all(connection)
    .await
    .map_err(ApiError::internal)?;

    rows.into_iter()
        .map(|row| {
            let sequence: i64 = row.try_get("sequence").map_err(ApiError::internal)?;
            let sequence = u64::try_from(sequence).map_err(ApiError::internal)?;
            let timestamp: String = row.try_get("timestamp").map_err(ApiError::internal)?;
            let payload: String = row.try_get("payload").map_err(ApiError::internal)?;
            let vote: Value = serde_json::from_str(&payload).map_err(ApiError::internal)?;
            let prev_hash: String = row.try_get("prev_hash").map_err(ApiError::internal)?;
            let current_hash: String = row.try_get("current_hash").map_err(ApiError::internal)?;

            Ok(VoteEvent {
                sequence,
                timestamp,
                prev_hash,
                current_hash,
                payload: ElectionEvent::VoteCast(vote),
            })
        })
        .collect()
}

fn verify_events(events: &[VoteEvent]) -> Result<(), ApiError> {
    let is_valid = verify_chain(events).map_err(ApiError::internal)?;
    if !is_valid {
        return Err(ApiError::internal("Election vote event chain is invalid"));
    }
    Ok(())
}

fn restore_election(
    manifest: &str,
    events: &[VoteEvent],
) -> Result<crate::domain::factory::OpenElection, ApiError> {
    let manifest = serde_json::from_str(manifest).map_err(ApiError::internal)?;
    let mut election = ElectionFactory::create(manifest).map_err(ApiError::internal)?;
    for event in events {
        let ElectionEvent::VoteCast(vote) = &event.payload else {
            return Err(ApiError::internal("Unsupported election event"));
        };
        election
            .cast_vote(vote.clone())
            .map_err(ApiError::internal)?;
    }
    Ok(election)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::to_bytes, response::IntoResponse};

    #[tokio::test]
    async fn votes_are_hashed_and_election_state_survives_handler_calls() {
        let database_path = std::env::temp_dir().join(format!("eng-sft-{}.db", Uuid::new_v4()));
        let database_url = format!("sqlite://{}", database_path.display());
        let pool = crate::infrastructure::db::init_db(&database_url)
            .await
            .unwrap();
        let manifest = serde_json::from_value(serde_json::json!({
            "type": "academic",
            "slates": ["A", "B"],
            "minimum_quorum": 1,
            "votes": []
        }))
        .unwrap();

        let response = create_election(State(pool.clone()), Json(manifest))
            .await
            .unwrap()
            .into_response();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let created: CreateElectionResponse = serde_json::from_slice(&body).unwrap();

        assert_eq!(
            cast_vote(
                Path(created.id.clone()),
                State(pool.clone()),
                Json(serde_json::json!("A")),
            )
            .await
            .unwrap(),
            StatusCode::OK
        );

        let persisted_hash: String =
            sqlx::query_scalar("SELECT current_hash FROM election_events WHERE election_id = ?")
                .bind(&created.id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(persisted_hash.len(), 64);

        let result = close_election(Path(created.id.clone()), State(pool.clone()))
            .await
            .unwrap();
        assert_eq!(result.0, serde_json::json!({"Winner": "A"}));

        drop(pool);
        let pool = crate::infrastructure::db::init_db(&database_url)
            .await
            .unwrap();
        let status: String = sqlx::query_scalar("SELECT status FROM elections WHERE id = ?")
            .bind(&created.id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(status, "closed");
        let stored_outcome: String =
            sqlx::query_scalar("SELECT outcome FROM elections WHERE id = ?")
                .bind(created.id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&stored_outcome).unwrap(),
            serde_json::json!({"Winner": "A"})
        );
        drop(pool);
        std::fs::remove_file(database_path).unwrap();
    }

    #[tokio::test]
    async fn rejects_invalid_votes_without_logging_them() {
        let pool = crate::infrastructure::db::init_db("sqlite::memory:")
            .await
            .unwrap();
        let manifest = serde_json::from_value(serde_json::json!({
            "type": "academic",
            "slates": ["A", "B"],
            "votes": []
        }))
        .unwrap();
        let response = create_election(State(pool.clone()), Json(manifest))
            .await
            .unwrap()
            .into_response();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let created: CreateElectionResponse = serde_json::from_slice(&body).unwrap();

        assert!(
            cast_vote(
                Path(created.id.clone()),
                State(pool.clone()),
                Json(serde_json::json!("unknown")),
            )
            .await
            .is_err()
        );
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM election_events WHERE election_id = ?")
                .bind(created.id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count, 0);
    }
}
