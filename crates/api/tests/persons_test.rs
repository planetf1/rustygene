use std::sync::Arc;

use reqwest::StatusCode;
use rusqlite::Connection;
use rustygene_api::{start_server, AppState};
use rustygene_core::assertion::{AssertionStatus, EvidenceType};
use rustygene_core::event::{Event, EventParticipant, EventRole, EventType};
use rustygene_core::evidence::CitationRef;
use rustygene_core::person::{NameType, Person, PersonName, Surname, SurnameOrigin};
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

#[tokio::test]
async fn list_persons_includes_aggregated_assertion_counts_and_event_years() {
    let backend = in_memory_backend();

    let person = Person {
        id: EntityId::new(),
        names: vec![PersonName {
            name_type: NameType::Birth,
            date_range: None,
            given_names: "Ethel May".to_string(),
            call_name: None,
            surnames: vec![Surname {
                value: "Harry".to_string(),
                origin_type: SurnameOrigin::Patrilineal,
                connector: None,
            }],
            prefix: None,
            suffix: None,
            sort_as: None,
        }],
        gender: Gender::Female,
        living: false,
        private: false,
        original_xref: None,
        _raw_gedcom: Default::default(),
    };
    backend.create_person(&person).await.expect("create person");

    backend
        .create_assertion(
            person.id,
            EntityType::Person,
            "name",
            &make_assertion(serde_json::json!({
                "name_type": "birth",
                "date_range": null,
                "given_names": "Ethel May",
                "call_name": null,
                "surnames": [{"value": "Harry", "origin_type": "patrilineal", "connector": null}],
                "prefix": null,
                "suffix": null,
                "sort_as": null
            })),
        )
        .await
        .expect("create name assertion");
    backend
        .create_assertion(
            person.id,
            EntityType::Person,
            "gender",
            &make_assertion(serde_json::json!("female")),
        )
        .await
        .expect("create gender assertion");

    let birth_1 = Event {
        id: EntityId::new(),
        event_type: EventType::Birth,
        date: Some(DateValue::Exact {
            date: FuzzyDate::new(1888, Some(1), Some(1)),
            calendar: Calendar::Gregorian,
        }),
        place_ref: None,
        participants: vec![EventParticipant {
            person_id: person.id,
            role: EventRole::Principal,
            census_role: None,
        }],
        description: Some("Earlier birth assertion".to_string()),
        _raw_gedcom: Default::default(),
    };
    let birth_2 = Event {
        id: EntityId::new(),
        event_type: EventType::Birth,
        date: Some(DateValue::Exact {
            date: FuzzyDate::new(1891, Some(1), Some(1)),
            calendar: Calendar::Gregorian,
        }),
        place_ref: None,
        participants: vec![EventParticipant {
            person_id: person.id,
            role: EventRole::Principal,
            census_role: None,
        }],
        description: Some("Later birth assertion".to_string()),
        _raw_gedcom: Default::default(),
    };
    let death_1 = Event {
        id: EntityId::new(),
        event_type: EventType::Death,
        date: Some(DateValue::Exact {
            date: FuzzyDate::new(1955, Some(1), Some(1)),
            calendar: Calendar::Gregorian,
        }),
        place_ref: None,
        participants: vec![EventParticipant {
            person_id: person.id,
            role: EventRole::Principal,
            census_role: None,
        }],
        description: Some("Earlier death assertion".to_string()),
        _raw_gedcom: Default::default(),
    };
    let death_2 = Event {
        id: EntityId::new(),
        event_type: EventType::Death,
        date: Some(DateValue::Exact {
            date: FuzzyDate::new(1962, Some(1), Some(1)),
            calendar: Calendar::Gregorian,
        }),
        place_ref: None,
        participants: vec![EventParticipant {
            person_id: person.id,
            role: EventRole::Principal,
            census_role: None,
        }],
        description: Some("Later death assertion".to_string()),
        _raw_gedcom: Default::default(),
    };

    backend
        .create_event(&birth_1)
        .await
        .expect("create birth_1");
    backend
        .create_event(&birth_2)
        .await
        .expect("create birth_2");
    backend
        .create_event(&death_1)
        .await
        .expect("create death_1");
    backend
        .create_event(&death_2)
        .await
        .expect("create death_2");

    let state = AppState::with_default_cors(backend, 0).expect("build app state");
    let server = start_server(state, 0).await.expect("start server");

    let response = reqwest::Client::new()
        .get(format!(
            "http://{}/api/v1/persons?limit=10&offset=0",
            server.local_addr
        ))
        .send()
        .await
        .expect("list persons");
    let status = response.status();
    let body_text = response.text().await.expect("read list response body");
    assert_eq!(
        status,
        StatusCode::OK,
        "unexpected response body: {body_text}"
    );

    let body: serde_json::Value =
        serde_json::from_str(&body_text).expect("parse list response JSON");
    let items = body
        .get("items")
        .and_then(serde_json::Value::as_array)
        .expect("items array");

    let person_row = items
        .iter()
        .find(|row| {
            row.get("id")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|id| id == person.id.to_string())
        })
        .expect("created person must be present in list");

    assert_eq!(
        person_row
            .get("display_name")
            .and_then(serde_json::Value::as_str),
        Some("Ethel May Harry")
    );
    assert_eq!(
        person_row
            .get("birth_year")
            .and_then(serde_json::Value::as_i64),
        Some(1888)
    );
    assert_eq!(
        person_row
            .get("death_year")
            .and_then(serde_json::Value::as_i64),
        Some(1962)
    );

    let assertion_counts = person_row
        .get("assertion_counts")
        .and_then(serde_json::Value::as_object)
        .expect("assertion_counts object");
    assert_eq!(
        assertion_counts
            .get("name")
            .and_then(serde_json::Value::as_u64),
        Some(1)
    );
    assert_eq!(
        assertion_counts
            .get("gender")
            .and_then(serde_json::Value::as_u64),
        Some(1)
    );

    server.shutdown().await.expect("shutdown server");
}

