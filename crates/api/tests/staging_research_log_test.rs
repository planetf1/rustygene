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

async fn create_person(
    client: &reqwest::Client,
    base_url: &str,
    given: &str,
    surname: &str,
) -> String {
    let response = client
        .post(format!("{base_url}/api/v1/persons"))
        .json(&serde_json::json!({
            "given_names": [given],
            "surnames": [{"value": surname, "origin_type": "patrilineal", "connector": null}],
            "gender": "unknown"
        }))
        .send()
        .await
        .expect("create person");
    assert_eq!(response.status(), StatusCode::CREATED);
    let body: Value = response.json().await.expect("person body");
    body.get("id")
        .and_then(Value::as_str)
        .expect("person id")
        .to_string()
}

#[tokio::test]
async fn staging_submit_list_approve_makes_assertion_live() {
    let backend = in_memory_backend();
    let state = AppState::with_default_cors_sqlite(backend, 0).expect("build app state");
    let server = start_server(state, 0).await.expect("start server");
    let client = reqwest::Client::new();
    let base_url = format!("http://{}", server.local_addr);

    let person_id = create_person(&client, &base_url, "John", "Smith").await;

    let submitted = client
        .post(format!("{base_url}/api/v1/staging"))
        .json(&serde_json::json!({
            "entity_type": "person",
            "entity_id": person_id,
            "proposed_field": "nickname",
            "proposed_value": "Johnny",
            "confidence": 0.88,
            "source": "unit-test"
        }))
        .send()
        .await
        .expect("submit staging");
    assert_eq!(submitted.status(), StatusCode::CREATED);
    let submitted_body: Value = submitted.json().await.expect("submitted body");
    let proposal_id = submitted_body
        .get("id")
        .and_then(Value::as_str)
        .expect("proposal id")
        .to_string();

    let listed = client
        .get(format!("{base_url}/api/v1/staging?status=pending"))
        .send()
        .await
        .expect("list staging");
    assert_eq!(listed.status(), StatusCode::OK);
    let listed_body: Value = listed.json().await.expect("list body");
    let proposals = listed_body.as_array().expect("proposals array");
    assert!(proposals
        .iter()
        .any(|p| p.get("id").and_then(Value::as_str) == Some(proposal_id.as_str())));

    let approved = client
        .post(format!("{base_url}/api/v1/staging/{proposal_id}/approve"))
        .json(&serde_json::json!({"reviewer":"tester"}))
        .send()
        .await
        .expect("approve staging");
    assert_eq!(approved.status(), StatusCode::OK);

    let assertions = client
        .get(format!("{base_url}/api/v1/persons/{person_id}/assertions"))
        .send()
        .await
        .expect("get person assertions");
    assert_eq!(assertions.status(), StatusCode::OK);
    let assertions_body: Value = assertions.json().await.expect("assertions body");
    let nickname_assertions = assertions_body
        .get("nickname")
        .and_then(Value::as_array)
        .expect("nickname assertions");
    assert!(nickname_assertions.iter().any(|a| {
        a.get("value") == Some(&Value::String("Johnny".to_string()))
            && a.get("status")
                .and_then(Value::as_str)
                .map(|status| status.eq_ignore_ascii_case("confirmed"))
                .unwrap_or(false)
    }));

    server.shutdown().await.expect("shutdown server");
}

