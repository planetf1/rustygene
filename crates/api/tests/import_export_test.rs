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
use serde::{Deserialize, Serialize};
use zip::ZipArchive;

#[derive(Debug, Deserialize)]
struct ImportAcceptedResponse {
    job_id: String,
    status_url: String,
}

#[derive(Debug, Deserialize)]
struct ImportWarningDetailResponse {
    code: String,
    title: String,
    counts: std::collections::BTreeMap<String, usize>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ImportJobStatusResponse {
    job_id: String,
    status: String,
    progress_pct: u8,
    entities_imported: Option<usize>,
    entities_imported_by_type: Option<std::collections::BTreeMap<String, usize>>,
    errors: Vec<String>,
    warnings: Vec<String>,
    warning_details: Vec<ImportWarningDetailResponse>,
    log_messages: Vec<String>,
    completed_at: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct MergeSelection {
    entity_type: String,
    entity_id: String,
    field: String,
    new_value: serde_json::Value,
    source: Option<String>,
    confidence: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct MergeDiffFieldPreview {
    entity_id: String,
    field: String,
    old_value: serde_json::Value,
    new_value: serde_json::Value,
    source: String,
    confidence: f64,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct MergeNewEntityPreview {
    entity_id: String,
    label: String,
    xref: Option<String>,
    fields: Vec<MergeSelection>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct MergeDiffResponse {
    changed_fields: Vec<MergeDiffFieldPreview>,
    new_entities: Vec<MergeNewEntityPreview>,
    unchanged_entities: usize,
}

#[derive(Debug, Serialize)]
struct ImportMergeRequest {
    selected_changes: Vec<MergeSelection>,
    submitted_by: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ImportMergeResponse {
    proposals_created: usize,
    proposal_ids: Vec<String>,
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
    let counts = status
        .entities_imported_by_type
        .expect("completed import should include entity counts by type");
    assert!(counts.get("person").copied().unwrap_or(0) > 0);
    assert!(counts.get("family").copied().unwrap_or(0) > 0);
    assert!(
        status
            .log_messages
            .iter()
            .any(|message| message.contains("Import completed")),
        "completed import should include final log message"
    );

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
    assert!(
        status.errors.iter().any(|err| {
            err.contains("tokenize failed")
                || err.contains("tree build failed")
                || err.contains("serialization failed")
                || err.contains("sqlite failed")
        }),
        "failed import error should include parser/storage context, got: {:?}",
        status.errors
    );
    assert!(
        status
            .log_messages
            .iter()
            .any(|message| message.contains("Import failed")),
        "failed import should include failure log message"
    );

    server.shutdown().await.expect("shutdown server");
}

#[tokio::test]
async fn importing_same_gedcom_twice_completes_without_unique_constraint_failure() {
    let backend = in_memory_backend();
    let state = AppState::with_default_cors_sqlite(backend, 0).expect("build app state");

    let server = start_server(state, 0).await.expect("start server");
    let client = reqwest::Client::new();
    let content = include_str!("../../../testdata/gedcom/simpsons.ged");

    for run in 1..=2 {
        let part = reqwest::multipart::Part::text(content.to_string())
            .file_name(format!("simpsons-run-{run}.ged"));
        let form = reqwest::multipart::Form::new()
            .text("format", "gedcom")
            .part("file", part);

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
        assert_eq!(
            status.status, "completed",
            "run {run} failed with errors: {:?}",
            status.errors
        );
        assert!(
            status.errors.is_empty(),
            "run {run} should not report errors: {:?}",
            status.errors
        );
        assert!(
            status
                .errors
                .iter()
                .all(|err| !err.contains("UNIQUE constraint failed")),
            "run {run} should not surface unique-constraint failures: {:?}",
            status.errors
        );
    }

    server.shutdown().await.expect("shutdown server");
}

#[tokio::test]
async fn import_status_surfaces_warning_details_for_unhandled_tags() {
    let backend = in_memory_backend();
    let state = AppState::with_default_cors_sqlite(backend, 0).expect("build app state");

    let server = start_server(state, 0).await.expect("start server");

    let content = "0 HEAD\n1 SOUR TEST\n1 GEDC\n2 VERS 5.5.1\n1 CHAR UTF-8\n0 @I1@ INDI\n1 NAME Test /Person/\n1 SEX M\n1 _CUSTOM should-survive\n0 TRLR\n";
    let part = reqwest::multipart::Part::text(content.to_string()).file_name("warning.ged");
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
    assert_eq!(
        status.status, "completed",
        "job errors: {:?}",
        status.errors
    );
    assert!(
        status
            .warnings
            .iter()
            .any(|warning| warning.contains("Unhandled custom GEDCOM tags")),
        "warnings should mention custom GEDCOM tags"
    );
    let custom_detail = status
        .warning_details
        .iter()
        .find(|detail| detail.code == "unhandled_custom_tags")
        .expect("warning details should include custom tag counts");
    assert_eq!(custom_detail.title, "Unhandled custom GEDCOM tags");
    assert!(
        custom_detail.counts.get("_CUSTOM").copied().unwrap_or(0) >= 1,
        "custom tag should be counted in warning details"
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

#[tokio::test]
async fn merge_diff_and_selective_merge_create_staging_only() {
    let backend = in_memory_backend();
    let state = AppState::with_default_cors_sqlite(backend, 0).expect("build app state");
    let server = start_server(state, 0).await.expect("start server");

    let client = reqwest::Client::new();
    let base_gedcom = include_str!("../../../testdata/gedcom/kennedy.ged");

    let import_form = reqwest::multipart::Form::new()
        .text("format", "gedcom")
        .part(
            "file",
            reqwest::multipart::Part::text(base_gedcom.to_string()).file_name("kennedy.ged"),
        );

    let accepted = client
        .post(format!("http://{}/api/v1/import", server.local_addr))
        .multipart(import_form)
        .send()
        .await
        .expect("start base import");
    assert_eq!(accepted.status(), StatusCode::ACCEPTED);
    let accepted_body: ImportAcceptedResponse = accepted.json().await.expect("parse accepted");

    for _ in 0..300 {
        let status: ImportJobStatusResponse = client
            .get(format!(
                "http://{}/api/v1/import/{}",
                server.local_addr, accepted_body.job_id
            ))
            .send()
            .await
            .expect("poll import")
            .json()
            .await
            .expect("parse job status");
        if status.status == "completed" {
            break;
        }
        assert_ne!(status.status, "failed", "base import should succeed");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let pre_merge_persons: serde_json::Value = client
        .get(format!("http://{}/api/v1/persons", server.local_addr))
        .send()
        .await
        .expect("list persons before selective merge")
        .json()
        .await
        .expect("parse persons before selective merge");
    let pre_merge_person_count = pre_merge_persons
        .as_array()
        .map_or(0, std::vec::Vec::len);

    let modified = build_modified_kennedy_fixture(base_gedcom);
    let diff_form = reqwest::multipart::Form::new()
        .text("format", "gedcom")
        .part(
            "file",
            reqwest::multipart::Part::text(modified).file_name("kennedy-modified.ged"),
        );

    let diff_resp = client
        .post(format!("http://{}/api/v1/import/diff", server.local_addr))
        .multipart(diff_form)
        .send()
        .await
        .expect("request import diff");
    assert_eq!(diff_resp.status(), StatusCode::OK);

    let diff: MergeDiffResponse = diff_resp.json().await.expect("parse diff response");
    assert!(
        !diff.changed_fields.is_empty() || !diff.new_entities.is_empty(),
        "diff should detect at least one change"
    );

    let mut selected_changes = Vec::new();
    if let Some(first_changed) = diff.changed_fields.first() {
        selected_changes.push(MergeSelection {
            entity_type: "person".to_string(),
            entity_id: first_changed.entity_id.clone(),
            field: first_changed.field.clone(),
            new_value: first_changed.new_value.clone(),
            source: Some(first_changed.source.clone()),
            confidence: Some(first_changed.confidence),
        });
    }
    assert!(
        !selected_changes.is_empty(),
        "test must select at least one changed-field proposal"
    );

    let merge_resp = client
        .post(format!("http://{}/api/v1/import/merge", server.local_addr))
        .json(&ImportMergeRequest {
            selected_changes: selected_changes.clone(),
            submitted_by: Some("test:selective-merge".to_string()),
        })
        .send()
        .await
        .expect("submit selective merge");
    assert_eq!(merge_resp.status(), StatusCode::CREATED);

    let merge_result: ImportMergeResponse = merge_resp.json().await.expect("parse merge response");
    assert_eq!(merge_result.proposals_created, selected_changes.len());
    assert_eq!(merge_result.proposal_ids.len(), selected_changes.len());

    let staging_entries: serde_json::Value = client
        .get(format!("http://{}/api/v1/staging", server.local_addr))
        .send()
        .await
        .expect("list staging entries")
        .json()
        .await
        .expect("parse staging entries");
    assert!(
        staging_entries
            .as_array()
            .is_some_and(|items| items.len() >= selected_changes.len()),
        "staging queue should contain submitted proposals"
    );

    let post_merge_persons: serde_json::Value = client
        .get(format!("http://{}/api/v1/persons", server.local_addr))
        .send()
        .await
        .expect("list persons after merge submit")
        .json()
        .await
        .expect("parse post-merge persons");
    let post_merge_person_count = post_merge_persons
        .as_array()
        .map_or(0, std::vec::Vec::len);

    assert_eq!(
        post_merge_person_count, pre_merge_person_count,
        "selective merge submit must not directly mutate canonical entities"
    );

    server.shutdown().await.expect("shutdown server");
}

fn build_modified_kennedy_fixture(base: &str) -> String {
    let mut lines = base.lines().map(str::to_string).collect::<Vec<_>>();

    let mut seen_birth = false;
    for line in &mut lines {
        if line.starts_with("1 BIRT") {
            seen_birth = true;
            continue;
        }

        if seen_birth && line.starts_with("2 DATE ") {
            *line = "2 DATE 1 JAN 1901".to_string();
            break;
        }

        if line.starts_with("1 ") && !line.starts_with("1 BIRT") {
            seen_birth = false;
        }
    }

    let insert_pos = lines
        .iter()
        .position(|line| line.trim() == "0 TRLR")
        .unwrap_or(lines.len());

    let new_person_block = [
        "0 @I9999@ INDI",
        "1 NAME Merge /Candidate/",
        "1 SEX U",
        "1 BIRT",
        "2 DATE 1 JAN 2000",
    ];

    for (offset, line) in new_person_block.iter().enumerate() {
        lines.insert(insert_pos + offset, (*line).to_string());
    }

    format!("{}\n", lines.join("\n"))
}
