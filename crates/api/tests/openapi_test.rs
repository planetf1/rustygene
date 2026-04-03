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
async fn openapi_json_endpoint_returns_valid_openapi_document() {
    let backend = in_memory_backend();
    let state = AppState::with_default_cors_sqlite(backend, 0).expect("build app state");

    let server = start_server(state, 0).await.expect("start server");
    assert_eq!(server.local_addr.ip(), IpAddr::V4(Ipv4Addr::LOCALHOST));

    let client = reqwest::Client::new();
    let response = client
        .get(format!("http://{}/api/v1/openapi.json", server.local_addr))
        .send()
        .await
        .expect("fetch openapi json");

    assert_eq!(response.status(), StatusCode::OK);

    let body: serde_json::Value = response.json().await.expect("parse openapi json");
    assert!(
        body.get("openapi")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|version| version.starts_with('3')),
        "openapi version field is missing or invalid"
    );

    let paths = body
        .get("paths")
        .and_then(serde_json::Value::as_object)
        .expect("openapi paths map");

    for expected in [
        "/api/v1/persons",
        "/api/v1/families",
        "/api/v1/events",
        "/api/v1/search",
        "/api/v1/graph/ancestors/{id}",
        "/api/v1/sources",
        "/api/v1/citations",
        "/api/v1/repositories",
        "/api/v1/notes",
        "/api/v1/research-log",
        "/api/v1/media",
        "/api/v1/import",
        "/api/v1/events/stream",
    ] {
        assert!(
            paths.contains_key(expected),
            "missing path in spec: {expected}"
        );
    }

    server.shutdown().await.expect("shutdown server");
}
