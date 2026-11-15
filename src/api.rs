use std::{collections::HashMap, sync::Arc};

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
};
use eng_sft::domain::{
    factory::{ElectionBoundary, ElectionFactory},
    manifest::Manifest,
};
use serde::Serialize;
use tokio::sync::RwLock;
use uuid::Uuid;

pub type SharedState = Arc<RwLock<HashMap<String, Box<dyn ElectionBoundary + Send + Sync>>>>;

pub fn app_router() -> Router {
    let state: SharedState = Arc::new(RwLock::new(HashMap::new()));

    Router::new()
        .route("/api/elections", post(create_election))
        .route("/api/elections/{id}/votes", post(cast_vote))
        .route("/api/elections/{id}/close", post(close_election))
        .with_state(state)
}

#[derive(Serialize)]
pub struct CreateElectionResponse {
    pub id: String,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

struct ApiError(StatusCode, String);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let Self(status, message) = self;
        (status, Json(ErrorResponse { error: message })).into_response()
    }
}

async fn create_election(
    State(state): State<SharedState>,
    Json(manifest): Json<Manifest>,
) -> Result<impl IntoResponse, ApiError> {
    let election = ElectionFactory::create(manifest)
        .map_err(|error| ApiError(StatusCode::BAD_REQUEST, error.to_string()))?;

    let id = Uuid::new_v4().to_string();

    let mut store = state.write().await;
    store.insert(id.clone(), election);

    Ok((StatusCode::CREATED, Json(CreateElectionResponse { id })))
}

async fn cast_vote(
    Path(id): Path<String>,
    State(state): State<SharedState>,
    Json(vote): Json<serde_json::Value>,
) -> Result<StatusCode, ApiError> {
    let mut store = state.write().await;
    let election = store
        .get_mut(&id)
        .ok_or_else(|| ApiError(StatusCode::NOT_FOUND, "Election not found".to_string()))?;

    election
        .cast_vote(vote)
        .map_err(|error| ApiError(StatusCode::BAD_REQUEST, error))?;

    Ok(StatusCode::OK)
}

async fn close_election(
    Path(id): Path<String>,
    State(state): State<SharedState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let election = state
        .write()
        .await
        .remove(&id)
        .ok_or_else(|| ApiError(StatusCode::NOT_FOUND, "Election not found".to_string()))?;

    let result_json = election
        .close()
        .map_err(|error| ApiError(StatusCode::BAD_REQUEST, error))?;

    Ok(Json(result_json))
}
