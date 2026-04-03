use std::sync::Arc;
use std::time::Duration;

use reqwest::StatusCode;
use rusqlite::Connection;
use rustygene_api::{start_server, AppState};
use rustygene_storage::run_migrations;
use rustygene_storage::sqlite_impl::SqliteBackend;
use serde::Deserialize;
use tokio::time::timeout;

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

fn create_person_payload(given: &str, surname: &str) -> serde_json::Value {
    serde_json::json!({
        "given_names": [given],
        "surnames": [{ "value": surname, "origin_type": "patrilineal", "connector": null }],
        "name_type": "birth",
        "gender": "unknown"
    })
}

async fn read_sse_frame(response: &mut reqwest::Response, wait: Duration) -> String {
    let mut buffer = String::new();

    loop {
        let next_chunk = timeout(wait, response.chunk())
            .await
            .expect("timed out waiting for SSE chunk")
            .expect("read SSE chunk failed");

        let Some(chunk) = next_chunk else {
            panic!("SSE stream closed unexpectedly");
        };

        let chunk_text = String::from_utf8(chunk.to_vec()).expect("SSE chunk should be utf8");
        buffer.push_str(&chunk_text);

        if let Some(frame) = extract_first_sse_frame(&mut buffer) {
            return frame;
        }
    }
}

fn extract_first_sse_frame(buffer: &mut String) -> Option<String> {
    for delimiter in ["\r\n\r\n", "\n\n"] {
        if let Some(frame_end) = buffer.find(delimiter) {
            let frame = buffer[..frame_end].replace("\r\n", "\n");
            let drain_end = frame_end + delimiter.len();
            buffer.drain(..drain_end);
            if frame.is_empty() {
                continue;
            }
            return Some(frame);
        }
    }

    None
}

#[tokio::test]
async fn sse_emits_entity_created_for_person_post() {
    let backend = in_memory_backend();
    let state = AppState::with_default_cors_sqlite(backend, 0).expect("build app state");
    let server = start_server(state, 0).await.expect("start server");

    let client = reqwest::Client::new();
    let mut stream_response = client
        .get(format!(
            "http://{}/api/v1/events/stream?types=entity.created",
            server.local_addr
        ))
        .send()
        .await
        .expect("open SSE stream");

    assert_eq!(stream_response.status(), StatusCode::OK);
    assert!(stream_response
        .headers()
        .get("content-type")
        .and_then(|h| h.to_str().ok())
        .unwrap_or_default()
        .starts_with("text/event-stream"));

    let create_response = client
        .post(format!("http://{}/api/v1/persons", server.local_addr))
        .json(&create_person_payload("John", "SSE"))
        .send()
        .await
        .expect("create person");
    assert_eq!(create_response.status(), StatusCode::CREATED);

    let created_body: serde_json::Value = create_response.json().await.expect("parse create body");
    let person_id = created_body
        .get("id")
        .and_then(serde_json::Value::as_str)
        .expect("person id");

    let frame = read_sse_frame(&mut stream_response, Duration::from_secs(5)).await;
    assert!(
        frame.contains("event: entity.created"),
        "unexpected frame: {frame}"
    );
    assert!(
        frame.contains(person_id),
        "frame missing person id: {frame}"
    );

    drop(stream_response);
    server.shutdown().await.expect("shutdown server");
}

