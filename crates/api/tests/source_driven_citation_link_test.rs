use std::sync::Arc;

use reqwest::StatusCode;
use rusqlite::Connection;
use rustygene_api::{start_server, AppState};
use rustygene_storage::run_migrations;
use rustygene_storage::sqlite_impl::SqliteBackend;
use serde_json::Value;

fn in_memory_backend() -> Arc<SqliteBackend> {
    let mut conn = Connection::open_in_memory().expect("open in-memory sqlite connection");
    run_migrations(&mut conn).expect("run sqlite migrations");
    Arc::new(SqliteBackend::new(conn))
}

async fn create_source(client: &reqwest::Client, base_url: &str) -> String {
    let response = client
        .post(format!("{base_url}/api/v1/sources"))
        .json(&serde_json::json!({
            "title": "Parish Register",
            "author": "Recorder",
            "publication_info": null,
            "abbreviation": null,
            "repository_refs": []
        }))
        .send()
        .await
        .expect("create source request");

    assert_eq!(response.status(), StatusCode::CREATED);
    response
        .json::<Value>()
        .await
        .expect("parse create source body")
        .get("id")
        .and_then(Value::as_str)
        .expect("source id")
        .to_string()
}

#[tokio::test]
async fn source_driven_person_assertion_carries_citation_id() {
    let backend = in_memory_backend();
    let state = AppState::with_default_cors(backend, 0).expect("build app state");
    let server = start_server(state, 0).await.expect("start server");
    let client = reqwest::Client::new();
    let base_url = format!("http://{}", server.local_addr);

    let source_id = create_source(&client, &base_url).await;

    let person_response = client
        .post(format!("{base_url}/api/v1/persons"))
        .json(&serde_json::json!({
            "given_names": ["John"],
            "surnames": [{"value": "Adams", "origin_type": "patrilineal", "connector": null}],
            "name_type": "birth",
            "gender": "unknown",
            "sort_as": null,
            "call_name": null,
            "prefix": null,
            "suffix": null,
            "birth_date": null,
            "birth_place": null
        }))
        .send()
        .await
        .expect("create person request");
    assert_eq!(person_response.status(), StatusCode::CREATED);

    let person_id = person_response
        .json::<Value>()
        .await
        .expect("parse create person body")
        .get("id")
        .and_then(Value::as_str)
        .expect("person id")
        .to_string();

    let assertions_response = client
        .get(format!("{base_url}/api/v1/persons/{person_id}/assertions"))
        .send()
        .await
        .expect("get person assertions request");
    assert_eq!(assertions_response.status(), StatusCode::OK);

    let assertion_id = assertions_response
        .json::<Value>()
        .await
        .expect("parse person assertions")
        .get("name")
        .and_then(Value::as_array)
        .and_then(|rows| rows.first())
        .and_then(|row| row.get("assertion_id"))
        .and_then(Value::as_str)
        .expect("name assertion id")
        .to_string();

    let citation_response = client
        .post(format!("{base_url}/api/v1/citations"))
        .json(&serde_json::json!({
            "source_id": source_id,
            "assertion_id": assertion_id,
            "citation_note": "line 14",
            "page": "12"
        }))
        .send()
        .await
        .expect("create citation request");
    assert_eq!(citation_response.status(), StatusCode::CREATED);

    let linked_assertions = client
        .get(format!("{base_url}/api/v1/persons/{person_id}/assertions"))
        .send()
        .await
        .expect("get linked person assertions")
        .json::<Value>()
        .await
        .expect("parse linked person assertions");

    let sources = linked_assertions
        .get("name")
        .and_then(Value::as_array)
        .and_then(|rows| rows.first())
        .and_then(|row| row.get("sources"))
        .and_then(Value::as_array)
        .expect("sources array");

    assert_eq!(
        sources.len(),
        1,
        "citation id should be linked to created person assertion"
    );

    server.shutdown().await.expect("shutdown server");
}

#[tokio::test]
async fn source_driven_event_assertion_carries_citation_id() {
    let backend = in_memory_backend();
    let state = AppState::with_default_cors(backend, 0).expect("build app state");
    let server = start_server(state, 0).await.expect("start server");
    let client = reqwest::Client::new();
    let base_url = format!("http://{}", server.local_addr);

    let source_id = create_source(&client, &base_url).await;

    let event_response = client
        .post(format!("{base_url}/api/v1/events"))
        .json(&serde_json::json!({
            "event_type": "Birth",
            "date": null,
            "place_id": null,
            "description": "Birth register entry"
        }))
        .send()
        .await
        .expect("create event request");
    assert_eq!(event_response.status(), StatusCode::CREATED);

    let event_id = event_response
        .json::<Value>()
        .await
        .expect("parse create event body")
        .get("id")
        .and_then(Value::as_str)
        .expect("event id")
        .to_string();

    let assertion_response = client
        .post(format!("{base_url}/api/v1/events/{event_id}/assertions"))
        .json(&serde_json::json!({
            "field": "description",
            "value": "Birth register entry",
            "confidence": 0.8,
            "status": "proposed",
            "source_citations": []
        }))
        .send()
        .await
        .expect("create event assertion request");
    assert_eq!(assertion_response.status(), StatusCode::CREATED);

    let assertion_id = assertion_response
        .json::<Value>()
        .await
        .expect("parse create event assertion body")
        .get("assertion_id")
        .and_then(Value::as_str)
        .expect("event assertion id")
        .to_string();

    let citation_response = client
        .post(format!("{base_url}/api/v1/citations"))
        .json(&serde_json::json!({
            "source_id": source_id,
            "assertion_id": assertion_id,
            "citation_note": "line 4",
            "page": "22"
        }))
        .send()
        .await
        .expect("create citation request");
    assert_eq!(citation_response.status(), StatusCode::CREATED);

    let linked_assertions = client
        .get(format!("{base_url}/api/v1/events/{event_id}/assertions"))
        .send()
        .await
        .expect("get event assertions")
        .json::<Value>()
        .await
        .expect("parse event assertions");

    let sources = linked_assertions
        .get("description")
        .and_then(Value::as_array)
        .and_then(|rows| rows.first())
        .and_then(|row| row.get("sources"))
        .and_then(Value::as_array)
        .expect("sources array");

    assert_eq!(
        sources.len(),
        1,
        "citation id should be linked to created event assertion"
    );

    server.shutdown().await.expect("shutdown server");
}
