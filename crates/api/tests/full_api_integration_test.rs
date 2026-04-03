mod common;

use reqwest::StatusCode;
use serde_json::Value;

use common::{spawn_test_server, spawn_test_server_with_kennedy_import};

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
async fn crud_roundtrip_for_person_family_event_source_repository_note_media() {
    let harness = spawn_test_server().await;
    let base = harness.base_url.as_str();
    let client = &harness.client;

    // Person
    let person_id = create_person(client, base, "Roundtrip", "Person").await;

    let person_detail = client
        .get(format!("{base}/api/v1/persons/{person_id}"))
        .send()
        .await
        .expect("get person");
    assert_eq!(person_detail.status(), StatusCode::OK);

    let person_update = client
        .put(format!("{base}/api/v1/persons/{person_id}"))
        .json(&serde_json::json!({
            "given_names": ["Updated"],
            "surnames": [{"value": "Person", "origin_type": "patrilineal", "connector": null}],
            "gender": "unknown"
        }))
        .send()
        .await
        .expect("update person");
    assert_eq!(person_update.status(), StatusCode::OK);

    // Repository
    let repository_created = client
        .post(format!("{base}/api/v1/repositories"))
        .json(&serde_json::json!({
            "name": "Test Repository",
            "address": "Test Address",
            "urls": ["https://example.org/repo"]
        }))
        .send()
        .await
        .expect("create repository");
    assert_eq!(repository_created.status(), StatusCode::CREATED);
    let repository_id = repository_created
        .json::<Value>()
        .await
        .expect("repository body")
        .get("id")
        .and_then(Value::as_str)
        .expect("repository id")
        .to_string();

    let repository_update = client
        .put(format!("{base}/api/v1/repositories/{repository_id}"))
        .json(&serde_json::json!({
            "name": "Updated Repository",
            "address": "Updated Address",
            "urls": ["https://example.org/repo-updated"]
        }))
        .send()
        .await
        .expect("update repository");
    assert_eq!(repository_update.status(), StatusCode::OK);

    // Source
    let source_created = client
        .post(format!("{base}/api/v1/sources"))
        .json(&serde_json::json!({
            "title": "Test Source",
            "author": "Researcher",
            "publication_info": "Journal",
            "abbreviation": "TS",
            "repository_refs": []
        }))
        .send()
        .await
        .expect("create source");
    assert_eq!(source_created.status(), StatusCode::CREATED);
    let source_id = source_created
        .json::<Value>()
        .await
        .expect("source body")
        .get("id")
        .and_then(Value::as_str)
        .expect("source id")
        .to_string();

    let source_update = client
        .put(format!("{base}/api/v1/sources/{source_id}"))
        .json(&serde_json::json!({
            "title": "Updated Source",
            "author": "Researcher",
            "publication_info": "Journal",
            "abbreviation": "US",
            "repository_refs": []
        }))
        .send()
        .await
        .expect("update source");
    assert_eq!(source_update.status(), StatusCode::OK);

    // Event
    let event_created = client
        .post(format!("{base}/api/v1/events"))
        .json(&serde_json::json!({
            "event_type": "Birth",
            "description": "Roundtrip event"
        }))
        .send()
        .await
        .expect("create event");
    assert_eq!(event_created.status(), StatusCode::CREATED);
    let event_id = event_created
        .json::<Value>()
        .await
        .expect("event body")
        .get("id")
        .and_then(Value::as_str)
        .expect("event id")
        .to_string();

    let event_update = client
        .put(format!("{base}/api/v1/events/{event_id}"))
        .json(&serde_json::json!({
            "event_type": "Death",
            "description": "Updated roundtrip event"
        }))
        .send()
        .await
        .expect("update event");
    assert_eq!(event_update.status(), StatusCode::OK);

    // Family
    let partner2_id = create_person(client, base, "Partner", "Two").await;
    let family_created = client
        .post(format!("{base}/api/v1/families"))
        .json(&serde_json::json!({
            "partner1_id": person_id,
            "partner2_id": partner2_id,
            "partner_link": "married",
            "child_ids": []
        }))
        .send()
        .await
        .expect("create family");
    assert_eq!(family_created.status(), StatusCode::CREATED);
    let family_id = family_created
        .json::<Value>()
        .await
        .expect("family body")
        .get("id")
        .and_then(Value::as_str)
        .expect("family id")
        .to_string();

    let family_detail = client
        .get(format!("{base}/api/v1/families/{family_id}"))
        .send()
        .await
        .expect("get family");
    assert_eq!(family_detail.status(), StatusCode::OK);

    // Note
    let note_created = client
        .post(format!("{base}/api/v1/notes"))
        .json(&serde_json::json!({
            "text": "Roundtrip note",
            "note_type": "research",
            "linked_entity_id": person_id,
            "linked_entity_type": "person"
        }))
        .send()
        .await
        .expect("create note");
    assert_eq!(note_created.status(), StatusCode::CREATED);
    let note_id = note_created
        .json::<Value>()
        .await
        .expect("note body")
        .get("id")
        .and_then(Value::as_str)
        .expect("note id")
        .to_string();

    let note_update = client
        .put(format!("{base}/api/v1/notes/{note_id}"))
        .json(&serde_json::json!({
            "text": "Updated roundtrip note",
            "note_type": "research",
            "linked_entity_id": person_id,
            "linked_entity_type": "person"
        }))
        .send()
        .await
        .expect("update note");
    assert_eq!(note_update.status(), StatusCode::OK);

    // Media
    let part = reqwest::multipart::Part::bytes(vec![0xFF, 0xD8, 0xFF, 0xD9]).file_name("tiny.jpg");
    let form = reqwest::multipart::Form::new().part("file", part);
    let media_created = client
        .post(format!("{base}/api/v1/media"))
        .multipart(form)
        .send()
        .await
        .expect("upload media");
    assert_eq!(media_created.status(), StatusCode::CREATED);
    let media_id = media_created
        .json::<Value>()
        .await
        .expect("media body")
        .get("id")
        .and_then(Value::as_str)
        .expect("media id")
        .to_string();

    let media_get = client
        .get(format!("{base}/api/v1/media/{media_id}"))
        .send()
        .await
        .expect("get media");
    assert_eq!(media_get.status(), StatusCode::OK);

    // Delete phase + verify 404
    for (route, id) in [
        ("persons", person_id.as_str()),
        ("families", family_id.as_str()),
        ("events", event_id.as_str()),
        ("sources", source_id.as_str()),
        ("repositories", repository_id.as_str()),
        ("notes", note_id.as_str()),
        ("media", media_id.as_str()),
    ] {
        let delete = client
            .delete(format!("{base}/api/v1/{route}/{id}"))
            .send()
            .await
            .expect("delete entity");
        assert!(
            delete.status() == StatusCode::NO_CONTENT || delete.status() == StatusCode::OK,
            "unexpected delete status for {route}/{id}: {}",
            delete.status()
        );

        let get_after_delete = client
            .get(format!("{base}/api/v1/{route}/{id}"))
            .send()
            .await
            .expect("get after delete");
        assert_eq!(
            get_after_delete.status(),
            StatusCode::NOT_FOUND,
            "expected 404 after deleting {route}/{id}"
        );
    }

    harness.shutdown().await;
}

