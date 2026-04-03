use std::sync::Arc;

use reqwest::StatusCode;
use rusqlite::Connection;
use rustygene_api::{start_server, AppState};
use rustygene_core::assertion::{AssertionStatus, EvidenceType};
use rustygene_core::event::{Event, EventParticipant, EventRole, EventType};
use rustygene_core::person::Person;
use rustygene_core::types::{ActorRef, Calendar, DateValue, EntityId, FuzzyDate, Gender};
use rustygene_storage::run_migrations;
use rustygene_storage::sqlite_impl::SqliteBackend;
use rustygene_storage::{EntityType, JsonAssertion, Storage};

fn in_memory_backend() -> Arc<SqliteBackend> {
    let mut conn = Connection::open_in_memory().expect("open in-memory sqlite connection");
    run_migrations(&mut conn).expect("run sqlite migrations");
    Arc::new(SqliteBackend::new(conn))
}

fn make_assertion(value: serde_json::Value) -> JsonAssertion {
    JsonAssertion {
        id: EntityId::new(),
        value,
        confidence: 0.9,
        status: AssertionStatus::Confirmed,
        evidence_type: EvidenceType::Direct,
        source_citations: Vec::new(),
        proposed_by: ActorRef::User("test".to_string()),
        created_at: chrono::Utc::now(),
        reviewed_at: None,
        reviewed_by: None,
    }
}

#[tokio::test]
async fn person_crud_round_trip_returns_full_name_assertion_shape() {
    let backend = in_memory_backend();
    let state = AppState::with_default_cors(backend, 0).expect("build app state");
    let server = start_server(state, 0).await.expect("start server");

    let client = reqwest::Client::new();
    let create_body = serde_json::json!({
        "given_names": ["John", "Quincy"],
        "surnames": [
            {"value": "Adams", "origin_type": "patrilineal", "connector": null}
        ],
        "name_type": "birth",
        "call_name": "John",
        "sort_as": "Adams, John Quincy",
        "gender": "male"
    });

    let create_response = client
        .post(format!("http://{}/api/v1/persons", server.local_addr))
        .json(&create_body)
        .send()
        .await
        .expect("create person");
    assert_eq!(create_response.status(), StatusCode::CREATED);

    let created: serde_json::Value = create_response.json().await.expect("parse create body");
    let person_id = created
        .get("id")
        .and_then(serde_json::Value::as_str)
        .expect("person id as string");

    let detail_response = client
        .get(format!(
            "http://{}/api/v1/persons/{}",
            server.local_addr, person_id
        ))
        .send()
        .await
        .expect("get person detail");
    assert_eq!(detail_response.status(), StatusCode::OK);

    let detail: serde_json::Value = detail_response.json().await.expect("parse detail body");
    let names = detail
        .get("names")
        .and_then(serde_json::Value::as_array)
        .expect("names array");
    assert_eq!(names.len(), 1);
    let first_name = &names[0];
    assert_eq!(
        first_name
            .get("given_names")
            .and_then(serde_json::Value::as_array)
            .expect("given names array")
            .len(),
        2
    );
    assert_eq!(
        first_name
            .get("name_type")
            .and_then(serde_json::Value::as_str),
        Some("birth")
    );
    assert_eq!(
        first_name
            .get("call_name")
            .and_then(serde_json::Value::as_str),
        Some("John")
    );
    assert_eq!(
        first_name
            .get("sort_as")
            .and_then(serde_json::Value::as_str),
        Some("Adams, John Quincy")
    );

    let update_body = serde_json::json!({
        "given_names": ["Johnny"],
        "surnames": [
            {"value": "Adams", "origin_type": "patrilineal", "connector": null}
        ],
        "name_type": "aka"
    });

    let update_response = client
        .put(format!(
            "http://{}/api/v1/persons/{}",
            server.local_addr, person_id
        ))
        .json(&update_body)
        .send()
        .await
        .expect("update person");
    assert_eq!(update_response.status(), StatusCode::OK);

    let assertions_response = client
        .get(format!(
            "http://{}/api/v1/persons/{}/assertions",
            server.local_addr, person_id
        ))
        .send()
        .await
        .expect("get person assertions");
    assert_eq!(assertions_response.status(), StatusCode::OK);

    let assertions: serde_json::Value = assertions_response.json().await.expect("parse assertions");
    let grouped_names = assertions
        .get("name")
        .and_then(serde_json::Value::as_array)
        .expect("grouped name assertions");
    assert!(
        grouped_names.len() >= 2,
        "updated person should have multiple name assertions"
    );

    let delete_response = client
        .delete(format!(
            "http://{}/api/v1/persons/{}",
            server.local_addr, person_id
        ))
        .send()
        .await
        .expect("delete person");
    assert_eq!(delete_response.status(), StatusCode::NO_CONTENT);

    let missing_response = client
        .get(format!(
            "http://{}/api/v1/persons/{}",
            server.local_addr, person_id
        ))
        .send()
        .await
        .expect("get deleted person");
    assert_eq!(missing_response.status(), StatusCode::NOT_FOUND);

    server.shutdown().await.expect("shutdown server");
}

