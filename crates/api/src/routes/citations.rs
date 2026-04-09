use std::collections::BTreeMap;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use rustygene_core::evidence::{Citation, CitationRef};
use rustygene_core::types::{DateValue, EntityId};
use rustygene_storage::Pagination;
use serde::Deserialize;

use crate::errors::{ApiError, parse_entity_id};
use crate::AppState;

#[derive(Debug, Deserialize)]
struct CitationsQuery {
    #[serde(default)]
    limit: Option<u32>,
    #[serde(default)]
    offset: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct CreateCitationRequest {
    source_id: EntityId,
    assertion_id: EntityId,
    #[serde(default)]
    citation_note: Option<String>,
    #[serde(default)]
    volume: Option<String>,
    #[serde(default)]
    page: Option<String>,
    #[serde(default)]
    folio: Option<String>,
    #[serde(default)]
    entry: Option<String>,
    #[serde(default)]
    confidence_level: Option<u8>,
    #[serde(default)]
    date_accessed: Option<DateValue>,
    #[serde(default)]
    transcription: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UpdateCitationRequest {
    source_id: EntityId,
    #[serde(default)]
    volume: Option<String>,
    #[serde(default)]
    page: Option<String>,
    #[serde(default)]
    folio: Option<String>,
    #[serde(default)]
    entry: Option<String>,
    #[serde(default)]
    confidence_level: Option<u8>,
    #[serde(default)]
    date_accessed: Option<DateValue>,
    #[serde(default)]
    transcription: Option<String>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_citations).post(create_citation))
        .route(
            "/:id",
            get(get_citation)
                .put(update_citation)
                .delete(delete_citation),
        )
}

async fn list_citations(
    State(state): State<AppState>,
    Query(query): Query<CitationsQuery>,
) -> Result<Json<Vec<Citation>>, ApiError> {
    let citations = state
        .storage
        .list_citations(Pagination {
            limit: query.limit.unwrap_or(100),
            offset: query.offset.unwrap_or(0),
        })
        .await?;

    Ok(Json(citations))
}

async fn create_citation(
    State(state): State<AppState>,
    Json(request): Json<CreateCitationRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    if let Some(confidence_level) = request.confidence_level {
        if confidence_level > 3 {
            return Err(ApiError::BadRequest {
                message: format!("Citation 'confidence_level' is out of range (got {confidence_level}). Provide a value between 0 and 3 (inclusive)."),
                details: Some(serde_json::json!({ "confidence_level": confidence_level, "range": [0, 3] })),
            });
        }
    }

    let _ = state.storage.get_source(request.source_id).await?;

    let citation_id = EntityId::new();
    let citation = Citation {
        id: citation_id,
        source_id: request.source_id,
        volume: request.volume,
        page: request.page,
        folio: request.folio,
        entry: request.entry,
        confidence_level: request.confidence_level,
        date_accessed: request.date_accessed,
        transcription: request.transcription,
        _raw_gedcom: BTreeMap::new(),
    };

    state.storage.create_citation(&citation).await?;
    state
        .storage
        .append_citation_ref_to_assertion(
            request.assertion_id,
            &CitationRef {
                citation_id,
                note: request.citation_note,
            },
        )
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "id": citation_id })),
    ))
}

async fn get_citation(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Citation>, ApiError> {
    let citation_id = parse_entity_id(&id)?;
    let citation = state.storage.get_citation(citation_id).await?;
    Ok(Json(citation))
}

async fn update_citation(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<UpdateCitationRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if let Some(confidence_level) = request.confidence_level {
        if confidence_level > 3 {
            return Err(ApiError::BadRequest {
                message: format!("Citation 'confidence_level' is out of range (got {confidence_level}). Provide a value between 0 and 3 (inclusive)."),
                details: Some(serde_json::json!({ "confidence_level": confidence_level, "range": [0, 3] })),
            });
        }
    }

    let citation_id = parse_entity_id(&id)?;
    let mut citation = state.storage.get_citation(citation_id).await?;

    let _ = state.storage.get_source(request.source_id).await?;
    citation.source_id = request.source_id;
    citation.volume = request.volume;
    citation.page = request.page;
    citation.folio = request.folio;
    citation.entry = request.entry;
    citation.confidence_level = request.confidence_level;
    citation.date_accessed = request.date_accessed;
    citation.transcription = request.transcription;

    state.storage.update_citation(&citation).await?;

    Ok(Json(serde_json::json!({ "id": citation_id })))
}

async fn delete_citation(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let citation_id = parse_entity_id(&id)?;
    let _ = state.storage.get_citation(citation_id).await?;
    state.storage.delete_citation(citation_id).await?;
    Ok(StatusCode::NO_CONTENT)
}


