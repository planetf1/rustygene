use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use chrono::Utc;
use rustygene_core::assertion::{Assertion, AssertionStatus, EvidenceType, Sandbox, SandboxStatus};
use rustygene_core::types::{ActorRef, EntityId};
use rustygene_storage::{EntityType, Pagination, SandboxAssertionDiff};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::errors::ApiError;
use crate::AppState;

// ---- Request / Response types ----

#[derive(Debug, Deserialize)]
struct ListSandboxesQuery {
    #[serde(default)]
    limit: Option<u32>,
    #[serde(default)]
    offset: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct CreateSandboxRequest {
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    parent_sandbox: Option<EntityId>,
}

#[derive(Debug, Deserialize)]
struct UpdateStatusRequest {
    status: String,
}

#[derive(Debug, Deserialize)]
struct DiffQuery {
    entity_id: String,
    entity_type: String,
}

#[derive(Debug, Deserialize)]
struct EntitySnapshotQuery {
    entity_type: String,
}

#[derive(Debug, Deserialize)]
struct CreateSandboxAssertionRequest {
    entity_id: EntityId,
    entity_type: String,
    field: String,
    value: Value,
    #[serde(default)]
    confidence: Option<f64>,
    #[serde(default)]
    evidence_type: Option<EvidenceType>,
}

#[derive(Debug, Serialize)]
struct SandboxResponse {
    id: EntityId,
    name: String,
    description: Option<String>,
    created_at: String,
    parent_sandbox: Option<EntityId>,
    status: String,
}

#[derive(Debug, Serialize)]
struct DiffResponse {
    field: String,
    trunk_assertion_id: Option<EntityId>,
    trunk_value: Option<Value>,
    sandbox_assertion_id: Option<EntityId>,
    sandbox_value: Option<Value>,
}

// ---- Router ----

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_sandboxes).post(create_sandbox))
        .route("/:id", get(get_sandbox).delete(delete_sandbox_handler))
        .route("/:id/status", put(update_status))
        .route("/:id/diff", get(get_diff))
        .route("/:id/assertions", post(create_assertion))
        .route("/:id/entities/:entity_id", get(get_entity_snapshot))
}

// ---- Helpers ----

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

fn parse_sandbox_status(raw: &str) -> Result<SandboxStatus, ApiError> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "active" => Ok(SandboxStatus::Active),
        "promoted" => Ok(SandboxStatus::Promoted),
        "discarded" => Ok(SandboxStatus::Discarded),
        _ => Err(ApiError::BadRequest(format!(
            "invalid sandbox status: {raw}"
        ))),
    }
}

fn status_label(status: &SandboxStatus) -> &'static str {
    match status {
        SandboxStatus::Active => "active",
        SandboxStatus::Promoted => "promoted",
        SandboxStatus::Discarded => "discarded",
    }
}

fn map_sandbox(s: Sandbox) -> SandboxResponse {
    SandboxResponse {
        id: s.id,
        name: s.name,
        description: s.description,
        created_at: s.created_at.to_rfc3339(),
        parent_sandbox: s.parent_sandbox,
        status: status_label(&s.status).to_string(),
    }
}

fn map_diff(d: SandboxAssertionDiff) -> DiffResponse {
    DiffResponse {
        field: d.field,
        trunk_assertion_id: d.trunk_assertion_id,
        trunk_value: d.trunk_value,
        sandbox_assertion_id: d.sandbox_assertion_id,
        sandbox_value: d.sandbox_value,
    }
}

// ---- Handlers ----

/// `GET /api/v1/sandboxes` — list all sandboxes.
async fn list_sandboxes(
    State(state): State<AppState>,
    Query(query): Query<ListSandboxesQuery>,
) -> Result<Json<Vec<SandboxResponse>>, ApiError> {
    let sandboxes = state
        .storage
        .list_sandboxes(Pagination {
            limit: query.limit.unwrap_or(100),
            offset: query.offset.unwrap_or(0),
        })
        .await?;
    Ok(Json(sandboxes.into_iter().map(map_sandbox).collect()))
}

