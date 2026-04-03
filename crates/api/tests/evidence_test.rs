use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;
use std::time::Duration;

use reqwest::StatusCode;
use rusqlite::Connection;
use rustygene_api::{start_server, AppState};
use rustygene_storage::run_migrations;
use rustygene_storage::sqlite_impl::SqliteBackend;
use serde::Deserialize;

fn in_memory_backend() -> Arc<SqliteBackend> {
    let mut conn = Connection::open_in_memory().expect("open in-memory sqlite connection");
    run_migrations(&mut conn).expect("run sqlite migrations");
    Arc::new(SqliteBackend::new(conn))
}

#[derive(Debug, Deserialize)]
struct ImportAcceptedResponse {
    job_id: String,
}

#[derive(Debug, Deserialize)]
struct ImportJobStatusResponse {
    status: String,
    errors: Vec<String>,
}

async fn wait_for_import_completion(client: &reqwest::Client, base_url: &str, job_id: &str) {
    for _ in 0..300 {
        let response = client
            .get(format!("{base_url}/api/v1/import/{job_id}"))
            .send()
            .await
            .expect("poll import status");
        assert_eq!(response.status(), StatusCode::OK);
        let status: ImportJobStatusResponse = response.json().await.expect("parse status");

        if status.status == "completed" {
            return;
        }

        if status.status == "failed" {
            panic!("import failed: {:?}", status.errors);
        }

        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    panic!("import did not complete in poll budget");
}

#[tokio::test]
async fn source_and_citation_round_trip_links_citation_to_assertion() {
    let backend = in_memory_backend();
    let state = AppState::with_default_cors_sqlite(backend, 0).expect("build app state");
    let server = start_server(state, 0).await.expect("start server");
    assert_eq!(server.local_addr.ip(), IpAddr::V4(Ipv4Addr::LOCALHOST));

    let client = reqwest::Client::new();
    let base_url = format!("http://{}", server.local_addr);

    let person = client
        .post(format!("{base_url}/api/v1/persons"))
        .json(&serde_json::json!({
            "given_names": ["John"],
            "surnames": [{"value": "Smith", "origin_type": "patrilineal", "connector": null}],
            "gender": "male"
        }))
        .send()
        .await
        .expect("create person");
    assert_eq!(person.status(), StatusCode::CREATED);
    let person_body: serde_json::Value = person.json().await.expect("person body");
    let person_id = person_body
        .get("id")
        .and_then(serde_json::Value::as_str)
        .expect("person id");

    let assertion = client
        .post(format!("{base_url}/api/v1/persons/{person_id}/assertions"))
        .json(&serde_json::json!({
            "field": "occupation",
            "value": "Carpenter"
        }))
        .send()
        .await
        .expect("create assertion");
    assert_eq!(assertion.status(), StatusCode::CREATED);
    let assertion_body: serde_json::Value = assertion.json().await.expect("assertion body");
    let assertion_id = assertion_body
        .get("assertion_id")
        .and_then(serde_json::Value::as_str)
        .expect("assertion id");

    let source = client
        .post(format!("{base_url}/api/v1/sources"))
        .json(&serde_json::json!({
            "title": "Parish Register",
            "author": "St. Mary Parish",
            "publication_info": "Archive volume",
            "abbreviation": "PR-01",
            "repository_refs": []
        }))
        .send()
        .await
        .expect("create source");
    assert_eq!(source.status(), StatusCode::CREATED);
    let source_body: serde_json::Value = source.json().await.expect("source body");
    let source_id = source_body
        .get("id")
        .and_then(serde_json::Value::as_str)
        .expect("source id");

    let citation = client
        .post(format!("{base_url}/api/v1/citations"))
        .json(&serde_json::json!({
            "source_id": source_id,
            "assertion_id": assertion_id,
            "citation_note": "occupation evidence",
            "page": "42"
        }))
        .send()
        .await
        .expect("create citation");
    assert_eq!(citation.status(), StatusCode::CREATED);
    let citation_body: serde_json::Value = citation.json().await.expect("citation body");
    let citation_id = citation_body
        .get("id")
        .and_then(serde_json::Value::as_str)
        .expect("citation id");

    let assertions = client
        .get(format!("{base_url}/api/v1/persons/{person_id}/assertions"))
        .send()
        .await
        .expect("list assertions");
    assert_eq!(assertions.status(), StatusCode::OK);
    let assertions_body: serde_json::Value = assertions.json().await.expect("assertions body");

    let occupation_assertions = assertions_body
        .get("occupation")
        .and_then(serde_json::Value::as_array)
        .expect("occupation assertions array");
    assert_eq!(occupation_assertions.len(), 1);

    let sources = occupation_assertions[0]
        .get("sources")
        .and_then(serde_json::Value::as_array)
        .expect("sources array");
    assert_eq!(sources.len(), 1);
    assert_eq!(
        sources[0]
            .get("citation_id")
            .and_then(serde_json::Value::as_str),
        Some(citation_id)
    );

    server.shutdown().await.expect("shutdown server");
}

#[tokio::test]
async fn repository_linked_to_source_round_trip() {
    let backend = in_memory_backend();
    let state = AppState::with_default_cors_sqlite(backend, 0).expect("build app state");
    let server = start_server(state, 0).await.expect("start server");

    let client = reqwest::Client::new();
    let base_url = format!("http://{}", server.local_addr);

    let repository = client
        .post(format!("{base_url}/api/v1/repositories"))
        .json(&serde_json::json!({
            "name": "National Archives",
            "repository_type": "archive",
            "address": "Kew",
            "urls": ["https://example.test/archive"]
        }))
        .send()
        .await
        .expect("create repository");
    assert_eq!(repository.status(), StatusCode::CREATED);
    let repository_body: serde_json::Value = repository.json().await.expect("repository body");
    let repository_id = repository_body
        .get("id")
        .and_then(serde_json::Value::as_str)
        .expect("repository id");

    let source = client
        .post(format!("{base_url}/api/v1/sources"))
        .json(&serde_json::json!({
            "title": "1881 Census",
            "author": "Registrar General",
            "repository_refs": [{
                "repository_id": repository_id,
                "call_number": "RG11",
                "media_type": "microfilm"
            }]
        }))
        .send()
        .await
        .expect("create source");
    assert_eq!(source.status(), StatusCode::CREATED);
    let source_body: serde_json::Value = source.json().await.expect("source body");
    let source_id = source_body
        .get("id")
        .and_then(serde_json::Value::as_str)
        .expect("source id");

    let source_detail = client
        .get(format!("{base_url}/api/v1/sources/{source_id}"))
        .send()
        .await
        .expect("source detail");
    assert_eq!(source_detail.status(), StatusCode::OK);
    let source_detail_body: serde_json::Value =
        source_detail.json().await.expect("source detail body");

    let repository_refs = source_detail_body
        .get("repository_refs")
        .and_then(serde_json::Value::as_array)
        .expect("repository refs");
    assert_eq!(repository_refs.len(), 1);
    assert_eq!(
        repository_refs[0]
            .get("repository_id")
            .and_then(serde_json::Value::as_str),
        Some(repository_id)
    );

    server.shutdown().await.expect("shutdown server");
}

#[tokio::test]
async fn note_create_link_and_filter_sanitizes_html() {
    let backend = in_memory_backend();
    let state = AppState::with_default_cors_sqlite(backend, 0).expect("build app state");
    let server = start_server(state, 0).await.expect("start server");

    let client = reqwest::Client::new();
    let base_url = format!("http://{}", server.local_addr);

    let person = client
        .post(format!("{base_url}/api/v1/persons"))
        .json(&serde_json::json!({
            "given_names": ["Mary"],
            "surnames": [{"value": "Johnson", "origin_type": "patrilineal", "connector": null}],
            "gender": "female"
        }))
        .send()
        .await
        .expect("create person");
    assert_eq!(person.status(), StatusCode::CREATED);
    let person_body: serde_json::Value = person.json().await.expect("person body");
    let person_id = person_body
        .get("id")
        .and_then(serde_json::Value::as_str)
        .expect("person id");

    let note = client
        .post(format!("{base_url}/api/v1/notes"))
        .json(&serde_json::json!({
            "text": "<script>alert('x')</script><b>Research note</b>",
            "note_type": "research",
            "linked_entity_id": person_id,
            "linked_entity_type": "person"
        }))
        .send()
        .await
        .expect("create note");
    assert_eq!(note.status(), StatusCode::CREATED);
    let note_body: serde_json::Value = note.json().await.expect("note body");
    let note_id = note_body
        .get("id")
        .and_then(serde_json::Value::as_str)
        .expect("note id");

    let note_detail = client
        .get(format!("{base_url}/api/v1/notes/{note_id}"))
        .send()
        .await
        .expect("get note");
    assert_eq!(note_detail.status(), StatusCode::OK);
    let note_detail_body: serde_json::Value = note_detail.json().await.expect("note detail body");

    let note_text = note_detail_body
        .get("text")
        .and_then(serde_json::Value::as_str)
        .expect("note text");
    assert!(!note_text.contains("<script>"));
    assert!(note_text.contains("Research note"));

    let filtered_notes = client
        .get(format!("{base_url}/api/v1/notes?entity_id={person_id}"))
        .send()
        .await
        .expect("list notes for person");
    assert_eq!(filtered_notes.status(), StatusCode::OK);
    let filtered_body: serde_json::Value =
        filtered_notes.json().await.expect("filtered notes body");
    let filtered_array = filtered_body.as_array().expect("notes array");
    assert_eq!(filtered_array.len(), 1);
    assert_eq!(
        filtered_array[0]
            .get("id")
            .and_then(serde_json::Value::as_str),
        Some(note_id)
    );

    server.shutdown().await.expect("shutdown server");
}

#[tokio::test]
async fn kennedy_import_exposes_notes_endpoint_data() {
    let backend = in_memory_backend();
    let state = AppState::with_default_cors_sqlite(backend, 0).expect("build app state");
    let server = start_server(state, 0).await.expect("start server");

    let client = reqwest::Client::new();
    let base_url = format!("http://{}", server.local_addr);

    let content = include_str!("../../../testdata/gedcom/kennedy.ged");
    let part = reqwest::multipart::Part::text(content.to_string()).file_name("kennedy.ged");
    let form = reqwest::multipart::Form::new()
        .text("format", "gedcom")
        .part("file", part);

    let accepted = client
        .post(format!("{base_url}/api/v1/import"))
        .multipart(form)
        .send()
        .await
        .expect("submit import");
    assert_eq!(accepted.status(), StatusCode::ACCEPTED);
    let accepted_body: ImportAcceptedResponse = accepted.json().await.expect("accepted body");

    wait_for_import_completion(&client, &base_url, &accepted_body.job_id).await;

    let notes = client
        .get(format!("{base_url}/api/v1/notes?limit=1000"))
        .send()
        .await
        .expect("list notes");
    assert_eq!(notes.status(), StatusCode::OK);
    let notes_body: serde_json::Value = notes.json().await.expect("notes body");
    let notes_array = notes_body.as_array().expect("notes array");
    assert!(
        !notes_array.is_empty(),
        "kennedy import should produce at least one typed note"
    );

    server.shutdown().await.expect("shutdown server");
}

#[tokio::test]
async fn standalone_gedcom_note_is_not_silently_dropped() {
    let backend = in_memory_backend();
    let state = AppState::with_default_cors_sqlite(backend, 0).expect("build app state");
    let server = start_server(state, 0).await.expect("start server");

    let client = reqwest::Client::new();
    let base_url = format!("http://{}", server.local_addr);

    let gedcom = "0 HEAD\n1 SOUR TEST\n1 CHAR UTF-8\n0 @N1@ NOTE Standalone note text\n0 TRLR\n";
    let part = reqwest::multipart::Part::text(gedcom.to_string()).file_name("standalone_note.ged");
    let form = reqwest::multipart::Form::new()
        .text("format", "gedcom")
        .part("file", part);

    let accepted = client
        .post(format!("{base_url}/api/v1/import"))
        .multipart(form)
        .send()
        .await
        .expect("submit import");
    assert_eq!(accepted.status(), StatusCode::ACCEPTED);
    let accepted_body: ImportAcceptedResponse = accepted.json().await.expect("accepted body");

    wait_for_import_completion(&client, &base_url, &accepted_body.job_id).await;

    let notes = client
        .get(format!("{base_url}/api/v1/notes"))
        .send()
        .await
        .expect("list notes");
    assert_eq!(notes.status(), StatusCode::OK);

    let notes_body: serde_json::Value = notes.json().await.expect("notes body");
    let notes_array = notes_body.as_array().expect("notes array");
    assert!(
        notes_array.iter().any(|note| {
            note.get("text").and_then(serde_json::Value::as_str) == Some("Standalone note text")
        }),
        "expected imported standalone NOTE text in typed notes endpoint"
    );

    server.shutdown().await.expect("shutdown server");
}
