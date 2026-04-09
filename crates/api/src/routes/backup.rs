use axum::body::Body;
use axum::extract::{Multipart, Path, State};
use axum::http::header::{CONTENT_DISPOSITION, CONTENT_TYPE};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio_util::io::ReaderStream;

use crate::errors::ApiError;
use crate::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupInfo {
    pub filename: String,
    pub size_bytes: u64,
    /// Unix timestamp (seconds since epoch)
    pub created_at: i64,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", post(create_backup))
        .route("/", get(list_backups))
        .route("/:filename", get(download_backup))
        .route("/:filename", delete(delete_backup))
        .route("/restore", post(restore_backup))
}

fn require_backup_dir(state: &AppState) -> Result<PathBuf, ApiError> {
    let backend = state
        .sqlite_backend
        .as_ref()
        .ok_or_else(|| ApiError::internal("no SQLite backend configured"))?;
    backend.backup_dir().ok_or_else(|| {
        ApiError::internal(
            "backup directory unavailable (in-memory database has no backup dir)"
        )
    })
}

/// `POST /api/v1/backup` — create a new backup snapshot.
async fn create_backup(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let backup_dir = require_backup_dir(&state)?;
    let backend = state
        .sqlite_backend
        .as_ref()
        .ok_or_else(|| ApiError::internal("no SQLite backend configured"))?;

    tokio::fs::create_dir_all(&backup_dir)
        .await
        .map_err(|e| ApiError::internal(format!("create backup dir failed: {e}")))?;

    let filename = format!("backup_{}.db", Utc::now().format("%Y%m%d_%H%M%S"));
    let dest_path = backup_dir.join(&filename);

    let dest_clone = dest_path.clone();
    let backend_clone = backend.clone();
    let size: u64 = tokio::task::spawn_blocking(move || backend_clone.backup_to_file(&dest_clone))
        .await
        .map_err(|e| ApiError::internal(format!("backup task panicked: {e}")))?
        .map_err(|e| ApiError::internal(e.message))?;

    let created_at = std::fs::metadata(&dest_path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or_else(|| Utc::now().timestamp());

    Ok((
        StatusCode::CREATED,
        Json(BackupInfo {
            filename,
            size_bytes: size,
            created_at,
        }),
    ))
}

/// `GET /api/v1/backup` — list available backups.
async fn list_backups(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let backup_dir = require_backup_dir(&state)?;

    let raw: Vec<(String, u64, i64)> = tokio::task::spawn_blocking(move || {
        rustygene_storage::sqlite_impl::SqliteBackend::list_backups(&backup_dir)
    })
    .await
    .map_err(|e| ApiError::internal(format!("list_backups task panicked: {e}")))?
    .map_err(|e| ApiError::internal(e.message))?;

    let infos: Vec<BackupInfo> = raw
        .into_iter()
        .map(|(filename, size_bytes, created_at)| BackupInfo {
            filename,
            size_bytes,
            created_at,
        })
        .collect();

    Ok(Json(infos))
}

/// `GET /api/v1/backup/:filename` — download a backup file.
async fn download_backup(
    State(state): State<AppState>,
    Path(filename): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    // Reject path traversal attempts
    if filename.contains('/') || filename.contains("..") {
        return Err(ApiError::bad_request("invalid filename"));
    }

    let backup_dir = require_backup_dir(&state)?;
    let file_path = backup_dir.join(&filename);

    let file = tokio::fs::File::open(&file_path)
        .await
        .map_err(|_| ApiError::not_found(format!("backup '{filename}' not found")))?;

    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    let content_disposition = format!("attachment; filename=\"{filename}\"");
    let response = Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, "application/x-sqlite3")
        .header(
            CONTENT_DISPOSITION,
            content_disposition
                .parse::<axum::http::HeaderValue>()
                .map_err(|e| ApiError::internal(e.to_string()))?,
        )
        .body(body)
        .map_err(|e| ApiError::internal(e.to_string()))?;

    Ok(response)
}

/// `DELETE /api/v1/backup/:filename` — remove a backup file.
async fn delete_backup(
    State(state): State<AppState>,
    Path(filename): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    if filename.contains('/') || filename.contains("..") {
        return Err(ApiError::bad_request("invalid filename"));
    }

    let backup_dir = require_backup_dir(&state)?;
    let file_path = backup_dir.join(&filename);

    if !file_path.exists() {
        return Err(ApiError::not_found(format!("backup '{filename}' not found")));
    }

    tokio::fs::remove_file(&file_path)
        .await
        .map_err(|e| ApiError::internal(format!("delete backup failed: {e}")))?;

    Ok(StatusCode::NO_CONTENT)
}

/// `POST /api/v1/backup/restore` — restore from an uploaded `.db` file.
///
/// Accepts `multipart/form-data` with a `file` field containing the backup database.
async fn restore_backup(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, ApiError> {
    let backend = state
        .sqlite_backend
        .as_ref()
        .ok_or_else(|| ApiError::internal("no SQLite backend configured"))?;

    let mut file_bytes: Option<Vec<u8>> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::bad_request(format!("multipart read failed: {e}")))?
    {
        if field.name().map(|n| n == "file").unwrap_or(false) {
            let bytes = field
                .bytes()
                .await
                .map_err(|e| ApiError::bad_request(format!("file read failed: {e}")))?;
            file_bytes = Some(bytes.to_vec());
        }
    }

    let bytes =
        file_bytes.ok_or_else(|| ApiError::bad_request("missing 'file' field"))?;

    // Write to a temp file then restore from it
    let tmp_path =
        std::env::temp_dir().join(format!("rustygene_restore_{}.db", uuid::Uuid::new_v4()));
    tokio::fs::write(&tmp_path, &bytes)
        .await
        .map_err(|e| ApiError::internal(format!("write temp file failed: {e}")))?;

    let backend_clone = backend.clone();
    let tmp_clone = tmp_path.clone();
    let result: Result<(), _> =
        tokio::task::spawn_blocking(move || backend_clone.restore_from_file(&tmp_clone))
            .await
            .map_err(|e| ApiError::internal(format!("restore task panicked: {e}")))?;

    // Clean up temp file regardless of outcome
    let _ = tokio::fs::remove_file(&tmp_path).await;

    result.map_err(|e| ApiError::internal(e.message))?;

    Ok(StatusCode::NO_CONTENT)
}