#[tokio::test]
async fn staging_reject_and_bulk_reject_persist_reason_and_apply_all() {
    let backend = in_memory_backend();
    let state = AppState::with_default_cors_sqlite(backend, 0).expect("build app state");
    let server = start_server(state, 0).await.expect("start server");
    let client = reqwest::Client::new();
    let base_url = format!("http://{}", server.local_addr);

    let person_id = create_person(&client, &base_url, "Jane", "Doe").await;

    let proposal_a = {
        let response = client
            .post(format!("{base_url}/api/v1/staging"))
            .json(&serde_json::json!({
                "entity_type": "person",
                "entity_id": person_id,
                "proposed_field": "alias",
                "proposed_value": "Janie",
                "source": "unit-test"
            }))
            .send()
            .await
            .expect("submit staging");
        assert_eq!(response.status(), StatusCode::CREATED);
        let body: Value = response.json().await.expect("submit body");
        body.get("id")
            .and_then(Value::as_str)
            .expect("proposal id")
            .to_string()
    };

    let proposal_b = {
        let response = client
            .post(format!("{base_url}/api/v1/staging"))
            .json(&serde_json::json!({
                "entity_type": "person",
                "entity_id": person_id,
                "proposed_field": "alias",
                "proposed_value": "J.D.",
                "source": "unit-test"
            }))
            .send()
            .await
            .expect("submit staging");
        assert_eq!(response.status(), StatusCode::CREATED);
        let body: Value = response.json().await.expect("submit body");
        body.get("id")
            .and_then(Value::as_str)
            .expect("proposal id")
            .to_string()
    };

    let proposal_c = {
        let response = client
            .post(format!("{base_url}/api/v1/staging"))
            .json(&serde_json::json!({
                "entity_type": "person",
                "entity_id": person_id,
                "proposed_field": "alias",
                "proposed_value": "Jay",
                "source": "unit-test"
            }))
            .send()
            .await
            .expect("submit staging");
        assert_eq!(response.status(), StatusCode::CREATED);
        let body: Value = response.json().await.expect("submit body");
        body.get("id")
            .and_then(Value::as_str)
            .expect("proposal id")
            .to_string()
    };

    let rejected = client
        .post(format!("{base_url}/api/v1/staging/{proposal_a}/reject"))
        .json(&serde_json::json!({"reviewer":"tester","reason":"insufficient evidence"}))
        .send()
        .await
        .expect("reject staging");
    assert_eq!(rejected.status(), StatusCode::OK);

    let detail = client
        .get(format!("{base_url}/api/v1/staging/{proposal_a}"))
        .send()
        .await
        .expect("staging detail");
    assert_eq!(detail.status(), StatusCode::OK);
    let detail_body: Value = detail.json().await.expect("detail body");
    assert_eq!(
        detail_body.get("status").and_then(Value::as_str),
        Some("rejected")
    );
    assert_eq!(
        detail_body.get("review_note").and_then(Value::as_str),
        Some("insufficient evidence")
    );
    assert!(detail_body
        .get("diff_summary")
        .and_then(Value::as_str)
        .map(|s| !s.is_empty())
        .unwrap_or(false));

    let bulk = client
        .post(format!("{base_url}/api/v1/staging/bulk"))
        .json(&serde_json::json!({
            "ids":[proposal_b.clone(), proposal_c.clone()],
            "action":"reject",
            "reviewer":"bulk-reviewer",
            "reason":"batch rejection"
        }))
        .send()
        .await
        .expect("bulk reject");
    assert_eq!(bulk.status(), StatusCode::OK);
    let bulk_body: Value = bulk.json().await.expect("bulk body");
    assert_eq!(bulk_body.get("processed").and_then(Value::as_u64), Some(2));

    for id in [proposal_b.as_str(), proposal_c.as_str()] {
        let response = client
            .get(format!("{base_url}/api/v1/staging/{id}"))
            .send()
            .await
            .expect("get bulk-rejected detail");
        assert_eq!(response.status(), StatusCode::OK);
        let body: Value = response.json().await.expect("detail body");
        assert_eq!(body.get("status").and_then(Value::as_str), Some("rejected"));
        assert_eq!(
            body.get("review_note").and_then(Value::as_str),
            Some("batch rejection")
        );
    }

    server.shutdown().await.expect("shutdown server");
}

#[tokio::test]
async fn research_log_create_get_update_and_filter_by_entity() {
    let backend = in_memory_backend();
    let state = AppState::with_default_cors_sqlite(backend, 0).expect("build app state");
    let server = start_server(state, 0).await.expect("start server");
    let client = reqwest::Client::new();
    let base_url = format!("http://{}", server.local_addr);

    let person_id = create_person(&client, &base_url, "Albert", "Newton").await;

    let created = client
        .post(format!("{base_url}/api/v1/research-log"))
        .json(&serde_json::json!({
            "title":"Trace baptism record",
            "description":"Looked at parish registries and census extracts",
            "entity_references":[{"entity_type":"person","id":person_id}],
            "status":"open"
        }))
        .send()
        .await
        .expect("create research log entry");
    assert_eq!(created.status(), StatusCode::CREATED);
    let created_body: Value = created.json().await.expect("created body");
    let entry_id = created_body
        .get("id")
        .and_then(Value::as_str)
        .expect("entry id")
        .to_string();

    let list_filtered = client
        .get(format!(
            "{base_url}/api/v1/research-log?entity_id={person_id}"
        ))
        .send()
        .await
        .expect("list research log by entity");
    assert_eq!(list_filtered.status(), StatusCode::OK);
    let list_body: Value = list_filtered.json().await.expect("list body");
    let rows = list_body.as_array().expect("list array");
    assert!(rows
        .iter()
        .any(|row| row.get("id").and_then(Value::as_str) == Some(entry_id.as_str())));

    let updated = client
        .put(format!("{base_url}/api/v1/research-log/{entry_id}"))
        .json(&serde_json::json!({
            "description":"Resolved with baptism source and citation",
            "status":"resolved"
        }))
        .send()
        .await
        .expect("update research log");
    assert_eq!(updated.status(), StatusCode::OK);

    let detail = client
        .get(format!("{base_url}/api/v1/research-log/{entry_id}"))
        .send()
        .await
        .expect("research log detail");
    assert_eq!(detail.status(), StatusCode::OK);
    let detail_body: Value = detail.json().await.expect("detail body");
    assert_eq!(
        detail_body.get("status").and_then(Value::as_str),
        Some("resolved")
    );
    assert_eq!(
        detail_body.get("description").and_then(Value::as_str),
        Some("Resolved with baptism source and citation")
    );

    let deleted = client
        .delete(format!("{base_url}/api/v1/research-log/{entry_id}"))
        .send()
        .await
        .expect("delete research log");
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);

    server.shutdown().await.expect("shutdown server");
}