#[tokio::test]
async fn sse_types_filter_excludes_entity_updated_when_subscribed_to_created_only() {
    let backend = in_memory_backend();
    let state = AppState::with_default_cors_sqlite(backend, 0).expect("build app state");
    let server = start_server(state, 0).await.expect("start server");

    let client = reqwest::Client::new();
    let mut stream_response = client
        .get(format!(
            "http://{}/api/v1/events/stream?types=entity.created",
            server.local_addr
        ))
        .send()
        .await
        .expect("open SSE stream");

    let create_response = client
        .post(format!("http://{}/api/v1/persons", server.local_addr))
        .json(&create_person_payload("Alice", "Filter"))
        .send()
        .await
        .expect("create person");
    assert_eq!(create_response.status(), StatusCode::CREATED);
    let created_body: serde_json::Value = create_response.json().await.expect("parse create body");
    let person_id = created_body
        .get("id")
        .and_then(serde_json::Value::as_str)
        .expect("person id")
        .to_string();

    let first_frame = read_sse_frame(&mut stream_response, Duration::from_secs(5)).await;
    assert!(first_frame.contains("event: entity.created"));
    assert!(first_frame.contains(&person_id));

    let update_response = client
        .put(format!(
            "http://{}/api/v1/persons/{}",
            server.local_addr, person_id
        ))
        .json(&create_person_payload("Alice", "Updated"))
        .send()
        .await
        .expect("update person");
    assert_eq!(update_response.status(), StatusCode::OK);

    let maybe_chunk = timeout(Duration::from_secs(1), stream_response.chunk()).await;
    if let Ok(Ok(Some(chunk))) = maybe_chunk {
        let text = String::from_utf8(chunk.to_vec()).expect("SSE chunk should be utf8");
        assert!(
            !text.contains("entity.updated"),
            "entity.updated leaked through filter: {text}"
        );
    }

    drop(stream_response);
    server.shutdown().await.expect("shutdown server");
}

#[tokio::test]
async fn sse_emits_import_completed_with_counts() {
    let backend = in_memory_backend();
    let state = AppState::with_default_cors_sqlite(backend, 0).expect("build app state");
    let server = start_server(state, 0).await.expect("start server");

    let client = reqwest::Client::new();
    let mut stream_response = client
        .get(format!(
            "http://{}/api/v1/events/stream?types=import.completed",
            server.local_addr
        ))
        .send()
        .await
        .expect("open SSE stream");

    let content = include_str!("../../../testdata/gedcom/kennedy.ged");
    let part = reqwest::multipart::Part::text(content.to_string()).file_name("kennedy.ged");
    let form = reqwest::multipart::Form::new()
        .text("format", "gedcom")
        .part("file", part);

    let accepted = client
        .post(format!("http://{}/api/v1/import", server.local_addr))
        .multipart(form)
        .send()
        .await
        .expect("start import job");
    assert_eq!(accepted.status(), StatusCode::ACCEPTED);

    let accepted_body: ImportAcceptedResponse = accepted.json().await.expect("parse accepted body");
    for _ in 0..300 {
        let response = client
            .get(format!(
                "http://{}/api/v1/import/{}",
                server.local_addr, accepted_body.job_id
            ))
            .send()
            .await
            .expect("poll import status");
        let status: ImportJobStatusResponse = response.json().await.expect("parse status body");
        if status.status == "completed" || status.status == "failed" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let frame = read_sse_frame(&mut stream_response, Duration::from_secs(10)).await;
    assert!(
        frame.contains("event: import.completed"),
        "unexpected frame: {frame}"
    );
    assert!(
        frame.contains("entities_imported"),
        "frame missing counts: {frame}"
    );

    drop(stream_response);
    server.shutdown().await.expect("shutdown server");
}

#[tokio::test]
async fn sse_keepalive_comment_arrives_when_idle() {
    let backend = in_memory_backend();
    let state = AppState::with_default_cors_sqlite(backend, 0).expect("build app state");
    let server = start_server(state, 0).await.expect("start server");

    let client = reqwest::Client::new();
    let mut stream_response = client
        .get(format!(
            "http://{}/api/v1/events/stream?types=entity.deleted",
            server.local_addr
        ))
        .send()
        .await
        .expect("open SSE stream");

    let frame = read_sse_frame(&mut stream_response, Duration::from_secs(35)).await;
    assert!(
        frame.starts_with(':'),
        "expected keepalive comment frame: {frame}"
    );

    drop(stream_response);
    server.shutdown().await.expect("shutdown server");
}
