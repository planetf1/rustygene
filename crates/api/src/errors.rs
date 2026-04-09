//! # API Error Handling  (see bead `rustygene-ahk`)
//!
//! ## Current state
//! Every handler returns `Result<_, ApiError>`.  When the `Err` branch is
//! taken, `IntoResponse for ApiError` serialises the response body as:
//!
//! ```json
//! { "error": "<human message>", "code": "NOT_FOUND" }
//! ```
//!
//! ## What bead `ahk` requires
//! The error envelope must be **extended** to the contract below so that API
//! clients can handle failures programmatically without parsing a free-form
//! string:
//!
//! ```json
//! {
//!   "error": {
//!     "type":    "validation|not_found|bad_request|internal|unavailable|unauthorized|forbidden",
//!     "message": "Human-readable, actionable message",
//!     "details": { /* optional – machine-parseable fields, e.g. which param is wrong */ }
//!   }
//! }
//! ```
//!
//! ## Junior developer checklist
//!
//! 1. **Rename / restructure `ApiErrorBody`** (line ~68 below).  Add a  `type`
//!    field that mirrors the string values listed above and an optional
//!    `details: Option<serde_json::Value>` field.  The outer key must change
//!    from `"error": "<string>"` to `"error": { … }`.  In serde terms, wrap
//!    the current flat struct in a newtype/single-field struct:
//!
//!    ```rust,ignore
//!    #[derive(Debug, Serialize)]
//!    struct ApiErrorInner {
//!        r#type: &'static str,    // serde rename to "type" – use #[serde(rename = "type")]
//!        message: String,
//!        #[serde(skip_serializing_if = "Option::is_none")]
//!        details: Option<serde_json::Value>,
//!    }
//!
//!    #[derive(Debug, Serialize)]
//!    struct ApiErrorBody { error: ApiErrorInner }
//!    ```
//!
//! 2. **Add a `details` method to `ApiError`** (or pass `details` through to
//!    `into_response`) so that callers like `BadRequest` can attach structured
//!    context (e.g. `{"field": "confidence", "expected": "0.0–1.0"}`).
//!
//! 3. **Extend `ApiError` variants** to carry the `type` string that maps to
//!    the prose above (`"validation"` for `BadRequest`, `"not_found"` for
//!    `NotFound`, `"internal"` for `InternalError`, etc.).
//!
//! 4. **Expand messages to be actionable**.  Client-correctable errors must
//!    answer *what* failed, *why*, and *how to fix it*.
//!    Example – current: `"confidence must be between 0.0 and 1.0"`
//!    Improved: `"confidence is out of range (got 1.7). Provide a float in [0.0, 1.0]."`
//!
//! 5. **Add `Unauthorized` / `Forbidden` variants** for the completeness of
//!    the contract even if auth is not yet wired up.
//!
//! 6. **Update `IntoResponse`** to serialise the new `ApiErrorBody` shape.
//!
//! 7. **Write integration tests** that assert the exact JSON shape for each
//!    error class (validation, not-found, internal) — see the acceptance
//!    criteria in bead `ahk`.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use rustygene_core::types::EntityId;
use serde::Serialize;
use uuid::Uuid;

use rustygene_storage::{StorageError, StorageErrorCode};

/// The top-level error type returned by every route handler.
///
/// ### How it works
/// Each variant maps to an HTTP status code and a fixed `type` string.
/// `IntoResponse` below serialises the variant into the JSON error envelope.
#[derive(Debug)]
pub enum ApiError {
    NotFound {
        message: String,
        details: Option<serde_json::Value>,
    },
    Conflict {
        message: String,
        details: Option<serde_json::Value>,
    },
    BadRequest {
        message: String,
        details: Option<serde_json::Value>,
    },
    Unauthorized {
        message: String,
        details: Option<serde_json::Value>,
    },
    Forbidden {
        message: String,
        details: Option<serde_json::Value>,
    },
    InternalError {
        message: String,
        details: Option<serde_json::Value>,
    },
    Unavailable {
        message: String,
        details: Option<serde_json::Value>,
    },
    StorageError(StorageError),
}

