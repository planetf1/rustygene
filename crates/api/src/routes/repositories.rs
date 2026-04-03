use std::collections::BTreeMap;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use rustygene_core::evidence::{Repository, RepositoryType};
use rustygene_core::types::EntityId;
use rustygene_storage::Pagination;
use serde::Deserialize;
use uuid::Uuid;

use crate::errors::ApiError;
use crate::AppState;

#[derive(Debug, Deserialize)]
struct RepositoriesQuery {
    #[serde(default)]
    limit: Option<u32>,
    #[serde(default)]
    offset: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct UpsertRepositoryRequest {
    name: String,
    #[serde(default)]
    repository_type: Option<RepositoryType>,
    #[serde(default)]
    address: Option<String>,
    #[serde(default)]
    urls: Vec<String>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_repositories).post(create_repository))
        .route(
            "/:id",
            get(get_repository)
                .put(update_repository)
                .delete(delete_repository),
        )
}

async fn list_repositories(
    State(state): State<AppState>,
    Query(query): Query<RepositoriesQuery>,
) -> Result<Json<Vec<Repository>>, ApiError> {
    let repositories = state
        .storage
        .list_repositories(Pagination {
            limit: query.limit.unwrap_or(100),
            offset: query.offset.unwrap_or(0),
        })
        .await?;

    Ok(Json(repositories))
}

async fn create_repository(
    State(state): State<AppState>,
    Json(request): Json<UpsertRepositoryRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    if request.name.trim().is_empty() {
        return Err(ApiError::BadRequest(
            "repository name must not be empty".to_string(),
        ));
    }

    let repository_id = EntityId::new();
    let repository = Repository {
        id: repository_id,
        name: request.name,
        repository_type: request.repository_type.unwrap_or_default(),
        address: request.address,
        urls: request.urls,
        original_xref: None,
        _raw_gedcom: BTreeMap::new(),
    };

    state.storage.create_repository(&repository).await?;

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "id": repository_id })),
    ))
}

async fn get_repository(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Repository>, ApiError> {
    let repository_id = parse_entity_id(&id)?;
    let repository = state.storage.get_repository(repository_id).await?;
    Ok(Json(repository))
}

async fn update_repository(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<UpsertRepositoryRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if request.name.trim().is_empty() {
        return Err(ApiError::BadRequest(
            "repository name must not be empty".to_string(),
        ));
    }

    let repository_id = parse_entity_id(&id)?;
    let mut repository = state.storage.get_repository(repository_id).await?;

    repository.name = request.name;
    repository.repository_type = request.repository_type.unwrap_or_default();
    repository.address = request.address;
    repository.urls = request.urls;

    state.storage.update_repository(&repository).await?;

    Ok(Json(serde_json::json!({ "id": repository_id })))
}

async fn delete_repository(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let repository_id = parse_entity_id(&id)?;
    let _ = state.storage.get_repository(repository_id).await?;
    state.storage.delete_repository(repository_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

fn parse_entity_id(raw: &str) -> Result<EntityId, ApiError> {
    Uuid::parse_str(raw)
        .map(EntityId)
        .map_err(|_| ApiError::BadRequest(format!("invalid entity id: {raw}")))
}
