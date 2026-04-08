use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use chrono::{DateTime, NaiveDate, Utc};
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
    entity_type: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    q: Option<String>,
    #[serde(default)]
    date_from: Option<String>,
    #[serde(default)]
    date_to: Option<String>,
    #[serde(default)]
    limit: Option<u32>,
    #[serde(default)]
    offset: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct CreateResearchLogRequest {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    date: Option<String>,
    #[serde(default)]
    researcher: Option<String>,
    #[serde(default)]
    hypothesis: Option<String>,
    #[serde(default)]
    action_taken: Option<String>,
    #[serde(default)]
    outcome: Option<String>,
    #[serde(default)]
    confidence: Option<f64>,
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
    date: Option<String>,
    #[serde(default)]
    researcher: Option<String>,
    #[serde(default)]
    hypothesis: Option<String>,
    #[serde(default)]
    action_taken: Option<String>,
    #[serde(default)]
    outcome: Option<String>,
    #[serde(default)]
    confidence: Option<f64>,
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
    date: String,
    researcher: String,
    hypothesis: String,
    action_taken: String,
    outcome: String,
    confidence: f64,
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
    let entity_id = query
        .entity_id
        .as_deref()
        .map(parse_entity_id)
        .transpose()?;

    let requested_type = query
        .entity_type
        .as_deref()
        .map(|value| value.trim().to_ascii_lowercase());

    if let Some(ref entity_type) = requested_type {
        if !matches!(entity_type.as_str(), "person" | "family" | "source" | "event") {
            return Err(ApiError::BadRequest(format!(
                "invalid entity_type: {entity_type}"
            )));
        }
    }

    let effective_entity_type = requested_type
        .as_deref()
        .or(if entity_id.is_some() { Some("person") } else { None });

    let person_ref = if effective_entity_type == Some("person") {
        entity_id
    } else {
        None
    };

    let status_filter = query
        .status
        .as_deref()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty());

    let text_filter = query
        .q
        .as_deref()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty());

    let offset = query.offset.unwrap_or(0) as usize;
    let limit = query.limit.unwrap_or(100) as usize;
    let fetch_limit = (offset + limit).clamp(200, 5000) as u32;

    let entries = state
        .storage
        .list_research_log_entries(
            &ResearchLogFilter {
                person_ref,
                result: None,
                date_from_iso: query.date_from.clone(),
                date_to_iso: query.date_to.clone(),
            },
            Pagination {
                limit: fetch_limit,
                offset: 0,
            },
        )
        .await?;

    let mut mapped = entries
        .into_iter()
        .map(research_entry_to_response)
        .collect::<Vec<_>>();

    if let (Some(filter_entity_id), Some(filter_entity_type)) = (entity_id, effective_entity_type) {
        mapped.retain(|entry| {
            entry
                .entity_references
                .iter()
                .any(|reference| {
                    reference.id == filter_entity_id
                        && reference.entity_type.trim().eq_ignore_ascii_case(filter_entity_type)
                })
        });
    }

    if let Some(status) = status_filter {
        mapped.retain(|entry| entry.status == status);
    }

    if let Some(query_text) = text_filter {
        mapped.retain(|entry| {
            [
                entry.hypothesis.as_str(),
                entry.action_taken.as_str(),
                entry.outcome.as_str(),
                entry.researcher.as_str(),
            ]
            .iter()
            .any(|value| value.to_ascii_lowercase().contains(&query_text))
        });
    }

    mapped.sort_by(|left, right| right.date.cmp(&left.date));

    let paged = mapped
        .into_iter()
        .skip(offset)
        .take(limit)
        .collect::<Vec<_>>();

    Ok(Json(paged))
}

