use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use axum::extract::{Query, State};
use axum::http::header::{CONTENT_DISPOSITION, CONTENT_TYPE};
use axum::http::{HeaderValue, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::errors::ApiError;
use crate::{AppState, RouteMetric};

const SENSITIVE_ENV_KEYS: [&str; 6] = [
    "AWS_SECRET_ACCESS_KEY",
    "AZURE_OPENAI_API_KEY",
    "OPENAI_API_KEY",
    "DATABASE_URL",
    "RUSTYGENE_TOKEN",
    "RUSTYGENE_API_KEY",
];

#[derive(Debug, Deserialize)]
struct DebugLogsQuery {
    #[serde(default)]
    level: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Serialize)]
struct DebugHealthDepsResponse {
    debug_enabled: bool,
    app_version: &'static str,
    git_commit: Option<&'static str>,
    api_port: u16,
    db: DependencyStatus,
    migrations: MigrationStatus,
    media_dir: DependencyStatus,
    config_snapshot: ConfigSnapshot,
}

#[derive(Debug, Serialize)]
struct DependencyStatus {
    ok: bool,
    detail: String,
}

#[derive(Debug, Serialize)]
struct MigrationStatus {
    ok: bool,
    present_tables: usize,
    missing_tables: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ConfigSnapshot {
    db_path: String,
    media_dir: String,
    cors_origins: Vec<String>,
    sandbox_mode_hint: String,
}

#[derive(Debug, Serialize)]
struct DebugMetricsResponse {
    total_requests: u64,
    routes: Vec<RouteMetric>,
    import_jobs: ImportJobCounts,
}

#[derive(Debug, Serialize)]
struct ImportJobCounts {
    queued: usize,
    running: usize,
    completed: usize,
    failed: usize,
}

#[derive(Debug, Serialize)]
struct DiagnosticsBundle {
    generated_at: String,
    app_version: &'static str,
    git_commit: Option<&'static str>,
    config_snapshot: ConfigSnapshot,
    import_warnings: Vec<ImportWarningSummary>,
    logs: Vec<crate::DebugLogEntry>,
    redactions_applied: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ImportWarningSummary {
    job_id: String,
    warnings: Vec<String>,
    warning_details: Vec<crate::routes::import_export::ImportWarningDetail>,
    completed_at: Option<String>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/health/deps", get(debug_health_deps))
        .route("/metrics", get(debug_metrics))
        .route("/logs", get(debug_logs))
        .route("/bundle", get(debug_bundle))
}

fn ensure_debug_enabled(state: &AppState) -> Result<(), ApiError> {
    if state.debug_route_available() {
        Ok(())
    } else {
        Err(ApiError::NotFound(
            "debug endpoints are disabled".to_string(),
        ))
    }
}

async fn debug_health_deps(
    State(state): State<AppState>,
) -> Result<Json<DebugHealthDepsResponse>, ApiError> {
    ensure_debug_enabled(&state)?;

    let db_path = resolve_db_path();
    let media_dir = resolve_media_dir();

    let db_status = if let Some(backend) = state.sqlite_backend.as_ref() {
        let result = backend.with_connection(|conn| {
            conn.query_row("SELECT 1", [], |row| row.get::<_, i64>(0))
                .map(|_| ())
                .map_err(|e| rustygene_storage::StorageError {
                    code: rustygene_storage::StorageErrorCode::Backend,
                    message: format!("database connectivity check failed: {e}"),
                })
        });

        match result {
            Ok(()) => DependencyStatus {
                ok: true,
                detail: format!("reachable ({})", redact_path(&db_path)),
            },
            Err(err) => DependencyStatus {
                ok: false,
                detail: err.message,
            },
        }
    } else {
        DependencyStatus {
            ok: false,
            detail: "sqlite backend unavailable".to_string(),
        }
    };

    let migration = check_migrations(&state).await;
    let media_status = match fs::create_dir_all(&media_dir).and_then(|_| {
        let probe = media_dir.join(".rustygene-write-probe");
        fs::write(&probe, b"ok")?;
        fs::remove_file(probe)
    }) {
        Ok(()) => DependencyStatus {
            ok: true,
            detail: format!("writable ({})", redact_path(&media_dir)),
        },
        Err(err) => DependencyStatus {
            ok: false,
            detail: format!("not writable: {err}"),
        },
    };

    Ok(Json(DebugHealthDepsResponse {
        debug_enabled: state.debug_route_available(),
        app_version: env!("CARGO_PKG_VERSION"),
        git_commit: option_env!("GIT_COMMIT"),
        api_port: state.port,
        db: db_status,
        migrations: migration,
        media_dir: media_status,
        config_snapshot: ConfigSnapshot {
            db_path: redact_path(&db_path),
            media_dir: redact_path(&media_dir),
            cors_origins: state.cors_origins.clone(),
            sandbox_mode_hint: "query parameter sandbox=1 enables sandbox mode".to_string(),
        },
    }))
}

async fn debug_metrics(
    State(state): State<AppState>,
) -> Result<Json<DebugMetricsResponse>, ApiError> {
    ensure_debug_enabled(&state)?;

    let (mut routes, total_requests) = {
        let metrics = state
            .request_metrics
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let rows = metrics
            .entries
            .iter()
            .map(|(route, (count, total_ms))| RouteMetric {
                route: route.clone(),
                request_count: *count,
                average_latency_ms: if *count == 0 {
                    0.0
                } else {
                    (total_ms / (*count as f64) * 100.0).round() / 100.0
                },
            })
            .collect::<Vec<_>>();

        let total = rows.iter().map(|entry| entry.request_count).sum();
        (rows, total)
    };

    routes.sort_by(|a, b| b.request_count.cmp(&a.request_count));
    let jobs = state.import_jobs.read().await;
    let import_jobs = jobs.values().fold(
        ImportJobCounts {
            queued: 0,
            running: 0,
            completed: 0,
            failed: 0,
        },
        |mut counts, status| {
            match status.status {
                crate::routes::import_export::ImportJobState::Queued => counts.queued += 1,
                crate::routes::import_export::ImportJobState::Running => counts.running += 1,
                crate::routes::import_export::ImportJobState::Completed => counts.completed += 1,
                crate::routes::import_export::ImportJobState::Failed => counts.failed += 1,
            }
            counts
        },
    );

    Ok(Json(DebugMetricsResponse {
        total_requests,
        routes,
        import_jobs,
    }))
}

async fn debug_logs(
    State(state): State<AppState>,
    Query(query): Query<DebugLogsQuery>,
) -> Result<Json<Vec<crate::DebugLogEntry>>, ApiError> {
    ensure_debug_enabled(&state)?;

    let level_filter = query.level.map(|value| value.to_ascii_uppercase());
    let limit = query.limit.unwrap_or(200).min(1000);

    let logs = state
        .debug_logs
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut rows = logs
        .iter()
        .filter(|entry| {
            level_filter
                .as_deref()
                .is_none_or(|level| entry.level.eq_ignore_ascii_case(level))
        })
        .cloned()
        .collect::<Vec<_>>();

    if rows.len() > limit {
        let keep_from = rows.len() - limit;
        rows = rows.split_off(keep_from);
    }

    Ok(Json(rows))
}

async fn debug_bundle(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    ensure_debug_enabled(&state)?;

    let db_path = resolve_db_path();
    let media_dir = resolve_media_dir();

    let jobs = state.import_jobs.read().await;
    let mut warning_rows = jobs
        .values()
        .filter(|status| !status.warnings.is_empty() || !status.warning_details.is_empty())
        .map(|status| ImportWarningSummary {
            job_id: status.job_id.to_string(),
            warnings: status.warnings.clone(),
            warning_details: status.warning_details.clone(),
            completed_at: status.completed_at.map(|value| value.to_rfc3339()),
        })
        .collect::<Vec<_>>();
    warning_rows.sort_by(|a, b| a.job_id.cmp(&b.job_id));

    let logs = state
        .debug_logs
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .cloned()
        .collect::<Vec<_>>();

    let mut redactions = vec![
        "db_path".to_string(),
        "media_dir".to_string(),
        "sensitive_env_values".to_string(),
    ];

    let mut env_snapshot = BTreeMap::new();
    for key in SENSITIVE_ENV_KEYS {
        if std::env::var_os(key).is_some() {
            env_snapshot.insert(key.to_string(), "[REDACTED]".to_string());
        }
    }
    if !env_snapshot.is_empty() {
        redactions.push("sensitive_env_keys_present".to_string());
    }

    let bundle = DiagnosticsBundle {
        generated_at: Utc::now().to_rfc3339(),
        app_version: env!("CARGO_PKG_VERSION"),
        git_commit: option_env!("GIT_COMMIT"),
        config_snapshot: ConfigSnapshot {
            db_path: redact_path(&db_path),
            media_dir: redact_path(&media_dir),
            cors_origins: state.cors_origins.clone(),
            sandbox_mode_hint: format!(
                "sandbox mode controlled by query parameter; env keys redacted count={}.",
                env_snapshot.len()
            ),
        },
        import_warnings: warning_rows,
        logs,
        redactions_applied: redactions,
    };

    let payload = serde_json::to_vec_pretty(&bundle).map_err(|err| {
        ApiError::InternalError(format!("failed to serialize diagnostics bundle: {err}"))
    })?;

    let mut headers = axum::http::HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(
        CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment; filename=\"rustygene-diagnostics.json\""),
    );