impl ApiError {
    /// Convenience constructor for a simple BadRequest error.
    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self::BadRequest {
            message: msg.into(),
            details: None,
        }
    }

    /// Convenience constructor for a simple InternalError.
    pub fn internal(msg: impl Into<String>) -> Self {
        Self::InternalError {
            message: msg.into(),
            details: None,
        }
    }

    /// Convenience constructor for a simple NotFound error.
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::NotFound {
            message: msg.into(),
            details: None,
        }
    }

    /// Attach structured details to the error if supported by the variant.
    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        match &mut self {
            Self::NotFound { details: d, .. }
            | Self::Conflict { details: d, .. }
            | Self::BadRequest { details: d, .. }
            | Self::Unauthorized { details: d, .. }
            | Self::Forbidden { details: d, .. }
            | Self::InternalError { details: d, .. }
            | Self::Unavailable { details: d, .. } => {
                *d = Some(details);
            }
            Self::StorageError(_) => {} // StorageError doesn't support details yet
        }
        self
    }

    pub fn message(&self) -> String {
        match self {
            Self::NotFound { message, .. }
            | Self::Conflict { message, .. }
            | Self::BadRequest { message, .. }
            | Self::Unauthorized { message, .. }
            | Self::Forbidden { message, .. }
            | Self::InternalError { message, .. }
            | Self::Unavailable { message, .. } => message.clone(),
            Self::StorageError(err) => err.message.clone(),
        }
    }

    fn r#type(&self) -> &'static str {
        match self {
            Self::NotFound { .. } => "not_found",
            Self::Conflict { .. } => "conflict",
            Self::BadRequest { .. } => "validation",
            Self::Unauthorized { .. } => "unauthorized",
            Self::Forbidden { .. } => "forbidden",
            Self::InternalError { .. } => "internal",
            Self::Unavailable { .. } => "unavailable",
            Self::StorageError(err) => match err.code {
                StorageErrorCode::NotFound => "not_found",
                StorageErrorCode::Conflict => "conflict",
                StorageErrorCode::Validation => "validation",
                StorageErrorCode::Serialization | StorageErrorCode::Backend => "internal",
            },
        }
    }

    fn details(&self) -> Option<serde_json::Value> {
        match self {
            Self::NotFound { details, .. }
            | Self::Conflict { details, .. }
            | Self::BadRequest { details, .. }
            | Self::Unauthorized { details, .. }
            | Self::Forbidden { details, .. }
            | Self::InternalError { details, .. }
            | Self::Unavailable { details, .. } => details.clone(),
            Self::StorageError(_) => None,
        }
    }

    fn status_code(&self) -> StatusCode {
        match self {
            Self::NotFound { .. } => StatusCode::NOT_FOUND,
            Self::Conflict { .. } => StatusCode::CONFLICT,
            Self::BadRequest { .. } => StatusCode::BAD_REQUEST,
            Self::Unauthorized { .. } => StatusCode::UNAUTHORIZED,
            Self::Forbidden { .. } => StatusCode::FORBIDDEN,
            Self::InternalError { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Unavailable { .. } => StatusCode::SERVICE_UNAVAILABLE,
            Self::StorageError(err) => match err.code {
                StorageErrorCode::NotFound => StatusCode::NOT_FOUND,
                StorageErrorCode::Conflict => StatusCode::CONFLICT,
                StorageErrorCode::Validation => StatusCode::BAD_REQUEST,
                StorageErrorCode::Serialization | StorageErrorCode::Backend => {
                    StatusCode::INTERNAL_SERVER_ERROR
                }
            },
        }
    }
}

impl From<StorageError> for ApiError {
    fn from(value: StorageError) -> Self {
        Self::StorageError(value)
    }
}

/// Centralized entity ID parsing for all route handlers.
pub fn parse_entity_id(raw: &str) -> Result<EntityId, ApiError> {
    Uuid::parse_str(raw).map(EntityId).map_err(|e| {
        ApiError::BadRequest {
            message: format!("invalid entity id: '{raw}' is not a valid UUID"),
            details: Some(serde_json::json!({
                "input": raw,
                "reason": e.to_string(),
                "format": "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
            })),
        }
    })
}

/// The inner structure of the JSON error envelope.
#[derive(Debug, Serialize)]
struct ApiErrorInner {
    #[serde(rename = "type")]
    pub r#type: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

/// The top-level JSON error envelope.
#[derive(Debug, Serialize)]
struct ApiErrorBody {
    error: ApiErrorInner,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let body = Json(ApiErrorBody {
            error: ApiErrorInner {
                r#type: self.r#type(),
                message: self.message(),
                details: self.details(),
            },
        });

        (status, body).into_response()
    }
}
