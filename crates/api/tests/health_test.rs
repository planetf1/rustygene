use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;

use axum::body::Body;
use axum::http::Request;
use http_body_util::BodyExt;
use reqwest::StatusCode;
use rusqlite::Connection;
use rustygene_api::{build_router, start_server, AppState, REQUEST_BODY_LIMIT_BYTES};
use rustygene_storage::run_migrations;
use rustygene_storage::sqlite_impl::SqliteBackend;
use tower::util::ServiceExt;

fn in_memory_backend() -> Arc<SqliteBackend> {
    let mut conn = Connection::open_in_memory().expect("open in-memory sqlite connection");
    run_migrations(&mut conn).expect("run sqlite migrations");
    Arc::new(SqliteBackend::new(conn))
}

#[tokio::test]
async fn health_endpoint_returns_ok_and_version() {
    let backend = in_memory_backend();
    let state = AppState::with_default_cors(backend, 0).expect("build app state");

    let server = start_server(state, 0).await.expect("start server");
    assert_eq!(server.local_addr.ip(), IpAddr::V4(Ipv4Addr::LOCALHOST));

    let client = reqwest::Client::new();
    let response = client
        .get(format!("http://{}/api/v1/health", server.local_addr))
        .send()
        .await
        .expect("call health endpoint");

    assert_eq!(response.status(), StatusCode::OK);

    let body: serde_json::Value = response.json().await.expect("parse json response");
    assert_eq!(
        body.get("status").and_then(serde_json::Value::as_str),
        Some("ok")
    );
    assert!(
        body.get("version")
            .and_then(serde_json::Value::as_str)
            .is_some(),
        "health response must contain version"
    );

    server.shutdown().await.expect("shutdown server");
}

#[tokio::test]
async fn request_body_over_configured_limit_returns_413() {
    let backend = in_memory_backend();
    let state = AppState::with_default_cors(backend, 0).expect("build app state");
    let oversized_content_length = REQUEST_BODY_LIMIT_BYTES + 1;

    let response = build_router(state)
        .oneshot(
            Request::post("/api/v1/import-export/import")
                .header("content-length", oversized_content_length.to_string())
                .body(Body::empty())
                .expect("build oversized request"),
        )
        .await
        .expect("route oversized request");

    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    let body = response
        .into_body()
        .collect()
        .await
        .expect("read response body")
        .to_bytes();
    assert!(
        !body.is_empty(),
        "payload-too-large response should include a response body"
    );
}

#[tokio::test]
async fn health_response_includes_security_headers_and_restricted_cors() {
    let backend = in_memory_backend();
    let state = AppState::with_default_cors(backend, 0).expect("build app state");

    let server = start_server(state, 0).await.expect("start server");
    assert_eq!(server.local_addr.ip(), IpAddr::V4(Ipv4Addr::LOCALHOST));

    let client = reqwest::Client::new();
    let allowed = "http://localhost";
    let disallowed = "https://evil.example";

    let allowed_response = client
        .get(format!("http://{}/api/v1/health", server.local_addr))
        .header("origin", allowed)
        .send()
        .await
        .expect("call health endpoint with allowed origin");

    assert_eq!(
        allowed_response
            .headers()
            .get("x-content-type-options")
            .and_then(|v| v.to_str().ok()),
        Some("nosniff")
    );
    assert_eq!(
        allowed_response
            .headers()
            .get("x-frame-options")
            .and_then(|v| v.to_str().ok()),
        Some("DENY")
    );
    assert_eq!(
        allowed_response
            .headers()
            .get("x-xss-protection")
            .and_then(|v| v.to_str().ok()),
        Some("1; mode=block")
    );
    assert_eq!(
        allowed_response
            .headers()
            .get("referrer-policy")
            .and_then(|v| v.to_str().ok()),
        Some("no-referrer")
    );
    assert_eq!(
        allowed_response
            .headers()
            .get("access-control-allow-origin")
            .and_then(|v| v.to_str().ok()),
        Some(allowed)
    );

    let disallowed_response = client
        .get(format!("http://{}/api/v1/health", server.local_addr))
        .header("origin", disallowed)
        .send()
        .await
        .expect("call health endpoint with disallowed origin");

    assert_eq!(disallowed_response.status(), StatusCode::OK);
    assert!(
        disallowed_response
            .headers()
            .get("access-control-allow-origin")
            .is_none(),
        "disallowed origin must not be reflected"
    );

    server.shutdown().await.expect("shutdown server");
}

#[tokio::test]
async fn health_response_allows_tauri_dev_origin_localhost_5173() {
    let backend = in_memory_backend();
    let state = AppState::with_default_cors(backend, 0).expect("build app state");

    let server = start_server(state, 0).await.expect("start server");
    assert_eq!(server.local_addr.ip(), IpAddr::V4(Ipv4Addr::LOCALHOST));

    let client = reqwest::Client::new();
    let response = client
        .get(format!("http://{}/api/v1/health", server.local_addr))
        .header("origin", "http://localhost:5173")
        .send()
        .await
        .expect("call health endpoint from tauri dev origin");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("access-control-allow-origin")
            .and_then(|v| v.to_str().ok()),
        Some("http://localhost:5173")
    );

    server.shutdown().await.expect("shutdown server");
}