    Ok((StatusCode::OK, headers, payload))
}

fn resolve_data_dir() -> PathBuf {
    if let Ok(value) = std::env::var("RUSTYGENE_DATA_DIR") {
        return PathBuf::from(value);
    }

    if let Some(mut path) = dirs::data_local_dir() {
        path.push("rustygene");
        return path;
    }

    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".rustygene")
}

fn resolve_db_path() -> PathBuf {
    if let Ok(value) = std::env::var("RUSTYGENE_DB_PATH") {
        return PathBuf::from(value);
    }
    resolve_data_dir().join("rustygene.db")
}

fn resolve_media_dir() -> PathBuf {
    resolve_data_dir().join("media")
}

fn redact_path(path: &Path) -> String {
    let path_text = path.to_string_lossy().to_string();
    if let Some(home) = dirs::home_dir() {
        let home_text = home.to_string_lossy().to_string();
        if path_text.starts_with(&home_text) {
            return path_text.replacen(&home_text, "~", 1);
        }
    }
    path_text
}

async fn check_migrations(state: &AppState) -> MigrationStatus {
    let expected_tables = [
        "persons",
        "families",
        "events",
        "sources",
        "repositories",
        "media",
        "notes",
        "assertions",
        "staging_queue",
    ];

    let Some(backend) = state.sqlite_backend.as_ref() else {
        return MigrationStatus {
            ok: false,
            present_tables: 0,
            missing_tables: expected_tables.iter().map(ToString::to_string).collect(),
        };
    };

    let present = backend.with_connection(|conn| {
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table'")
            .map_err(|e| rustygene_storage::StorageError {
                code: rustygene_storage::StorageErrorCode::Backend,
                message: format!("prepare schema query failed: {e}"),
            })?;

        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| rustygene_storage::StorageError {
                code: rustygene_storage::StorageErrorCode::Backend,
                message: format!("query schema names failed: {e}"),
            })?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| rustygene_storage::StorageError {
                code: rustygene_storage::StorageErrorCode::Backend,
                message: format!("collect schema names failed: {e}"),
            })
    });

    match present {
        Ok(values) => {
            let table_set = values
                .into_iter()
                .collect::<std::collections::BTreeSet<_>>();
            let missing = expected_tables
                .iter()
                .filter(|name| !table_set.contains(**name))
                .map(|name| (*name).to_string())
                .collect::<Vec<_>>();

            MigrationStatus {
                ok: missing.is_empty(),
                present_tables: table_set.len(),
                missing_tables: missing,
            }
        }
        Err(_) => MigrationStatus {
            ok: false,
            present_tables: 0,
            missing_tables: expected_tables.iter().map(ToString::to_string).collect(),
        },
    }
}