async fn create_entry(
    State(state): State<AppState>,
    Json(request): Json<CreateResearchLogRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let objective = request
        .hypothesis
        .as_deref()
        .or(request.title.as_deref())
        .unwrap_or_default()
        .trim()
        .to_string();

    if objective.is_empty() {
        return Err(ApiError::BadRequest("title must not be empty".to_string()));
    }

    let now = request
        .date
        .as_deref()
        .map(parse_entry_date)
        .transpose()?
        .unwrap_or_else(Utc::now);

    let mut tags = encode_non_person_refs(&request.entity_references);
    if let Some(confidence) = request.confidence {
        upsert_confidence_tag(&mut tags, confidence)?;
    }

    let entry = ResearchLogEntry {
        id: EntityId::new(),
        date: now,
        objective,
        repository: None,
        repository_name: request
            .researcher
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string),
        search_terms: Vec::new(),
        source_searched: None,
        result: parse_research_status(request.status.as_deref())?,
        findings: request
            .outcome
            .as_deref()
            .or(request.description.as_deref())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string),
        citations_created: Vec::new(),
        next_steps: request
            .action_taken
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string),
        person_refs: request
            .entity_references
            .iter()
            .filter(|r| r.entity_type.trim().eq_ignore_ascii_case("person"))
            .map(|r| r.id)
            .collect(),
        tags,
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

    if let Some(hypothesis) = request.hypothesis {
        if hypothesis.trim().is_empty() {
            return Err(ApiError::BadRequest(
                "hypothesis must not be empty".to_string(),
            ));
        }
        entry.objective = hypothesis;
    }

    if let Some(raw_date) = request.date {
        entry.date = parse_entry_date(raw_date.as_str())?;
    }

    if let Some(researcher) = request.researcher {
        entry.repository_name = if researcher.trim().is_empty() {
            None
        } else {
            Some(researcher)
        };
    }

    if let Some(action_taken) = request.action_taken {
        entry.next_steps = if action_taken.trim().is_empty() {
            None
        } else {
            Some(action_taken)
        };
    }

    if let Some(description) = request.description {
        entry.findings = Some(description);
    }

    if let Some(outcome) = request.outcome {
        entry.findings = if outcome.trim().is_empty() {
            None
        } else {
            Some(outcome)
        };
    }

    if let Some(status) = request.status {
        entry.result = parse_research_status(Some(status.as_str()))?;
    }

    if let Some(confidence) = request.confidence {
        upsert_confidence_tag(&mut entry.tags, confidence)?;
    }

    if let Some(entity_references) = request.entity_references {
        let metadata_tags = entry
            .tags
            .iter()
            .filter(|tag| !tag.starts_with("entity_ref:"))
            .cloned()
            .collect::<Vec<_>>();

        entry.person_refs = entity_references
            .iter()
            .filter(|r| r.entity_type.trim().eq_ignore_ascii_case("person"))
            .map(|r| r.id)
            .collect();
        let mut rebuilt_tags = metadata_tags;
        rebuilt_tags.extend(encode_non_person_refs(&entity_references));
        entry.tags = rebuilt_tags;
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
    let confidence = extract_confidence(&entry.tags).unwrap_or(0.5);
    let outcome = entry.findings.unwrap_or_default();
    let action_taken = entry.next_steps.unwrap_or_default();
    let hypothesis = entry.objective;
    let researcher = entry.repository_name.unwrap_or_default();
    let status = map_research_status_label(entry.result);

    ResearchLogEntryResponse {
        id: entry.id,
        date: ts.clone(),
        researcher: researcher.clone(),
        hypothesis: hypothesis.clone(),
        action_taken: action_taken.clone(),
        outcome: outcome.clone(),
        confidence,
        title: hypothesis,
        description: outcome,
        entity_references: entity_refs,
        status,
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
        Some(v) if v == "working" || v == "deferred" => Ok(SearchResult::PartiallyFound),
        Some(v) if v == "closed" || v == "resolved" => Ok(SearchResult::Found),
        Some(v) if v == "abandoned" || v == "not_found" => Ok(SearchResult::NotFound),
        Some(v) => Err(ApiError::BadRequest(format!(
            "invalid research-log status: {v}"
        ))),
    }
}

fn map_research_status_label(result: SearchResult) -> String {
    match result {
        SearchResult::Inconclusive => "open",
        SearchResult::PartiallyFound => "working",
        SearchResult::Found => "closed",
        SearchResult::NotFound => "abandoned",
    }
    .to_string()
}

fn parse_entry_date(raw: &str) -> Result<DateTime<Utc>, ApiError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(ApiError::BadRequest("date must not be empty".to_string()));
    }

    if let Ok(parsed) = DateTime::parse_from_rfc3339(trimmed) {
        return Ok(parsed.with_timezone(&Utc));
    }

    if let Ok(parsed_date) = NaiveDate::parse_from_str(trimmed, "%Y-%m-%d") {
        if let Some(naive_dt) = parsed_date.and_hms_opt(0, 0, 0) {
            return Ok(DateTime::<Utc>::from_naive_utc_and_offset(naive_dt, Utc));
        }
    }

    Err(ApiError::BadRequest(format!(
        "invalid date format: {trimmed}. Use RFC3339 or YYYY-MM-DD"
    )))
}

fn upsert_confidence_tag(tags: &mut Vec<String>, confidence: f64) -> Result<(), ApiError> {
    if !(0.0..=1.0).contains(&confidence) {
        return Err(ApiError::BadRequest(
            "confidence must be between 0 and 1".to_string(),
        ));
    }

    tags.retain(|tag| !tag.starts_with("meta_confidence:"));
    tags.push(format!("meta_confidence:{confidence:.4}"));
    Ok(())
}

fn extract_confidence(tags: &[String]) -> Option<f64> {
    tags.iter().find_map(|tag| {
        tag.strip_prefix("meta_confidence:")
            .and_then(|value| value.parse::<f64>().ok())
    })
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
