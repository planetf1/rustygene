use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use chrono::Utc;
use rustygene_core::research::{ResearchLogEntry, SearchResult};
use rustygene_core::types::EntityId;
use rustygene_storage::{Pagination, ResearchLogFilter};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::errors::ApiError;
use crate::AppState;

#[derive(Debug, Deserialize)]
struct ResearchLogListQuery {
    #[serde(default)]
    entity_id: Option<String>,
    #[serde(default)]
    limit: Option<u32>,
    #[serde(default)]
    offset: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct CreateResearchLogRequest {
    title: String,
    description: String,
    #[serde(default)]
    entity_references: Vec<EntityReference>,
    #[serde(default)]
    status: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UpdateResearchLogRequest {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    entity_references: Option<Vec<EntityReference>>,
    #[serde(default)]
    status: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct EntityReference {
    entity_type: String,
    id: EntityId,
}

#[derive(Debug, Serialize)]
struct ResearchLogEntryResponse {
    id: EntityId,
    title: String,
    description: String,
    entity_references: Vec<EntityReference>,
    status: String,
    created_at: String,
    updated_at: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_entries).post(create_entry))
        .route(
            "/:id",
            get(get_entry).put(update_entry).delete(delete_entry),
        )
}

async fn list_entries(
    State(state): State<AppState>,
    Query(query): Query<ResearchLogListQuery>,
) -> Result<Json<Vec<ResearchLogEntryResponse>>, ApiError> {
    let person_ref = query
        .entity_id
        .as_deref()
        .map(parse_entity_id)
        .transpose()?;

    let entries = state
        .storage
        .list_research_log_entries(
            &ResearchLogFilter {
                person_ref,
                result: None,
                date_from_iso: None,
                date_to_iso: None,
            },
            Pagination {
                limit: query.limit.unwrap_or(50),
                offset: query.offset.unwrap_or(0),
            },
        )
        .await?;

    Ok(Json(
        entries
            .into_iter()
            .map(research_entry_to_response)
            .collect::<Vec<_>>(),
    ))
}

async fn create_entry(
    State(state): State<AppState>,
    Json(request): Json<CreateResearchLogRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    if request.title.trim().is_empty() {
        return Err(ApiError::BadRequest("title must not be empty".to_string()));
    }

    let now = Utc::now();
    let entry = ResearchLogEntry {
        id: EntityId::new(),
        date: now,
        objective: request.title,
        repository: None,
        repository_name: None,
        search_terms: Vec::new(),
        source_searched: None,
        result: parse_research_status(request.status.as_deref())?,
        findings: Some(request.description),
        citations_created: Vec::new(),
        next_steps: None,
        person_refs: request
            .entity_references
            .iter()
            .filter(|r| r.entity_type.trim().eq_ignore_ascii_case("person"))
            .map(|r| r.id)
            .collect(),
        tags: encode_non_person_refs(&request.entity_references),
    };

    state.storage.create_research_log_entry(&entry).await?;

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "id": entry.id })),
    ))
}

