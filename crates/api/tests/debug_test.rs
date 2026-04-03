use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;

use reqwest::StatusCode;
use rusqlite::Connection;
use rustygene_api::{start_server, AppState};
use rustygene_storage::run_migrations;
use rustygene_storage::sqlite_impl::SqliteBackend;

fn in_memory_backend() -> Arc<SqliteBackend> {
    let mut conn = Connection::open_in_memory().expect("open in-memory sqlite connection");
    run_migrations(&mut conn).expect("run sqlite migrations");
    Arc::new(SqliteBackend::new(conn))
}

#[tokio::test]
async fn debug_endpoints_work_when_enabled() {
    let backend = in_memory_backend();
    let state = AppState::with_default_cors_sqlite(backend, 0).expect("build app state");

    let server = start_server(state, 0).await.expect("start server");
    assert_eq!(server.local_addr.ip(), IpAddr::V4(Ipv4Addr::LOCALHOST));

    let client = reqwest::Client::new();
    let deps = client
        .get(format!(
            "http://{}/api/v1/debug/health/deps",
            server.local_addr
        ))
        .send()
        .await
        .expect("call debug deps");
    assert_eq!(deps.status(), StatusCode::OK);

    let metrics = client
        .get(format!("http://{}/api/v1/debug/metrics", server.local_addr))
        .send()
        .await
        .expect("call debug metrics");
    assert_eq!(metrics.status(), StatusCode::OK);

    let logs = client
        .get(format!(
            "http://{}/api/v1/debug/logs?limit=50",
            server.local_addr
        ))
        .send()
        .await
        .expect("call debug logs");
    assert_eq!(logs.status(), StatusCode::OK);

    server.shutdown().await.expect("shutdown server");
}

#[tokio::test]
async fn debug_endpoints_unavailable_when_state_disables_them() {
    let backend = in_memory_backend();
    let mut state = AppState::with_default_cors_sqlite(backend, 0).expect("build app state");
    state.debug_enabled = false;

    let server = start_server(state, 0).await.expect("start server");

    let client = reqwest::Client::new();
    let response = client
        .get(format!(
            "http://{}/api/v1/debug/health/deps",
            server.local_addr
        ))
        .send()
        .await
        .expect("call disabled debug deps");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    server.shutdown().await.expect("shutdown server");
}

#[tokio::test]
async fn diagnostics_bundle_redacts_secret_environment_values() {
    let backend = in_memory_backend();
    let state = AppState::with_default_cors_sqlite(backend, 0).expect("build app state");

    let secret = "super-secret-token-value";
    std::env::set_var("OPENAI_API_KEY", secret);

    let server = start_server(state, 0).await.expect("start server");

    let client = reqwest::Client::new();
    let response = client
        .get(format!("http://{}/api/v1/debug/bundle", server.local_addr))
        .send()
        .await
        .expect("download diagnostics bundle");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.text().await.expect("read diagnostics body");
    assert!(
        !body.contains(secret),
        "diagnostics bundle must not include secret env values"
    );
    assert!(
        body.contains("[REDACTED]") || body.contains("redactions_applied"),
        "diagnostics bundle should indicate redaction"
    );

    std::env::remove_var("OPENAI_API_KEY");

    server.shutdown().await.expect("shutdown server");
}