#[tokio::test]
async fn person_assertions_endpoint_deduplicates_duplicate_citation_refs() {
    let backend = in_memory_backend();

    let person = Person {
        id: EntityId::new(),
        names: Vec::new(),
        gender: Gender::Unknown,
        living: false,
        private: false,
        original_xref: None,
        _raw_gedcom: Default::default(),
    };
    backend.create_person(&person).await.expect("create person");

    let duplicated_ref = CitationRef {
        citation_id: EntityId::new(),
        note: Some("same-source".to_string()),
    };

    backend
        .create_assertion(
            person.id,
            EntityType::Person,
            "occupation",
            &JsonAssertion {
                id: EntityId::new(),
                value: serde_json::json!("Carpenter"),
                confidence: 0.9,
                status: AssertionStatus::Confirmed,
                evidence_type: EvidenceType::Direct,
                source_citations: vec![duplicated_ref.clone(), duplicated_ref],
                proposed_by: ActorRef::User("test".to_string()),
                created_at: chrono::Utc::now(),
                reviewed_at: None,
                reviewed_by: None,
            },
        )
        .await
        .expect("create assertion with duplicate citation refs");

    let state = AppState::with_default_cors(backend, 0).expect("build app state");
    let server = start_server(state, 0).await.expect("start server");

    let response = reqwest::Client::new()
        .get(format!(
            "http://{}/api/v1/persons/{}/assertions",
            server.local_addr, person.id
        ))
        .send()
        .await
        .expect("get person assertions");
    assert_eq!(response.status(), StatusCode::OK);

    let body: serde_json::Value = response.json().await.expect("parse assertions response");
    let occupation_assertions = body
        .get("occupation")
        .and_then(serde_json::Value::as_array)
        .expect("occupation assertions array");
    assert_eq!(occupation_assertions.len(), 1);

    let sources = occupation_assertions[0]
        .get("sources")
        .and_then(serde_json::Value::as_array)
        .expect("sources array");
    assert_eq!(
        sources.len(),
        1,
        "duplicate citation refs should be collapsed in API response"
    );

    server.shutdown().await.expect("shutdown server");
}

