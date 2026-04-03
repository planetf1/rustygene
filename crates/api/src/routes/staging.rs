use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use rusqlite::OptionalExtension;
use rustygene_core::assertion::{AssertionStatus, EvidenceType};
use rustygene_core::types::{ActorRef, EntityId};
use rustygene_storage::{
    EntityType, JsonAssertion, Pagination, StagingProposal, StagingProposalFilter,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::errors::ApiError;
use crate::AppState;

#[derive(Debug, Deserialize)]
struct StagingListQuery {
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    entity_id: Option<String>,
    #[serde(default)]
    entity_type: Option<String>,
    #[serde(default)]
    limit: Option<u32>,
    #[serde(default)]
    offset: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct SubmitStagingProposalRequest {
    entity_type: String,
    entity_id: EntityId,
    proposed_field: String,
    proposed_value: Value,
    #[serde(default)]
    confidence: Option<f64>,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    submitted_by: Option<String>,
    #[serde(default)]
    evidence_type: Option<EvidenceType>,
}

#[derive(Debug, Deserialize)]
struct ReviewProposalRequest {
    #[serde(default)]
    reviewer: Option<String>,
    #[serde(default)]
    reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BulkReviewRequest {
    ids: Vec<EntityId>,
    action: String,
    #[serde(default)]
    reviewer: Option<String>,
    #[serde(default)]
    reason: Option<String>,
}

#[derive(Debug, Serialize)]
struct StagingProposalResponse {
    id: EntityId,
    entity_type: String,
    entity_id: EntityId,
    proposed_field: String,
    proposed_value: Value,
    current_value: Option<Value>,
    diff_summary: String,
    confidence: f64,
    source: Option<String>,
    status: String,
    created_at: String,
    reviewed_at: Option<String>,
    reviewed_by: Option<String>,
    review_note: Option<String>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_staging).post(submit_staging))
        .route("/bulk", post(bulk_review))
        .route("/:id", get(get_staging_detail))
        .route("/:id/approve", post(approve_staging))
        .route("/:id/reject", post(reject_staging))
}

async fn list_staging(
    State(state): State<AppState>,
    Query(query): Query<StagingListQuery>,
) -> Result<Json<Vec<StagingProposalResponse>>, ApiError> {
    let filter = StagingProposalFilter {
        entity_id: query
            .entity_id
            .as_deref()
            .map(parse_entity_id)
            .transpose()?,
        entity_type: query
            .entity_type
            .as_deref()
            .map(parse_entity_type)
            .transpose()?,
        status: parse_staging_status(query.status.as_deref())?,
    };

    let proposals = state
        .storage
        .list_staging_proposals(
            &filter,
            Pagination {
                limit: query.limit.unwrap_or(100),
                offset: query.offset.unwrap_or(0),
            },
        )
        .await?;

    let mut output = Vec::with_capacity(proposals.len());
    for proposal in proposals {
        output.push(materialize_staging_response(&state, proposal).await?);
    }

    Ok(Json(output))
}

async fn submit_staging(
    State(state): State<AppState>,
    Json(request): Json<SubmitStagingProposalRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    if request.proposed_field.trim().is_empty() {
        return Err(ApiError::BadRequest(
            "proposed_field must not be empty".to_string(),
        ));
    }

    let entity_type = parse_entity_type(&request.entity_type)?;
    let confidence = request.confidence.unwrap_or(0.8);
    let source = request.source.clone().unwrap_or_else(|| "api".to_string());

    let assertion = JsonAssertion {
        id: EntityId::new(),
        value: request.proposed_value,
        confidence,
        status: AssertionStatus::Proposed,
        evidence_type: request.evidence_type.unwrap_or(EvidenceType::Direct),
        source_citations: Vec::new(),
        proposed_by: ActorRef::User(source.clone()),
        created_at: Utc::now(),
        reviewed_at: None,
        reviewed_by: None,
    };

    let proposal_id = state
        .storage
        .submit_staging_proposal(
            request.entity_id,
            entity_type,
            &request.proposed_field,
            &assertion,
            request.submitted_by.as_deref().unwrap_or("api"),
        )
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "id": proposal_id })),
    ))
}

