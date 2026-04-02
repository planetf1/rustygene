use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

use rustygene_storage::{StorageError, StorageErrorCode};

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

#[derive(Debug, Serialize)]
struct ApiErrorBody {
    error: String,
    code: &'static str,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let body = Json(ApiErrorBody {
            error: self.message(),
            code: self.code(),
        });

        (status, body).into_response()
    }
}