#[tokio::test]
async fn timeline_endpoint_returns_events_in_chronological_order() {
    let backend = in_memory_backend();
    let person = Person {
        id: EntityId::new(),
        names: Vec::new(),
        gender: Gender::Male,
        living: false,
        private: false,
        original_xref: None,
        _raw_gedcom: Default::default(),
    };
    backend.create_person(&person).await.expect("create person");

    let later_event = Event {
        id: EntityId::new(),
        event_type: EventType::Death,
        date: Some(DateValue::Exact {
            date: FuzzyDate::new(1900, Some(1), Some(1)),
            calendar: Calendar::Gregorian,
        }),
        place_ref: None,
        participants: vec![EventParticipant {
            person_id: person.id,
            role: EventRole::Principal,
            census_role: None,
        }],
        description: Some("Later event".to_string()),
        _raw_gedcom: Default::default(),
    };

    let earlier_event = Event {
        id: EntityId::new(),
        event_type: EventType::Birth,
        date: Some(DateValue::Exact {
            date: FuzzyDate::new(1880, Some(1), Some(1)),
            calendar: Calendar::Gregorian,
        }),
        place_ref: None,
        participants: vec![EventParticipant {
            person_id: person.id,
            role: EventRole::Principal,
            census_role: None,
        }],
        description: Some("Earlier event".to_string()),
        _raw_gedcom: Default::default(),
    };

    backend
        .create_event(&later_event)
        .await
        .expect("create later event");
    backend
        .create_event(&earlier_event)
        .await
        .expect("create earlier event");
    backend
        .create_assertion(
            person.id,
            EntityType::Person,
            "name",
            &make_assertion(serde_json::json!({
                "name_type": "birth",
                "date_range": null,
                "given_names": "Test",
                "call_name": null,
                "surnames": [{"value": "Person", "origin_type": "patrilineal", "connector": null}],
                "prefix": null,
                "suffix": null,
                "sort_as": null
            })),
        )
        .await
        .expect("create name assertion");

    let state = AppState::with_default_cors(backend, 0).expect("build app state");
    let server = start_server(state, 0).await.expect("start server");

    let client = reqwest::Client::new();
    let response = client
        .get(format!(
            "http://{}/api/v1/persons/{}/timeline",
            server.local_addr, person.id
        ))
        .send()
        .await
        .expect("get timeline");
    assert_eq!(response.status(), StatusCode::OK);

    let timeline: serde_json::Value = response.json().await.expect("parse timeline");
    let events = timeline.as_array().expect("timeline array");
    assert_eq!(events.len(), 2);
    assert_eq!(
        events[0]
            .get("description")
            .and_then(serde_json::Value::as_str),
        Some("Earlier event")
    );
    assert_eq!(
        events[1]
            .get("description")
            .and_then(serde_json::Value::as_str),
        Some("Later event")
    );

    server.shutdown().await.expect("shutdown server");
}
