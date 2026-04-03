use std::sync::Arc;
use std::time::Duration;

use reqwest::StatusCode;
use rusqlite::Connection;
use rustygene_api::{start_server, AppState, ServerHandle};
use rustygene_storage::run_migrations;
use rustygene_storage::sqlite_impl::SqliteBackend;
use serde::Deserialize;

pub struct TestServer {
    pub server: ServerHandle,
    pub client: reqwest::Client,
    pub base_url: String,
}

impl TestServer {
    pub async fn shutdown(self) {
        self.server.shutdown().await.expect("shutdown server");
    }
}

#[derive(Debug, Deserialize)]
struct ImportAcceptedResponse {
    job_id: String,
}

#[derive(Debug, Deserialize)]
struct ImportJobStatusResponse {
    status: String,
}

fn in_memory_backend() -> Arc<SqliteBackend> {
    let mut conn = Connection::open_in_memory().expect("open in-memory sqlite connection");
    run_migrations(&mut conn).expect("run sqlite migrations");
    Arc::new(SqliteBackend::new(conn))
}

pub async fn spawn_test_server() -> TestServer {
    let backend = in_memory_backend();
    let state = AppState::with_default_cors_sqlite(backend, 0).expect("build app state");
    let server = start_server(state, 0).await.expect("start server");

    TestServer {
        base_url: format!("http://{}", server.local_addr),
        server,
        client: reqwest::Client::new(),
    }
}

pub async fn spawn_test_server_with_kennedy_import() -> TestServer {
    let harness = spawn_test_server().await;

    let content = include_str!("../../../../testdata/gedcom/kennedy.ged");
    let part = reqwest::multipart::Part::text(content.to_string()).file_name("kennedy.ged");
    let form = reqwest::multipart::Form::new()
        .text("format", "gedcom")
        .part("file", part);

    let accepted = harness
        .client
        .post(format!("{}/api/v1/import", harness.base_url))
        .multipart(form)
        .send()
        .await
        .expect("start import job");
    assert_eq!(accepted.status(), StatusCode::ACCEPTED);

    let accepted_body: ImportAcceptedResponse = accepted.json().await.expect("accepted body");
    wait_for_import_completion(&harness, &accepted_body.job_id).await;

    harness
}

pub async fn wait_for_import_completion(harness: &TestServer, job_id: &str) {
    for _ in 0..300 {
        let response = harness
            .client
            .get(format!("{}/api/v1/import/{job_id}", harness.base_url))
            .send()
            .await
            .expect("poll import status");

        let status: ImportJobStatusResponse = response.json().await.expect("parse status body");
        if status.status == "completed" {
            return;
        }
        assert_ne!(status.status, "failed", "import unexpectedly failed");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    panic!("import did not complete within timeout");
}