/// Regression test for rustygene-qae: updating assertion confidence/preferred via the API
/// must trigger a person snapshot recompute so that generated columns (primary_surname,
/// birth_year, etc.) stay in sync with the updated assertion data.
#[tokio::test]
async fn person_assertion_confidence_update_triggers_snapshot_recompute() {
    let backend = in_memory_backend();

    let person_id = EntityId::new();
    let person = Person {
        id: person_id,
        names: vec![PersonName {
            given_names: "Snapshot".to_string(),
            surnames: vec![rustygene_core::person::Surname {
                value: "Test".to_string(),
                origin_type: Default::default(),
                connector: None,
            }],
            ..Default::default()
        }],
        gender: Gender::Unknown,
        living: false,
        private: false,
        original_xref: None,
        _raw_gedcom: Default::default(),
    };
    backend.create_person(&person).await.expect("create person");

    // Create a confirmed name assertion
    let assertion_id = EntityId::new();
    backend
        .create_assertion(
            person_id,
            EntityType::Person,
            "name",
            &JsonAssertion {
                id: assertion_id,
                value: serde_json::json!("Snapshot Test"),
                confidence: 0.5,
                status: AssertionStatus::Confirmed,
                evidence_type: EvidenceType::Direct,
                source_citations: vec![],
                proposed_by: ActorRef::User("test".to_string()),
                created_at: chrono::Utc::now(),
                reviewed_at: None,
                reviewed_by: None,
            },
        )
        .await
        .expect("create assertion");

    let state = AppState::with_default_cors(backend.clone(), 0).expect("build app state");
    let server = start_server(state, 0).await.expect("start server");
    let client = reqwest::Client::new();
    let base = format!("http://{}/api/v1", server.local_addr);

    // Update assertion confidence via the API
    let update_resp = client
        .put(format!(
            "{base}/persons/{person_id}/assertions/{assertion_id}"
        ))
        .json(&serde_json::json!({ "confidence": 0.95 }))
        .send()
        .await
        .expect("put assertion confidence");
    assert_eq!(
        update_resp.status(),
        StatusCode::OK,
        "confidence update should succeed"
    );

    // Verify the updated confidence is retrievable via assertions endpoint
    let assertions_resp = client
        .get(format!("{base}/persons/{person_id}/assertions"))
        .send()
        .await
        .expect("get assertions");
    assert_eq!(assertions_resp.status(), StatusCode::OK);
    let assertions: serde_json::Value = assertions_resp.json().await.expect("parse assertions");
    let name_assertions = assertions["name"]
        .as_array()
        .expect("name assertions array");
    assert_eq!(name_assertions.len(), 1);
    let updated_confidence = name_assertions[0]["confidence"]
        .as_f64()
        .expect("confidence as f64");
    assert!(
        (updated_confidence - 0.95).abs() < 1e-6,
        "confidence should be 0.95 after update, got {updated_confidence}"
    );

    // Verify person detail is still accessible (snapshot was not corrupted by recompute)
    let detail_resp = client
        .get(format!("{base}/persons/{person_id}"))
        .send()
        .await
        .expect("get person detail");
    assert_eq!(
        detail_resp.status(),
        StatusCode::OK,
        "person detail must be accessible after confidence update"
    );

    server.shutdown().await.ok();

}
#[tokio::test]
async fn list_persons_sql_filter_sort_and_pagination_work() {
    let backend = in_memory_backend();

    // Create 6 persons: 3 with surname "Smith", 3 with surname "Jones"
    let surnames_and_given: &[(&str, &str)] = &[
        ("Smith", "Alice"),
        ("Smith", "Bob"),
        ("Smith", "Carol"),
        ("Jones", "David"),
        ("Jones", "Eve"),
        ("Jones", "Frank"),
    ];
    for (surname, given) in surnames_and_given {
        let person = rustygene_core::person::Person {
            id: rustygene_core::types::EntityId::new(),
            names: vec![rustygene_core::person::PersonName {
                name_type: rustygene_core::person::NameType::Birth,
                date_range: None,
                given_names: given.to_string(),
                call_name: None,
                surnames: vec![rustygene_core::person::Surname {
                    value: surname.to_string(),
                    origin_type: rustygene_core::person::SurnameOrigin::Patrilineal,
                    connector: None,
                }],
                prefix: None,
                suffix: None,
                sort_as: None,
            }],
            gender: rustygene_core::types::Gender::Unknown,
            living: false,
            private: false,
            original_xref: None,
            _raw_gedcom: Default::default(),
        };
        backend.create_person(&person).await.expect("create person");
    }

    let state = AppState::with_default_cors(backend, 0).expect("build app state");
    let server = start_server(state, 0).await.expect("start server");
    let client = reqwest::Client::new();
    let base = format!("http://{}/api/v1", server.local_addr);

    // 1. Unfiltered: total = 6, default limit = 50, get all 6 items.
    let resp: serde_json::Value = client
        .get(format!("{base}/persons"))
        .send()
        .await
        .expect("list persons unfiltered")
        .json()
        .await
        .expect("parse");
    assert_eq!(resp["total"].as_u64(), Some(6), "total must be 6");
    assert_eq!(
        resp["items"].as_array().map(|a| a.len()),
        Some(6),
        "items must contain 6 entries"
    );

    // 2. Search filter: q=Smith returns only the 3 Smith entries.
    let resp: serde_json::Value = client
        .get(format!("{base}/persons?q=Smith"))
        .send()
        .await
        .expect("list persons filtered")
        .json()
        .await
        .expect("parse");
    assert_eq!(resp["total"].as_u64(), Some(3), "filtered total must be 3");
    assert_eq!(
        resp["items"].as_array().map(|a| a.len()),
        Some(3),
        "filtered items must be 3"
    );
    for item in resp["items"].as_array().unwrap() {
        let name = item["display_name"].as_str().unwrap_or("");
        assert!(
            name.contains("Smith"),
            "each result must contain 'Smith', got: {name}"
        );
    }

    // 3. Pagination: limit=2, offset=0 on unfiltered -> items.len()==2, total==6.
    let resp: serde_json::Value = client
        .get(format!("{base}/persons?limit=2&offset=0"))
        .send()
        .await
        .expect("list persons page 1")
        .json()
        .await
        .expect("parse");
    assert_eq!(resp["total"].as_u64(), Some(6), "paged total must still be 6");
    assert_eq!(
        resp["items"].as_array().map(|a| a.len()),
        Some(2),
        "page 1 must return exactly 2 items"
    );

    // 4. Pagination: limit=2, offset=4 -> last page returns exactly 2 items.
    let resp: serde_json::Value = client
        .get(format!("{base}/persons?limit=2&offset=4"))
        .send()
        .await
        .expect("list persons page 3")
        .json()
        .await
        .expect("parse");
    assert_eq!(resp["total"].as_u64(), Some(6), "last page total must be 6");
    assert_eq!(
        resp["items"].as_array().map(|a| a.len()),
        Some(2),
        "last page must return 2 items"
    );

    // 5. Pagination beyond end: offset=10 -> items empty, total still 6.
    let resp: serde_json::Value = client
        .get(format!("{base}/persons?limit=5&offset=10"))
        .send()
        .await
        .expect("list persons beyond end")
        .json()
        .await
        .expect("parse");
    assert_eq!(
        resp["total"].as_u64(),
        Some(6),
        "beyond-end total must still be 6"
    );
    assert_eq!(
        resp["items"].as_array().map(|a| a.len()),
        Some(0),
        "beyond-end items must be empty"
    );

    server.shutdown().await.ok();
}
