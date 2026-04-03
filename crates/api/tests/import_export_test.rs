use std::io::{Cursor, Read};
use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;
use std::time::Duration;

use reqwest::StatusCode;
use rusqlite::Connection;
use rustygene_api::{start_server, AppState};
use rustygene_gedcom::{build_gedcom_tree, tokenize_gedcom};
use rustygene_storage::run_migrations;
use rustygene_storage::sqlite_impl::SqliteBackend;
use serde::Deserialize;
use zip::ZipArchive;

#[derive(Debug, Deserialize)]
struct ImportAcceptedResponse {
    job_id: String,
    status_url: String,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ImportJobStatusResponse {
    job_id: String,
    status: String,
    progress_pct: u8,
    entities_imported: Option<usize>,
    errors: Vec<String>,
    warnings: Vec<String>,
    completed_at: Option<String>,
}

fn in_memory_backend() -> Arc<SqliteBackend> {
    let mut conn = Connection::open_in_memory().expect("open in-memory sqlite connection");
    run_migrations(&mut conn).expect("run sqlite migrations");
    Arc::new(SqliteBackend::new(conn))
}

#[tokio::test]
async fn import_kennedy_gedcom_completes_and_reports_entities() {
    let backend = in_memory_backend();
    let state = AppState::with_default_cors_sqlite(backend, 0).expect("build app state");

    let server = start_server(state, 0).await.expect("start server");
    assert_eq!(server.local_addr.ip(), IpAddr::V4(Ipv4Addr::LOCALHOST));

    let content = include_str!("../../../testdata/gedcom/kennedy.ged");
    let part = reqwest::multipart::Part::text(content.to_string()).file_name("kennedy.ged");
    let form = reqwest::multipart::Form::new()
        .text("format", "gedcom")
        .part("file", part);

    let client = reqwest::Client::new();
    let accepted = client
        .post(format!("http://{}/api/v1/import", server.local_addr))
        .multipart(form)
        .send()
        .await
        .expect("post import request");

    assert_eq!(accepted.status(), StatusCode::ACCEPTED);
    let body: ImportAcceptedResponse = accepted.json().await.expect("parse accepted body");
    assert!(
        body.status_url.contains(&body.job_id),
        "status_url should include job id"
    );

    let mut completed: Option<ImportJobStatusResponse> = None;
    for _ in 0..300 {
        let response = client
            .get(format!(
                "http://{}/api/v1/import/{}",
                server.local_addr, body.job_id
            ))
            .send()
            .await
            .expect("poll import status");
        assert_eq!(response.status(), StatusCode::OK);
        let status: ImportJobStatusResponse = response.json().await.expect("parse status body");

        if status.status == "completed" || status.status == "failed" {
            completed = Some(status);
            break;
        }

        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let status = completed.expect("import job should complete or fail within poll budget");
    assert_eq!(
        status.status, "completed",
        "job errors: {:?}",
        status.errors
    );
    assert!(status.entities_imported.unwrap_or(0) > 0);

    server.shutdown().await.expect("shutdown server");
}

#[tokio::test]
async fn invalid_gedcom_import_fails_as_job_not_http_500() {
    let backend = in_memory_backend();
    let state = AppState::with_default_cors_sqlite(backend, 0).expect("build app state");

    let server = start_server(state, 0).await.expect("start server");

    let invalid_payload = "not a valid gedcom payload";
    let part = reqwest::multipart::Part::text(invalid_payload.to_string()).file_name("broken.ged");
    let form = reqwest::multipart::Form::new()
        .text("format", "gedcom")
        .part("file", part);

    let client = reqwest::Client::new();
    let accepted = client
        .post(format!("http://{}/api/v1/import", server.local_addr))
        .multipart(form)
        .send()
        .await
        .expect("post import request");

    assert_eq!(accepted.status(), StatusCode::ACCEPTED);
    let body: ImportAcceptedResponse = accepted.json().await.expect("parse accepted body");

    let mut completed: Option<ImportJobStatusResponse> = None;
    for _ in 0..300 {
        let response = client
            .get(format!(
                "http://{}/api/v1/import/{}",
                server.local_addr, body.job_id
            ))
            .send()
            .await
            .expect("poll import status");
        assert_eq!(response.status(), StatusCode::OK);
        let status: ImportJobStatusResponse = response.json().await.expect("parse status body");

        if status.status == "completed" || status.status == "failed" {
            completed = Some(status);
            break;
        }

        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let status = completed.expect("import job should complete or fail within poll budget");
    assert_eq!(status.status, "failed");
    assert!(
        !status.errors.is_empty(),
        "failed job should expose at least one error"
    );

    server.shutdown().await.expect("shutdown server");
}

#[tokio::test]
async fn export_gedcom_is_parseable_and_bundle_has_manifest() {
    let backend = in_memory_backend();
    let state = AppState::with_default_cors_sqlite(backend, 0).expect("build app state");

    let server = start_server(state, 0).await.expect("start server");

    let content = include_str!("../../../testdata/gedcom/kennedy.ged");
    let part = reqwest::multipart::Part::text(content.to_string()).file_name("kennedy.ged");
    let form = reqwest::multipart::Form::new()
        .text("format", "gedcom")
        .part("file", part);

    let client = reqwest::Client::new();
    let accepted = client
        .post(format!("http://{}/api/v1/import", server.local_addr))
        .multipart(form)
        .send()
        .await
        .expect("post import request");
    assert_eq!(accepted.status(), StatusCode::ACCEPTED);
    let body: ImportAcceptedResponse = accepted.json().await.expect("parse accepted body");

    for _ in 0..300 {
        let response = client
            .get(format!(
                "http://{}/api/v1/import/{}",
                server.local_addr, body.job_id
            ))
            .send()
            .await
            .expect("poll import status");
        let status: ImportJobStatusResponse = response.json().await.expect("parse status body");
        if status.status == "completed" {
            break;
        }
        assert_ne!(status.status, "failed", "import should succeed");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let gedcom_response = client
        .get(format!(
            "http://{}/api/v1/export?format=gedcom&redact_living=true",
            server.local_addr
        ))
        .send()
        .await
        .expect("export gedcom");
    assert_eq!(gedcom_response.status(), StatusCode::OK);

    let gedcom_text = gedcom_response.text().await.expect("read gedcom body");
    let lines = tokenize_gedcom(&gedcom_text).expect("tokenize exported gedcom");
    let roots = build_gedcom_tree(&lines).expect("build tree for exported gedcom");
    assert!(!roots.is_empty(), "exported GEDCOM should not be empty");

    let bundle_response = client
        .get(format!(
            "http://{}/api/v1/export?format=bundle",
            server.local_addr
        ))
        .send()
        .await
        .expect("export bundle");
    assert_eq!(bundle_response.status(), StatusCode::OK);

    let zip_bytes = bundle_response.bytes().await.expect("read bundle body");
    let cursor = Cursor::new(zip_bytes);
    let mut archive = ZipArchive::new(cursor).expect("open zip archive");

    let mut manifest = String::new();
    archive
        .by_name("manifest.json")
        .expect("manifest.json present")
        .read_to_string(&mut manifest)
        .expect("read manifest.json");

    let manifest_json: serde_json::Value =
        serde_json::from_str(&manifest).expect("manifest is valid JSON");
    assert!(
        manifest_json
            .get("entity_counts")
            .and_then(serde_json::Value::as_object)
            .is_some(),
        "manifest should include entity_counts"
    );

    server.shutdown().await.expect("shutdown server");
}
