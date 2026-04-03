use axum::extract::{Path, State};
use axum::routing::put;
use axum::{Json, Router};
use chrono::Utc;
use rusqlite::OptionalExtension;
use rustygene_core::assertion::AssertionStatus;
use rustygene_core::types::EntityId;
use rustygene_storage::{StorageError, StorageErrorCode};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::errors::ApiError;
use crate::AppState;

#[derive(Debug, Deserialize)]
struct UpdateAssertionRequest {
    #[serde(default)]
    confidence: Option<f64>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    preferred: Option<bool>,
}

pub fn router() -> Router<AppState> {
    Router::new().route("/:id", put(update_assertion))
}

async fn update_assertion(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<UpdateAssertionRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let assertion_id = parse_entity_id(&id)?;

    if request.confidence.is_none() && request.status.is_none() && request.preferred.is_none() {
        return Err(ApiError::BadRequest(
            "at least one of confidence/status/preferred must be provided".to_string(),
        ));
    }

    if let Some(confidence) = request.confidence {
        if !(0.0..=1.0).contains(&confidence) {
            return Err(ApiError::BadRequest(
                "confidence must be between 0.0 and 1.0".to_string(),
            ));
        }

        update_assertion_confidence(&state, assertion_id, confidence).await?;
    }

    if let Some(status_raw) = request.status.as_deref() {
        let status = parse_status(status_raw)?;
        state
            .storage
            .update_assertion_status(assertion_id, status)
            .await?;
    }

    if let Some(preferred) = request.preferred {
        set_assertion_preferred(&state, assertion_id, preferred).await?;
    }

    let reviewed_at = Utc::now().to_rfc3339();

    Ok(Json(json!({
        "id": assertion_id,
        "updated": {
            "confidence": request.confidence,
            "status": request.status,
            "preferred": request.preferred,
            "reviewed_at": reviewed_at
        }
    })))
}

async fn update_assertion_confidence(
    state: &AppState,
    assertion_id: EntityId,
    confidence: f64,
) -> Result<(), ApiError> {
    let backend = state.sqlite_backend.clone().ok_or_else(|| {
        ApiError::InternalError("assertion updates require sqlite backend".to_string())
    })?;

    backend.with_connection(|conn| {
        let tx = conn
            .transaction()
            .map_err(storage_backend_error("begin assertion tx"))?;

        let rows = tx
            .execute(
                "UPDATE assertions SET confidence = ? WHERE id = ?",
                rusqlite::params![confidence, assertion_id.to_string()],
            )
            .map_err(storage_backend_error("update assertion confidence"))?;

        if rows == 0 {
            return Err(StorageError {
                code: StorageErrorCode::NotFound,
                message: format!("assertion not found: {assertion_id}"),
            });
        }

        tx.commit()
            .map_err(storage_backend_error("commit assertion confidence"))?;
        Ok(())
    })?;

    Ok(())
}

async fn set_assertion_preferred(
    state: &AppState,
    assertion_id: EntityId,
    preferred: bool,
) -> Result<(), ApiError> {
    let backend = state.sqlite_backend.clone().ok_or_else(|| {
        ApiError::InternalError("assertion updates require sqlite backend".to_string())
    })?;

    backend.with_connection(|conn| {
        let tx = conn
            .transaction()
            .map_err(storage_backend_error("begin preferred tx"))?;

        let found: Option<(String, String)> = tx
            .query_row(
                "SELECT entity_id, field FROM assertions WHERE id = ?",
                rusqlite::params![assertion_id.to_string()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(storage_backend_error(
                "lookup assertion for preferred update",
            ))?;

        let Some((entity_id, field)) = found else {
            return Err(StorageError {
                code: StorageErrorCode::NotFound,
                message: format!("assertion not found: {assertion_id}"),
            });
        };

        if preferred {
            tx.execute(
                "UPDATE assertions
                 SET preferred = 0
                 WHERE entity_id = ? AND field = ? AND id != ? AND sandbox_id IS NULL",
                rusqlite::params![entity_id, field, assertion_id.to_string()],
            )
            .map_err(storage_backend_error("clear existing preferred assertions"))?;
        }

        tx.execute(
            "UPDATE assertions SET preferred = ? WHERE id = ?",
            rusqlite::params![if preferred { 1 } else { 0 }, assertion_id.to_string()],
        )
        .map_err(storage_backend_error("set preferred assertion"))?;

        tx.commit()
            .map_err(storage_backend_error("commit preferred assertion"))?;
        Ok(())
    })?;

    Ok(())
}

fn parse_entity_id(raw: &str) -> Result<EntityId, ApiError> {
    Uuid::parse_str(raw)
        .map(EntityId)
        .map_err(|_| ApiError::BadRequest(format!("invalid entity id: {raw}")))
}

fn parse_status(raw: &str) -> Result<AssertionStatus, ApiError> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "pending" | "proposed" => Ok(AssertionStatus::Proposed),
        "approved" | "confirmed" => Ok(AssertionStatus::Confirmed),
        "rejected" | "retract" | "retracted" => Ok(AssertionStatus::Rejected),
        "disputed" => Ok(AssertionStatus::Disputed),
        value => Err(ApiError::BadRequest(format!(
            "invalid assertion status: {value}"
        ))),
    }
}

fn storage_backend_error(action: &'static str) -> impl Fn(rusqlite::Error) -> StorageError + Copy {
    move |err| StorageError {
        code: StorageErrorCode::Backend,
        message: format!("{action} failed: {err}"),
    }
}