/// `POST /api/v1/sandboxes` — create a new sandbox.
async fn create_sandbox(
    State(state): State<AppState>,
    Json(body): Json<CreateSandboxRequest>,
) -> Result<impl axum::response::IntoResponse, ApiError> {
    let sandbox = Sandbox {
        id: EntityId::new(),
        name: body.name,
        description: body.description,
        created_at: Utc::now(),
        parent_sandbox: body.parent_sandbox,
        status: SandboxStatus::Active,
    };
    state.storage.create_sandbox(&sandbox).await?;
    Ok((StatusCode::CREATED, Json(map_sandbox(sandbox))))
}

/// `GET /api/v1/sandboxes/:id` — fetch sandbox details.
async fn get_sandbox(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<SandboxResponse>, ApiError> {
    let sandbox_id = parse_entity_id(&id)?;
    let sandbox = state.storage.get_sandbox(sandbox_id).await?;
    Ok(Json(map_sandbox(sandbox)))
}

/// `DELETE /api/v1/sandboxes/:id` — delete a sandbox.
async fn delete_sandbox_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let sandbox_id = parse_entity_id(&id)?;
    state.storage.delete_sandbox(sandbox_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `PUT /api/v1/sandboxes/:id/status` — promote or discard a sandbox.
async fn update_status(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<UpdateStatusRequest>,
) -> Result<StatusCode, ApiError> {
    let sandbox_id = parse_entity_id(&id)?;
    let status = parse_sandbox_status(&body.status)?;
    state
        .storage
        .update_sandbox_status(sandbox_id, status)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /api/v1/sandboxes/:id/diff?entity_id=...&entity_type=...` — compare sandbox vs trunk.
async fn get_diff(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<DiffQuery>,
) -> Result<Json<Vec<DiffResponse>>, ApiError> {
    let sandbox_id = parse_entity_id(&id)?;
    let entity_id = parse_entity_id(&query.entity_id)?;
    let entity_type = parse_entity_type(&query.entity_type)?;
    let diffs = state
        .storage
        .compare_sandbox_vs_trunk(entity_id, entity_type, sandbox_id)
        .await?;
    Ok(Json(diffs.into_iter().map(map_diff).collect()))
}

/// `POST /api/v1/sandboxes/:id/assertions` — add an assertion to a sandbox.
async fn create_assertion(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<CreateSandboxAssertionRequest>,
) -> Result<StatusCode, ApiError> {
    let sandbox_id = parse_entity_id(&id)?;
    let entity_type = parse_entity_type(&body.entity_type)?;
    let assertion = Assertion {
        id: EntityId::new(),
        value: body.value,
        confidence: body.confidence.unwrap_or(0.8),
        status: AssertionStatus::Proposed,
        evidence_type: body.evidence_type.unwrap_or(EvidenceType::Direct),
        source_citations: Vec::new(),
        proposed_by: ActorRef::Agent("api".to_string()),
        created_at: Utc::now(),
        reviewed_at: None,
        reviewed_by: None,
    };
    state
        .storage
        .create_assertion_in_sandbox(
            body.entity_id,
            entity_type,
            &body.field,
            &assertion,
            sandbox_id,
        )
        .await?;
    Ok(StatusCode::CREATED)
}

/// `GET /api/v1/sandboxes/:id/entities/:entity_id?entity_type=...` — entity snapshot with sandbox overlay.
async fn get_entity_snapshot(
    State(state): State<AppState>,
    Path((id, entity_id_raw)): Path<(String, String)>,
    Query(query): Query<EntitySnapshotQuery>,
) -> Result<Json<Value>, ApiError> {
    let sandbox_id = parse_entity_id(&id)?;
    let entity_id = parse_entity_id(&entity_id_raw)?;
    let entity_type = parse_entity_type(&query.entity_type)?;
    let snapshot = state
        .storage
        .compute_entity_snapshot_with_sandbox(entity_id, entity_type, sandbox_id)
        .await?;
    Ok(Json(snapshot))
}
