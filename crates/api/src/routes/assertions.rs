use axum::extract::{Path, State};
use axum::routing::put;
use axum::{Json, Router};
use chrono::Utc;
use rusqlite::OptionalExtension;
use rustygene_core::assertion::AssertionStatus;
use rustygene_core::types::EntityId;
use rustygene_storage::{StorageError, StorageErrorCode};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::errors::{ApiError, parse_entity_id};
use crate::AppState;

#[derive(Debug, Serialize, Deserialize)]
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
        return Err(ApiError::BadRequest {
            message: "Missing update parameters. At least one of 'confidence', 'status', or 'preferred' must be provided in the request body.".to_string(),
            details: Some(serde_json::json!({ "provided": request })),
        });
    }

    if let Some(confidence) = request.confidence {
        if !(0.0..=1.0).contains(&confidence) {
            return Err(ApiError::BadRequest {
                message: format!("Confidence is out of range (got {confidence}). Provide a float between 0.0 and 1.0 (inclusive)."),
                details: Some(serde_json::json!({ "confidence": confidence, "range": [0.0, 1.0] })),
            });
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
        ApiError::internal("assertion updates require sqlite backend")
    })?;

    backend.with_connection(|conn: &mut rusqlite::Connection| {
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
        ApiError::internal("assertion updates require sqlite backend")
    })?;

    backend.with_connection(|conn: &mut rusqlite::Connection| {
        let tx = conn
            .transaction()
            .map_err(storage_backend_error("begin preferred tx"))?;

        let found: Option<(String, String)> = tx
            .query_row(
                "SELECT entity_id, field FROM assertions WHERE id = ?",
                rusqlite::params![assertion_id.to_string()],
                |row: &rusqlite::Row| Ok((row.get(0)?, row.get(1)?)),
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



fn parse_status(raw: &str) -> Result<AssertionStatus, ApiError> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "pending" | "proposed" => Ok(AssertionStatus::Proposed),
        "approved" | "confirmed" => Ok(AssertionStatus::Confirmed),
        "rejected" | "retract" | "retracted" => Ok(AssertionStatus::Rejected),
        "disputed" => Ok(AssertionStatus::Disputed),
        value => Err(ApiError::BadRequest {
            message: format!("Invalid assertion status: '{value}'. Valid values are: proposed (pending), confirmed (approved), rejected (retract), disputed."),
            details: Some(serde_json::json!({ "invalid_status": value, "allowed": ["proposed", "pending", "confirmed", "approved", "rejected", "retracted", "retract", "disputed"] })),
        }),
    }
}

fn storage_backend_error(action: &'static str) -> impl Fn(rusqlite::Error) -> StorageError + Copy {
    move |err| StorageError {
        code: StorageErrorCode::Backend,
        message: format!("{action} failed: {err}"),
    }
}