async fn get_entry(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ResearchLogEntryResponse>, ApiError> {
    let entry_id = parse_entity_id(&id)?;
    let entry = state.storage.get_research_log_entry(entry_id).await?;
    Ok(Json(research_entry_to_response(entry)))
}

async fn update_entry(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<UpdateResearchLogRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let entry_id = parse_entity_id(&id)?;
    let mut entry = state.storage.get_research_log_entry(entry_id).await?;

    if let Some(title) = request.title {
        if title.trim().is_empty() {
            return Err(ApiError::BadRequest("title must not be empty".to_string()));
        }
        entry.objective = title;
    }

    if let Some(description) = request.description {
        entry.findings = Some(description);
    }

    if let Some(status) = request.status {
        entry.result = parse_research_status(Some(status.as_str()))?;
    }

    if let Some(entity_references) = request.entity_references {
        entry.person_refs = entity_references
            .iter()
            .filter(|r| r.entity_type.trim().eq_ignore_ascii_case("person"))
            .map(|r| r.id)
            .collect();
        entry.tags = encode_non_person_refs(&entity_references);
    }

    // no update method in Storage trait yet; replace row atomically in sqlite backend
    let backend = state.sqlite_backend.clone().ok_or_else(|| {
        ApiError::InternalError("research-log update requires sqlite backend".to_string())
    })?;

    backend.with_connection(|conn| {
        let search_terms = serde_json::to_string(&entry.search_terms).map_err(|e| {
            rustygene_storage::StorageError {
                code: rustygene_storage::StorageErrorCode::Serialization,
                message: format!("serialize search_terms failed: {e}"),
            }
        })?;
        let citations_created = serde_json::to_string(&entry.citations_created).map_err(|e| {
            rustygene_storage::StorageError {
                code: rustygene_storage::StorageErrorCode::Serialization,
                message: format!("serialize citations_created failed: {e}"),
            }
        })?;
        let person_refs = serde_json::to_string(&entry.person_refs).map_err(|e| {
            rustygene_storage::StorageError {
                code: rustygene_storage::StorageErrorCode::Serialization,
                message: format!("serialize person_refs failed: {e}"),
            }
        })?;
        let tags = serde_json::to_string(&entry.tags).map_err(|e| rustygene_storage::StorageError {
            code: rustygene_storage::StorageErrorCode::Serialization,
            message: format!("serialize tags failed: {e}"),
        })?;

        conn.execute(
            "UPDATE research_log
             SET objective = ?, result = ?, findings = ?, repository_id = ?, repository_name = ?,
                 search_terms = ?, source_id = ?, citations_created = ?, next_steps = ?, person_refs = ?, tags = ?
             WHERE id = ?",
            rusqlite::params![
                entry.objective,
                research_status_to_db(&entry.result),
                entry.findings,
                entry.repository.map(|v| v.to_string()),
                entry.repository_name,
                search_terms,
                entry.source_searched.map(|v| v.to_string()),
                citations_created,
                entry.next_steps,
                person_refs,
                tags,
                entry.id.to_string(),
            ],
        )
        .map_err(|e| rustygene_storage::StorageError {
            code: rustygene_storage::StorageErrorCode::Backend,
            message: format!("research-log update failed: {e}"),
        })?;

        Ok(())
    })?;

    Ok(Json(serde_json::json!({ "id": entry_id })))
}

async fn delete_entry(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let entry_id = parse_entity_id(&id)?;
    let _ = state.storage.get_research_log_entry(entry_id).await?;
    state.storage.delete_research_log_entry(entry_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

fn research_entry_to_response(entry: ResearchLogEntry) -> ResearchLogEntryResponse {
    let mut entity_refs = entry
        .person_refs
        .iter()
        .map(|id| EntityReference {
            entity_type: "person".to_string(),
            id: *id,
        })
        .collect::<Vec<_>>();

    entity_refs.extend(decode_non_person_refs(&entry.tags));

    let ts = entry.date.to_rfc3339();
    ResearchLogEntryResponse {
        id: entry.id,
        title: entry.objective,
        description: entry.findings.unwrap_or_default(),
        entity_references: entity_refs,
        status: map_research_status_label(entry.result),
        created_at: ts.clone(),
        updated_at: ts,
    }
}

fn parse_entity_id(raw: &str) -> Result<EntityId, ApiError> {
    Uuid::parse_str(raw)
        .map(EntityId)
        .map_err(|_| ApiError::BadRequest(format!("invalid entity id: {raw}")))
}

fn parse_research_status(raw: Option<&str>) -> Result<SearchResult, ApiError> {
    match raw.map(|v| v.trim().to_ascii_lowercase()) {
        None => Ok(SearchResult::Inconclusive),
        Some(v) if v == "open" => Ok(SearchResult::Inconclusive),
        Some(v) if v == "resolved" => Ok(SearchResult::Found),
        Some(v) if v == "deferred" => Ok(SearchResult::PartiallyFound),
        Some(v) if v == "not_found" => Ok(SearchResult::NotFound),
        Some(v) => Err(ApiError::BadRequest(format!(
            "invalid research-log status: {v}"
        ))),
    }
}

fn map_research_status_label(result: SearchResult) -> String {
    match result {
        SearchResult::Inconclusive => "open",
        SearchResult::Found => "resolved",
        SearchResult::PartiallyFound => "deferred",
        SearchResult::NotFound => "not_found",
    }
    .to_string()
}

fn research_status_to_db(result: &SearchResult) -> &'static str {
    match result {
        SearchResult::Found => "found",
        SearchResult::NotFound => "not_found",
        SearchResult::PartiallyFound => "partially_found",
        SearchResult::Inconclusive => "inconclusive",
    }
}

fn encode_non_person_refs(refs: &[EntityReference]) -> Vec<String> {
    refs.iter()
        .filter(|r| !r.entity_type.trim().eq_ignore_ascii_case("person"))
        .map(|r| {
            format!(
                "entity_ref:{}:{}",
                r.entity_type.trim().to_ascii_lowercase(),
                r.id
            )
        })
        .collect()
}

fn decode_non_person_refs(tags: &[String]) -> Vec<EntityReference> {
    tags.iter()
        .filter_map(|tag| {
            let rest = tag.strip_prefix("entity_ref:")?;
            let mut parts = rest.split(':');
            let entity_type = parts.next()?.to_string();
            let id_raw = parts.next()?;
            if parts.next().is_some() {
                return None;
            }
            Uuid::parse_str(id_raw)
                .ok()
                .map(EntityId)
                .map(|id| EntityReference { entity_type, id })
        })
        .collect()
}