async fn get_staging_detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<StagingProposalResponse>, ApiError> {
    let proposal_id = parse_entity_id(&id)?;
    let proposal = get_staging_proposal_by_id(&state, proposal_id).await?;
    Ok(Json(materialize_staging_response(&state, proposal).await?))
}

async fn approve_staging(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<ReviewProposalRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let proposal_id = parse_entity_id(&id)?;
    let reviewer = reviewer_actor_ref(request.reviewer.as_deref().unwrap_or("api"));
    state
        .storage
        .accept_staging_proposal(proposal_id, &reviewer)
        .await?;
    state.publish_staging_approved(proposal_id, &reviewer);

    Ok(Json(serde_json::json!({
        "id": proposal_id,
        "status": "approved"
    })))
}

async fn reject_staging(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<ReviewProposalRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let proposal_id = parse_entity_id(&id)?;
    let reviewer = reviewer_actor_ref(request.reviewer.as_deref().unwrap_or("api"));
    state
        .storage
        .reject_staging_proposal(proposal_id, &reviewer, request.reason.as_deref())
        .await?;
    state.publish_staging_rejected(proposal_id, &reviewer);

    Ok(Json(serde_json::json!({
        "id": proposal_id,
        "status": "rejected"
    })))
}

async fn bulk_review(
    State(state): State<AppState>,
    Json(request): Json<BulkReviewRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if request.ids.is_empty() {
        return Err(ApiError::BadRequest(
            "bulk ids must not be empty".to_string(),
        ));
    }

    let reviewer = reviewer_actor_ref(request.reviewer.as_deref().unwrap_or("api"));
    let action = request.action.trim().to_ascii_lowercase();

    let backend = state.sqlite_backend.clone().ok_or_else(|| {
        ApiError::InternalError("bulk staging requires sqlite backend".to_string())
    })?;

    backend.with_connection(|conn| {
        let tx = conn.transaction().map_err(|e| rustygene_storage::StorageError {
            code: rustygene_storage::StorageErrorCode::Backend,
            message: format!("bulk transaction begin failed: {e}"),
        })?;

        for proposal_id in &request.ids {
            let status: Option<String> = tx
                .query_row(
                    "SELECT status FROM staging_queue WHERE id = ?",
                    rusqlite::params![proposal_id.to_string()],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|e| rustygene_storage::StorageError {
                    code: rustygene_storage::StorageErrorCode::Backend,
                    message: format!("bulk lookup failed: {e}"),
                })?;

            let status = status.ok_or(rustygene_storage::StorageError {
                code: rustygene_storage::StorageErrorCode::NotFound,
                message: format!("staging proposal not found: {proposal_id}"),
            })?;

            if status != "proposed" {
                return Err(rustygene_storage::StorageError {
                    code: rustygene_storage::StorageErrorCode::Conflict,
                    message: format!("proposal not pending: {proposal_id}"),
                });
            }
        }

        let now = Utc::now().to_rfc3339();

        for proposal_id in &request.ids {
            let (assertion_id, entity_id, field): (String, String, String) = tx
                .query_row(
                    "SELECT assertion_id, entity_id, field FROM staging_queue WHERE id = ?",
                    rusqlite::params![proposal_id.to_string()],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .map_err(|e| rustygene_storage::StorageError {
                    code: rustygene_storage::StorageErrorCode::Backend,
                    message: format!("bulk read proposal failed: {e}"),
                })?;

            match action.as_str() {
                "approve" => {
                    tx.execute(
                        "UPDATE assertions
                         SET preferred = 0
                         WHERE entity_id = ? AND field = ? AND id != ? AND sandbox_id IS NULL",
                        rusqlite::params![entity_id, field, assertion_id],
                    )
                    .map_err(|e| rustygene_storage::StorageError {
                        code: rustygene_storage::StorageErrorCode::Backend,
                        message: format!("bulk clear preferred failed: {e}"),
                    })?;

                    tx.execute(
                        "UPDATE assertions
                         SET status = 'confirmed', preferred = 1, reviewed_at = ?, reviewed_by = ?
                         WHERE id = ?",
                        rusqlite::params![now, reviewer.as_str(), assertion_id],
                    )
                    .map_err(|e| rustygene_storage::StorageError {
                        code: rustygene_storage::StorageErrorCode::Backend,
                        message: format!("bulk approve assertion failed: {e}"),
                    })?;

                    tx.execute(
                        "UPDATE staging_queue
                         SET status = 'confirmed', reviewed_at = ?, reviewed_by = ?, review_note = NULL
                         WHERE id = ?",
                        rusqlite::params![now, reviewer.as_str(), proposal_id.to_string()],
                    )
                    .map_err(|e| rustygene_storage::StorageError {
                        code: rustygene_storage::StorageErrorCode::Backend,
                        message: format!("bulk approve queue failed: {e}"),
                    })?;
                }
                "reject" => {
                    tx.execute(
                        "UPDATE assertions
                         SET status = 'rejected', preferred = 0, reviewed_at = ?, reviewed_by = ?
                         WHERE id = ?",
                        rusqlite::params![now, reviewer.as_str(), assertion_id],
                    )
                    .map_err(|e| rustygene_storage::StorageError {
                        code: rustygene_storage::StorageErrorCode::Backend,
                        message: format!("bulk reject assertion failed: {e}"),
                    })?;

                    tx.execute(
                        "UPDATE staging_queue
                         SET status = 'rejected', reviewed_at = ?, reviewed_by = ?, review_note = ?
                         WHERE id = ?",
                        rusqlite::params![
                            now,
                            reviewer.as_str(),
                            request.reason.as_deref(),
                            proposal_id.to_string()
                        ],
                    )
                    .map_err(|e| rustygene_storage::StorageError {
                        code: rustygene_storage::StorageErrorCode::Backend,
                        message: format!("bulk reject queue failed: {e}"),
                    })?;
                }
                _ => {
                    return Err(rustygene_storage::StorageError {
                        code: rustygene_storage::StorageErrorCode::Validation,
                        message: format!("unsupported bulk action: {}", request.action),
                    });
                }
            }
        }

        tx.commit().map_err(|e| rustygene_storage::StorageError {
            code: rustygene_storage::StorageErrorCode::Backend,
            message: format!("bulk commit failed: {e}"),
        })?;

        Ok(())
    })?;

    Ok(Json(serde_json::json!({
        "processed": request.ids.len(),
        "action": action
    })))
}

async fn materialize_staging_response(
    state: &AppState,
    proposal: StagingProposal,
) -> Result<StagingProposalResponse, ApiError> {
    let assertion = find_assertion_by_id(state, proposal.entity_id, proposal.assertion_id).await?;
    let current_value = current_field_value(
        state,
        proposal.entity_id,
        &proposal.field,
        proposal.assertion_id,
    )
    .await?;
    let diff_summary =
        build_diff_summary(&proposal.field, current_value.as_ref(), &assertion.value);

    Ok(StagingProposalResponse {
        id: proposal.id,
        entity_type: entity_type_label(proposal.entity_type),
        entity_id: proposal.entity_id,
        proposed_field: proposal.field,
        proposed_value: assertion.value,
        current_value,
        diff_summary,
        confidence: assertion.confidence,
        source: match assertion.proposed_by {
            ActorRef::User(v) | ActorRef::Agent(v) | ActorRef::Import(v) => Some(v),
        },
        status: map_status_label(proposal.status),
        created_at: proposal.submitted_at,
        reviewed_at: proposal.reviewed_at,
        reviewed_by: proposal.reviewed_by,
        review_note: proposal.review_note,
    })
}

async fn find_assertion_by_id(
    state: &AppState,
    entity_id: EntityId,
    assertion_id: EntityId,
) -> Result<JsonAssertion, ApiError> {
    let assertions = state.storage.list_assertions_for_entity(entity_id).await?;
    assertions
        .into_iter()
        .find(|a| a.id == assertion_id)
        .ok_or_else(|| {
            ApiError::NotFound(format!("assertion not found for proposal: {assertion_id}"))
        })
}

async fn current_field_value(
    state: &AppState,
    entity_id: EntityId,
    field: &str,
    proposed_assertion_id: EntityId,
) -> Result<Option<Value>, ApiError> {
    let assertions = state
        .storage
        .list_assertions_for_field(entity_id, field)
        .await?;
    let current = assertions
        .iter()
        .find(|a| {
            a.id != proposed_assertion_id
                && a.status == AssertionStatus::Confirmed
                && (a.reviewed_at.is_some() || a.reviewed_by.is_some())
        })
        .or_else(|| {
            assertions
                .iter()
                .find(|a| a.id != proposed_assertion_id && a.status == AssertionStatus::Confirmed)
        });

    Ok(current.map(|a| a.value.clone()))
}

async fn get_staging_proposal_by_id(
    state: &AppState,
    proposal_id: EntityId,
) -> Result<StagingProposal, ApiError> {
    let proposals = state
        .storage
        .list_staging_proposals(
            &StagingProposalFilter {
                entity_id: None,
                entity_type: None,
                status: None,
            },
            Pagination {
                limit: 10_000,
                offset: 0,
            },
        )
        .await?;

    proposals
        .into_iter()
        .find(|proposal| proposal.id == proposal_id)
        .ok_or_else(|| ApiError::NotFound(format!("staging proposal not found: {proposal_id}")))
}

fn parse_entity_id(raw: &str) -> Result<EntityId, ApiError> {
    Uuid::parse_str(raw)
        .map(EntityId)
        .map_err(|_| ApiError::BadRequest(format!("invalid entity id: {raw}")))
}

fn parse_entity_type(raw: &str) -> Result<EntityType, ApiError> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "person" => Ok(EntityType::Person),
        "family" => Ok(EntityType::Family),
        "relationship" => Ok(EntityType::Relationship),
        "event" => Ok(EntityType::Event),
        "place" => Ok(EntityType::Place),
        "source" => Ok(EntityType::Source),
        "citation" => Ok(EntityType::Citation),
        "repository" => Ok(EntityType::Repository),
        "media" => Ok(EntityType::Media),
        "note" => Ok(EntityType::Note),
        "lds_ordinance" | "ldsordinance" => Ok(EntityType::LdsOrdinance),
        _ => Err(ApiError::BadRequest(format!("invalid entity_type: {raw}"))),
    }
}

