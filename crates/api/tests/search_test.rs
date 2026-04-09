use std::sync::Arc;
use std::time::Duration;

mod common;


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

        let status: ImportJobStatusResponse = response.json().await.expect("parse import status");
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

async fn start_server_with_kennedy_data() -> (rustygene_api::ServerHandle, String, reqwest::Client)
{
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

    let accepted_body: ImportAcceptedResponse = accepted.json().await.expect("accepted response");
    wait_for_import_completion(&client, &base_url, &accepted_body.job_id).await;

    (server, base_url, client)
}

#[tokio::test]
async fn search_kennedy_returns_many_person_results() {
    let (server, base_url, client) = start_server_with_kennedy_data().await;

    let response = client
        .get(format!("{base_url}/api/v1/search?q=Kennedy"))
        .send()
        .await
        .expect("search request");
    assert_eq!(response.status(), StatusCode::OK);

    let body: serde_json::Value = response.json().await.expect("parse search response");
    let results = body
        .get("results")
        .and_then(serde_json::Value::as_array)
        .expect("results array");

    let person_count = results
        .iter()
        .filter(|result| {
            result
                .get("entity_type")
                .and_then(serde_json::Value::as_str)
                == Some("person")
        })
        .count();

    assert!(
        person_count > 5,
        "expected >5 person results for Kennedy query, found {}",
        person_count
    );

    server.shutdown().await.expect("shutdown server");
}

#[tokio::test]
async fn search_phonetic_finds_kenedy_typo_but_exact_does_not() {
    let (server, base_url, client) = start_server_with_kennedy_data().await;

    let phonetic = client
        .get(format!(
            "{base_url}/api/v1/search?q=Kenedy&strategy=phonetic&type=person"
        ))
        .send()
        .await
        .expect("phonetic search request");
    assert_eq!(phonetic.status(), StatusCode::OK);
    let phonetic_body: serde_json::Value = phonetic.json().await.expect("parse phonetic response");
    let phonetic_results = phonetic_body
        .get("results")
        .and_then(serde_json::Value::as_array)
        .expect("phonetic results array");
    assert!(
        !phonetic_results.is_empty(),
        "phonetic strategy should return Kennedy matches for typo Kenedy"
    );

    let exact = client
        .get(format!(
            "{base_url}/api/v1/search?q=Kenedy&strategy=exact&type=person"
        ))
        .send()
        .await
        .expect("exact search request");
    assert_eq!(exact.status(), StatusCode::OK);
    let exact_body: serde_json::Value = exact.json().await.expect("parse exact response");
    let exact_results = exact_body
        .get("results")
        .and_then(serde_json::Value::as_array)
        .expect("exact results array");
    assert!(
        exact_results.is_empty(),
        "exact strategy should not match typo Kenedy"
    );

    server.shutdown().await.expect("shutdown server");
}

#[tokio::test]
async fn search_type_filter_person_only_returns_person_entities() {
    let (server, base_url, client) = start_server_with_kennedy_data().await;

    let response = client
        .get(format!("{base_url}/api/v1/search?q=Kennedy&type=person"))
        .send()
        .await
        .expect("typed search request");
    assert_eq!(response.status(), StatusCode::OK);

    let body: serde_json::Value = response.json().await.expect("parse search response");
    let results = body
        .get("results")
        .and_then(serde_json::Value::as_array)
        .expect("results array");

    for result in results {
        assert_eq!(
            result
                .get("entity_type")
                .and_then(serde_json::Value::as_str),
            Some("person")
        );
    }

    server.shutdown().await.expect("shutdown server");
}

#[tokio::test]
async fn search_empty_query_returns_bad_request() {
    let backend = in_memory_backend();
    let state = AppState::with_default_cors_sqlite(backend, 0).expect("build app state");
    let server = start_server(state, 0).await.expect("start server");

    let client = reqwest::Client::new();
    let response = client
        .get(format!("http://{}/api/v1/search?q=", server.local_addr))
        .send()
        .await
        .expect("search request");

    common::assert_api_error(response, StatusCode::BAD_REQUEST, "validation").await;

    server.shutdown().await.expect("shutdown server");
}