#[tokio::test]
async fn relationship_integrity_cross_entity_openapi_and_headers() {
    let harness = spawn_test_server_with_kennedy_import().await;
    let base = harness.base_url.as_str();
    let client = &harness.client;

    let person_a = create_person(client, base, "Partner", "A").await;
    let person_b = create_person(client, base, "Partner", "B").await;

    let family_create = client
        .post(format!("{base}/api/v1/families"))
        .json(&serde_json::json!({
            "partner1_id": person_a,
            "partner2_id": person_b,
            "partner_link": "married",
            "child_ids": []
        }))
        .send()
        .await
        .expect("create linked family");
    assert_eq!(family_create.status(), StatusCode::CREATED);

    let family_id = family_create
        .json::<Value>()
        .await
        .expect("family response")
        .get("id")
        .and_then(Value::as_str)
        .expect("family id")
        .to_string();

    let family_detail = client
        .get(format!("{base}/api/v1/families/{family_id}"))
        .send()
        .await
        .expect("family detail");
    assert_eq!(family_detail.status(), StatusCode::OK);

    for person_id in [&person_a, &person_b] {
        let response = client
            .get(format!("{base}/api/v1/persons/{person_id}/families"))
            .send()
            .await
            .expect("person families");
        assert_eq!(response.status(), StatusCode::OK);

        let body: Value = response.json().await.expect("person families body");
        let rows = body.as_array().expect("families array");
        assert!(rows.iter().any(|row| {
            row.get("id")
                .and_then(Value::as_str)
                .is_some_and(|id| id == family_id)
        }));
    }

    let search = client
        .get(format!("{base}/api/v1/search?q=Kennedy&type=person"))
        .send()
        .await
        .expect("search kennedy");
    assert_eq!(search.status(), StatusCode::OK);
    let search_body: Value = search.json().await.expect("search body");
    let search_rows = search_body
        .get("results")
        .and_then(Value::as_array)
        .expect("search results");
    assert!(search_rows.len() > 5, "expected >5 Kennedy search results");

    let jfk_id = search_rows
        .iter()
        .find_map(|row| {
            let display = row
                .get("display_name")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if display.contains("John") && display.contains("Kennedy") {
                row.get("entity_id").and_then(Value::as_str)
            } else {
                None
            }
        })
        .or_else(|| {
            search_rows
                .first()
                .and_then(|row| row.get("entity_id").and_then(Value::as_str))
        })
        .expect("JFK id")
        .to_string();

    let pedigree = client
        .get(format!(
            "{base}/api/v1/graph/pedigree/{jfk_id}?generations=4"
        ))
        .send()
        .await
        .expect("pedigree request");
    assert_eq!(pedigree.status(), StatusCode::OK);
    let pedigree_body: Value = pedigree.json().await.expect("pedigree body");
    let nodes = pedigree_body
        .get("nodes")
        .and_then(Value::as_array)
        .expect("pedigree nodes");
    assert!(!nodes.is_empty(), "pedigree should return nodes");

    let openapi = client
        .get(format!("{base}/api/v1/openapi.json"))
        .send()
        .await
        .expect("openapi request");
    assert_eq!(openapi.status(), StatusCode::OK);

    for header in [
        "x-content-type-options",
        "x-frame-options",
        "x-xss-protection",
        "referrer-policy",
    ] {
        assert!(
            openapi.headers().get(header).is_some(),
            "missing security header {header}"
        );
    }

    let openapi_body: Value = openapi.json().await.expect("openapi body");
    let paths = openapi_body
        .get("paths")
        .and_then(Value::as_object)
        .expect("openapi paths");

    for expected in [
        "/api/v1/persons",
        "/api/v1/families",
        "/api/v1/events",
        "/api/v1/sources",
        "/api/v1/repositories",
        "/api/v1/notes",
        "/api/v1/media",
        "/api/v1/staging",
        "/api/v1/search",
        "/api/v1/graph/ancestors/{id}",
        "/api/v1/import",
        "/api/v1/events/stream",
    ] {
        assert!(
            paths.contains_key(expected),
            "missing endpoint in spec: {expected}"
        );
    }

    let empty_search = client
        .get(format!("{base}/api/v1/search?q="))
        .send()
        .await
        .expect("empty search");
    assert_eq!(empty_search.status(), StatusCode::BAD_REQUEST);

    harness.shutdown().await;
}