fn parse_staging_status(raw: Option<&str>) -> Result<Option<AssertionStatus>, ApiError> {
    match raw.map(|v| v.trim().to_ascii_lowercase()) {
        None => Ok(None),
        Some(v) if v == "pending" || v == "proposed" => Ok(Some(AssertionStatus::Proposed)),
        Some(v) if v == "approved" || v == "confirmed" => Ok(Some(AssertionStatus::Confirmed)),
        Some(v) if v == "rejected" => Ok(Some(AssertionStatus::Rejected)),
        Some(v) if v == "disputed" => Ok(Some(AssertionStatus::Disputed)),
        Some(v) => Err(ApiError::BadRequest(format!("invalid status filter: {v}"))),
    }
}

fn map_status_label(status: AssertionStatus) -> String {
    match status {
        AssertionStatus::Proposed => "pending".to_string(),
        AssertionStatus::Confirmed => "approved".to_string(),
        AssertionStatus::Rejected => "rejected".to_string(),
        AssertionStatus::Disputed => "disputed".to_string(),
    }
}

fn entity_type_label(entity_type: EntityType) -> String {
    match entity_type {
        EntityType::Person => "person",
        EntityType::Family => "family",
        EntityType::Relationship => "relationship",
        EntityType::Event => "event",
        EntityType::Place => "place",
        EntityType::Source => "source",
        EntityType::Citation => "citation",
        EntityType::Repository => "repository",
        EntityType::Media => "media",
        EntityType::Note => "note",
        EntityType::LdsOrdinance => "lds_ordinance",
    }
    .to_string()
}

fn build_diff_summary(field: &str, current: Option<&Value>, proposed: &Value) -> String {
    match current {
        Some(previous) => format!(
            "changing {field} from {} to {}",
            value_preview(previous),
            value_preview(proposed)
        ),
        None => format!("setting {field} to {}", value_preview(proposed)),
    }
}

fn value_preview(value: &Value) -> String {
    match value {
        Value::String(s) => format!("'{}'", s),
        _ => value.to_string(),
    }
}

fn reviewer_actor_ref(raw: &str) -> String {
    if raw.contains(':') {
        raw.to_string()
    } else {
        format!("user:{raw}")
    }
}
