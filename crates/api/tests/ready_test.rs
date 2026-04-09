mod common;

use axum::http::StatusCode;
use rustygene_api::AppState;
use std::sync::Arc;

#[tokio::test]
async fn test_ready_endpoint_200_when_backend_active() {
    let harness = common::spawn_test_server().await;
    let client = reqwest::Client::new();
    let base = &harness.base_url;

    let response = client
        .get(format!("{base}/api/v1/ready"))
        .send()
        .await
        .expect("send request");

    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = response.json().await.expect("parse json");
    assert_eq!(body["status"], "ready");

    harness.shutdown().await;
}

#[tokio::test]
async fn test_ready_endpoint_503_when_no_backend() {
    // Manually build a server without a sqlite_backend
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let port = 0;
    
    // We need a dummy storage for AppState construction
    let backend = Arc::new(rustygene_storage::sqlite_impl::SqliteBackend::new(
        rusqlite::Connection::open_in_memory().unwrap()
    ));
    
    // Create state WITHOUT the sqlite_backend Arc (passing None)
    let state = AppState::new(
        backend.clone(),
        None, // This triggers the 503 path in ready_handler
        port,
        vec!["http://localhost".to_string()]
    ).expect("build state");

    let bind_addr = std::net::SocketAddr::from(([127, 0, 0, 1], 0));
    let listener = tokio::net::TcpListener::bind(bind_addr).await.unwrap();
    let local_addr = listener.local_addr().unwrap();
    let router = rustygene_api::build_router(state);

    let task = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            })
            .await
    });

    let client = reqwest::Client::new();
    let base = format!("http://{}", local_addr);

    let response = client
        .get(format!("{base}/api/v1/ready"))
        .send()
        .await
        .expect("send request");

    // Verify 503 and structured error
    common::assert_api_error(response, StatusCode::SERVICE_UNAVAILABLE, "unavailable").await;

    let _ = shutdown_tx.send(());
    task.await.unwrap().unwrap();
}
