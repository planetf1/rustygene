use std::collections::BTreeMap;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use rustygene_core::evidence::{Citation, RepositoryRef, Source};
use rustygene_core::types::EntityId;
use rustygene_storage::Pagination;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::errors::ApiError;
use crate::AppState;

#[derive(Debug, Deserialize)]
struct SourcesQuery {
    #[serde(default)]
    limit: Option<u32>,
    #[serde(default)]
    offset: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct UpsertSourceRequest {
    title: String,
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    publication_info: Option<String>,
    #[serde(default)]
    abbreviation: Option<String>,
    #[serde(default)]
    repository_refs: Vec<RepositoryRef>,
}

#[derive(Debug, Serialize)]
struct SourceDetailResponse {
    id: EntityId,
    title: String,
    author: Option<String>,
    publication_info: Option<String>,
    abbreviation: Option<String>,
    repository_refs: Vec<RepositoryRef>,
    citations: Vec<Citation>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_sources).post(create_source))
        .route(
            "/:id",
            get(get_source).put(update_source).delete(delete_source),
        )
}

async fn list_sources(
    State(state): State<AppState>,
    Query(query): Query<SourcesQuery>,
) -> Result<Json<Vec<Source>>, ApiError> {
    let sources = state
        .storage
        .list_sources(Pagination {
            limit: query.limit.unwrap_or(100),
            offset: query.offset.unwrap_or(0),
        })
        .await?;

    Ok(Json(sources))
}

async fn create_source(
    State(state): State<AppState>,
    Json(request): Json<UpsertSourceRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    if request.title.trim().is_empty() {
        return Err(ApiError::BadRequest(
            "source title must not be empty".to_string(),
        ));
    }

    for repository_ref in &request.repository_refs {
        let _ = state
            .storage
            .get_repository(repository_ref.repository_id)
            .await?;
    }

    let source_id = EntityId::new();
    let source = Source {
        id: source_id,
        title: request.title,
        author: request.author,
        publication_info: request.publication_info,
        abbreviation: request.abbreviation,
        repository_refs: request.repository_refs,
        original_xref: None,
        _raw_gedcom: BTreeMap::new(),
    };

    state.storage.create_source(&source).await?;

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "id": source_id })),
    ))
}

async fn get_source(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<SourceDetailResponse>, ApiError> {
    let source_id = parse_entity_id(&id)?;
    let source = state.storage.get_source(source_id).await?;
    let citations = state
        .storage
        .list_citations(Pagination {
            limit: 1_000,
            offset: 0,
        })
        .await?
        .into_iter()
        .filter(|citation| citation.source_id == source_id)
        .collect::<Vec<_>>();

    Ok(Json(SourceDetailResponse {
        id: source.id,
        title: source.title,
        author: source.author,
        publication_info: source.publication_info,
        abbreviation: source.abbreviation,
        repository_refs: source.repository_refs,
        citations,
    }))
}

async fn update_source(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<UpsertSourceRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if request.title.trim().is_empty() {
        return Err(ApiError::BadRequest(
            "source title must not be empty".to_string(),
        ));
    }

    let source_id = parse_entity_id(&id)?;
    let mut source = state.storage.get_source(source_id).await?;

    for repository_ref in &request.repository_refs {
        let _ = state
            .storage
            .get_repository(repository_ref.repository_id)
            .await?;
    }

    source.title = request.title;
    source.author = request.author;
    source.publication_info = request.publication_info;
    source.abbreviation = request.abbreviation;
    source.repository_refs = request.repository_refs;

    state.storage.update_source(&source).await?;

    Ok(Json(serde_json::json!({ "id": source_id })))
}

async fn delete_source(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let source_id = parse_entity_id(&id)?;
    let _ = state.storage.get_source(source_id).await?;
    state.storage.delete_source(source_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

fn parse_entity_id(raw: &str) -> Result<EntityId, ApiError> {
    Uuid::parse_str(raw)
        .map(EntityId)
        .map_err(|_| ApiError::BadRequest(format!("invalid entity id: {raw}")))
}
