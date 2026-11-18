use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::{
    application::{ElectionService, ElectionServiceError},
    domain::{ElectionSummary, ElectionVote, manifest::Manifest, storage::Infrastructure},
    error::StorageError,
};

pub fn app_router<I>(service: ElectionService<I>) -> Router
where
    I: Infrastructure<Value, Value> + Clone + Send + Sync + 'static,
{
    Router::new()
        .route(
            "/api/elections",
            get(list_elections::<I>).post(create_election::<I>),
        )
        .route("/api/elections/{id}/votes", post(cast_vote::<I>))
        .route("/api/elections/{id}/close", post(close_election::<I>))
        .with_state(service)
}

#[derive(Serialize, Deserialize)]
pub struct CreateElectionResponse {
    pub id: String,
}

#[derive(Deserialize)]
struct CreateElectionRequest {
    name: String,
    manifest: Manifest,
}

async fn list_elections<I>(
    State(service): State<ElectionService<I>>,
) -> Result<Json<Vec<ElectionSummary>>, ApiError>
where
    I: Infrastructure<Value, Value> + Clone + Send + Sync + 'static,
{
    let elections = service.list_elections().await.map_err(ApiError::from)?;
    Ok(Json(elections))
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

#[derive(Debug)]
struct ApiError(StatusCode, String);

impl From<ElectionServiceError> for ApiError {
    fn from(error: ElectionServiceError) -> Self {
        let status = match &error {
            ElectionServiceError::Invalid(_) => StatusCode::BAD_REQUEST,
            ElectionServiceError::NotFound => StatusCode::NOT_FOUND,
            ElectionServiceError::Closed => StatusCode::CONFLICT,
            ElectionServiceError::Infrastructure(StorageError::ElectionNotFound(_)) => {
                StatusCode::NOT_FOUND
            }
            ElectionServiceError::Infrastructure(
                StorageError::ElectionClosed
                | StorageError::VoterAlreadyVoted
                | StorageError::DuplicateElectionName
                | StorageError::ConcurrencyConflict { .. },
            ) => StatusCode::CONFLICT,
            ElectionServiceError::Infrastructure(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        Self(status, error.to_string())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let Self(status, message) = self;
        (status, Json(ErrorResponse { error: message })).into_response()
    }
}

async fn create_election<I>(
    State(service): State<ElectionService<I>>,
    Json(request): Json<CreateElectionRequest>,
) -> Result<impl IntoResponse, ApiError>
where
    I: Infrastructure<Value, Value> + Clone + Send + Sync + 'static,
{
    let id = service
        .create_election(request.name, request.manifest)
        .await
        .map_err(ApiError::from)?;
    Ok((StatusCode::CREATED, Json(CreateElectionResponse { id })))
}

async fn cast_vote<I>(
    Path(id): Path<String>,
    State(service): State<ElectionService<I>>,
    headers: HeaderMap,
    Json(vote): Json<ElectionVote>,
) -> Result<StatusCode, ApiError>
where
    I: Infrastructure<Value, Value> + Clone + Send + Sync + 'static,
{
    let voter_id = headers
        .get("x-voter-id")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| Uuid::parse_str(value).ok())
        .ok_or_else(|| {
            ApiError(
                StatusCode::BAD_REQUEST,
                "Missing or invalid X-Voter-Id header".to_owned(),
            )
        })?;
    service
        .cast_vote(&id, voter_id, vote)
        .await
        .map_err(ApiError::from)?;
    Ok(StatusCode::OK)
}

async fn close_election<I>(
    Path(id): Path<String>,
    State(service): State<ElectionService<I>>,
) -> Result<Json<Value>, ApiError>
where
    I: Infrastructure<Value, Value> + Clone + Send + Sync + 'static,
{
    let outcome = service.close_election(&id).await.map_err(ApiError::from)?;
    Ok(Json(outcome))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{Body, to_bytes},
        http::Request,
        response::IntoResponse,
    };
    use tower::ServiceExt;

    use crate::infrastructure::MemoryInfrastructure;

    fn json_request(uri: &str, body: &str) -> Request<Body> {
        json_request_with_voter(uri, body, "00000000-0000-0000-0000-000000000001")
    }

    fn json_request_with_voter(uri: &str, body: &str, voter_id: &str) -> Request<Body> {
        Request::builder()
            .method("POST")
            .uri(uri)
            .header("content-type", "application/json")
            .header("x-voter-id", voter_id)
            .body(Body::from(body.to_owned()))
            .unwrap()
    }

    fn get_request(uri: &str) -> Request<Body> {
        Request::builder()
            .method("GET")
            .uri(uri)
            .body(Body::empty())
            .unwrap()
    }

    #[tokio::test]
    async fn list_route_returns_elections_and_persisted_results() {
        let service = ElectionService::new(MemoryInfrastructure::<Value, Value>::new());
        let app = app_router(service);
        let created = app
            .clone()
            .oneshot(json_request(
                "/api/elections",
                r#"{"name":"Academic election","manifest":{"type":"academic","slates":["A","B"],"votes":[]}}"#,
            ))
            .await
            .unwrap();
        let body = to_bytes(created.into_body(), usize::MAX).await.unwrap();
        let created: CreateElectionResponse = serde_json::from_slice(&body).unwrap();
        let duplicate = app
            .clone()
            .oneshot(json_request(
                "/api/elections",
                r#"{"name":"academic ELECTION","manifest":{"type":"academic","slates":["A"],"votes":[]}}"#,
            ))
            .await
            .unwrap();
        assert_eq!(duplicate.status(), StatusCode::CONFLICT);

        let list = app
            .clone()
            .oneshot(get_request("/api/elections"))
            .await
            .unwrap();
        assert_eq!(list.status(), StatusCode::OK);
        let body = to_bytes(list.into_body(), usize::MAX).await.unwrap();
        let elections: Vec<ElectionSummary> = serde_json::from_slice(&body).unwrap();
        assert_eq!(elections.len(), 1);
        assert_eq!(elections[0].id, created.id);
        assert_eq!(elections[0].name, "Academic election");
        assert!(matches!(
            &elections[0].manifest,
            Manifest::Typed(crate::domain::manifest::TypedManifest::Academic { .. })
        ));
        assert!(!elections[0].is_closed);
        assert!(elections[0].outcome.is_none());

        app.clone()
            .oneshot(json_request(
                &format!("/api/elections/{}/votes", created.id),
                r#""A""#,
            ))
            .await
            .unwrap();
        app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/elections/{}/close", created.id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let list = app.oneshot(get_request("/api/elections")).await.unwrap();
        let body = to_bytes(list.into_body(), usize::MAX).await.unwrap();
        let elections: Vec<ElectionSummary> = serde_json::from_slice(&body).unwrap();
        assert!(elections[0].is_closed);
        assert_eq!(
            elections[0].outcome,
            Some(serde_json::json!({"Winner": "A"}))
        );
    }

    #[tokio::test]
    async fn maps_service_errors_to_statuses_and_json_errors() {
        let errors = [
            (
                ElectionServiceError::Invalid("invalid".into()),
                StatusCode::BAD_REQUEST,
            ),
            (ElectionServiceError::NotFound, StatusCode::NOT_FOUND),
            (ElectionServiceError::Closed, StatusCode::CONFLICT),
            (
                StorageError::ElectionNotFound("election".into()).into(),
                StatusCode::NOT_FOUND,
            ),
            (StorageError::ElectionClosed.into(), StatusCode::CONFLICT),
            (StorageError::VoterAlreadyVoted.into(), StatusCode::CONFLICT),
            (
                StorageError::DuplicateElectionName.into(),
                StatusCode::CONFLICT,
            ),
            (
                StorageError::ConcurrencyConflict {
                    expected: 1,
                    got: 2,
                }
                .into(),
                StatusCode::CONFLICT,
            ),
            (
                StorageError::AppendFailed("write".into()).into(),
                StatusCode::INTERNAL_SERVER_ERROR,
            ),
            (
                StorageError::ReadFailed("read".into()).into(),
                StatusCode::INTERNAL_SERVER_ERROR,
            ),
        ];

        for (error, expected_status) in errors {
            let response = ApiError::from(error).into_response();
            assert_eq!(response.status(), expected_status);
            let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            assert!(serde_json::from_slice::<Value>(&body).unwrap()["error"].is_string());
        }
    }

    #[tokio::test]
    async fn vote_route_maps_json_and_election_errors_to_http_statuses() {
        let service = ElectionService::new(MemoryInfrastructure::<Value, Value>::new());
        let app = app_router(service);
        let response = app
            .clone()
            .oneshot(json_request(
                "/api/elections",
                r#"{"name":"Election","manifest":{"type":"academic","slates":["A","B"],"votes":[]}}"#,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let created: CreateElectionResponse = serde_json::from_slice(&body).unwrap();
        let vote_uri = format!("/api/elections/{}/votes", created.id);

        let missing_voter_id = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(&vote_uri)
                    .header("content-type", "application/json")
                    .body(Body::from(r#""A""#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(missing_voter_id.status(), StatusCode::BAD_REQUEST);

        assert_eq!(
            app.clone()
                .oneshot(json_request(&vote_uri, "{"))
                .await
                .unwrap()
                .status(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            app.clone()
                .oneshot(json_request(&vote_uri, r#""unknown""#))
                .await
                .unwrap()
                .status(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            app.clone()
                .oneshot(json_request(&vote_uri, r#"["A"]"#))
                .await
                .unwrap()
                .status(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            app.clone()
                .oneshot(json_request("/api/elections/missing/votes", r#""A""#,))
                .await
                .unwrap()
                .status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            app.clone()
                .oneshot(json_request(&vote_uri, r#""A""#))
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
        assert_eq!(
            app.clone()
                .oneshot(json_request_with_voter(
                    &vote_uri,
                    r#""A""#,
                    "00000000-0000-0000-0000-000000000001",
                ))
                .await
                .unwrap()
                .status(),
            StatusCode::CONFLICT
        );
        assert_eq!(
            app.clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri(format!("/api/elections/{}/close", created.id))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
        assert_eq!(
            app.oneshot(json_request(&vote_uri, r#""A""#))
                .await
                .unwrap()
                .status(),
            StatusCode::CONFLICT
        );
    }

    #[tokio::test]
    async fn elections_use_the_injected_infrastructure() {
        let service = ElectionService::new(MemoryInfrastructure::<Value, Value>::new());
        let manifest = serde_json::from_value(serde_json::json!({
            "type": "academic",
            "slates": ["A", "B"],
            "minimum_quorum": 1,
            "votes": []
        }))
        .unwrap();

        let response = create_election(
            State(service.clone()),
            Json(CreateElectionRequest {
                name: "Direct test election".to_owned(),
                manifest,
            }),
        )
        .await
        .unwrap()
        .into_response();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let created: CreateElectionResponse = serde_json::from_slice(&body).unwrap();

        assert_eq!(
            cast_vote(
                Path(created.id.clone()),
                State(service.clone()),
                HeaderMap::from_iter([(
                    axum::http::header::HeaderName::from_static("x-voter-id"),
                    "00000000-0000-0000-0000-000000000002".parse().unwrap(),
                )]),
                Json(ElectionVote::Academic(
                    crate::domain::academic::Slate::new("A").unwrap()
                )),
            )
            .await
            .unwrap(),
            StatusCode::OK
        );

        let outcome = close_election(Path(created.id), State(service))
            .await
            .unwrap();
        assert_eq!(outcome.0, serde_json::json!({"Winner": "A"}));
    }

    #[tokio::test]
    async fn rejects_invalid_votes_without_logging_them() {
        let infrastructure = MemoryInfrastructure::<Value, Value>::new();
        let service = ElectionService::new(infrastructure.clone());
        let manifest = serde_json::from_value(serde_json::json!({
            "type": "academic",
            "slates": ["A", "B"],
            "votes": []
        }))
        .unwrap();
        let id = service
            .create_election("Invalid vote test".to_owned(), manifest)
            .await
            .unwrap();

        assert!(
            service
                .cast_vote(
                    &id,
                    Uuid::from_u128(3),
                    ElectionVote::Academic(crate::domain::academic::Slate::new("unknown").unwrap()),
                )
                .await
                .is_err()
        );
        assert!(
            infrastructure
                .get_election(&id)
                .await
                .unwrap()
                .unwrap()
                .vote_events
                .is_empty()
        );
    }
}
