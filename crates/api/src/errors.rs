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
//!    ```rust
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
use serde::Serialize;

use rustygene_storage::{StorageError, StorageErrorCode};

/// The top-level error type returned by every route handler.
///
/// ### How it works
/// Each variant maps to an HTTP status code and a fixed `code` string.
/// `IntoResponse` below serialises the variant into the JSON error envelope.
///
/// ### Bead `ahk` – what needs to change
/// The current variants carry only a `String` message, so structured
/// `details` cannot be attached.  As part of bead `ahk`, each variant (or
/// `BadRequest` at minimum) should be able to carry an optional
/// `serde_json::Value` for machine-readable context.  Consider:
///
/// ```rust
/// BadRequest { message: String, details: Option<serde_json::Value> },
/// ```
///
/// Also add `Unauthorized(String)` and `Forbidden(String)` for the full
/// contract documented at the top of this file.
#[derive(Debug)]
pub enum ApiError {
    NotFound(String),
    Conflict(String),
    BadRequest(String),
    InternalError(String),
    StorageError(StorageError),
}

impl ApiError {
    pub fn message(&self) -> String {
        match self {
            Self::NotFound(msg)
            | Self::Conflict(msg)
            | Self::BadRequest(msg)
            | Self::InternalError(msg) => msg.clone(),
            Self::StorageError(err) => err.message.clone(),
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::NotFound(_) => "NOT_FOUND",
            Self::Conflict(_) => "CONFLICT",
            Self::BadRequest(_) => "BAD_REQUEST",
            Self::InternalError(_) => "INTERNAL_ERROR",
            Self::StorageError(err) => match err.code {
                StorageErrorCode::NotFound => "NOT_FOUND",
                StorageErrorCode::Conflict => "CONFLICT",
                StorageErrorCode::Validation => "BAD_REQUEST",
                StorageErrorCode::Serialization | StorageErrorCode::Backend => "INTERNAL_ERROR",
            },
        }
    }

    fn status_code(&self) -> StatusCode {
        match self {
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::InternalError(_) => StatusCode::INTERNAL_SERVER_ERROR,
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

/// ### Current wire format (flat – pre-bead `ahk`)
/// ```json
/// { "error": "<string>", "code": "NOT_FOUND" }
/// ```
///
/// ### Target wire format (bead `ahk`)
/// ```json
/// { "error": { "type": "not_found", "message": "…", "details": {…} } }
/// ```
///
/// **Junior developer – to migrate:**
/// 1. Rename this struct to something like `ApiErrorInner` and add a `type`
///    field (`#[serde(rename = "type")] pub r#type: &'static str`) plus
///    `#[serde(skip_serializing_if = "Option::is_none")] pub details: Option<serde_json::Value>`.
/// 2. Create a wrapper struct `ApiErrorBody { error: ApiErrorInner }` so the
///    outer key is `"error"` and the value is the nested object.
/// 3. Remove the standalone `code` field — the `type` field replaces it.
/// 4. Update `into_response` to populate the new shape.
/// 5. Any existing tests hitting the `"code"` key in the response JSON will
///    need to be updated to check `"error"."type"` instead.
#[derive(Debug, Serialize)]
struct ApiErrorBody {
    // TODO (bead ahk): replace this flat shape with the nested envelope:
    // `{ "error": { "type": "…", "message": "…", "details": {…} } }`
    // See the module-level doc comment for the exact migration steps.
    error: String,
    code: &'static str,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let body = Json(ApiErrorBody {
            // TODO (bead ahk): replace with ApiErrorBody { error: ApiErrorInner { … } }
            error: self.message(),
            code: self.code(),
        });

        (status, body).into_response()
    }
}
