use std::sync::Arc;

use reqwest::StatusCode;
use rusqlite::Connection;
use rustygene_api::{start_server, AppState};
use rustygene_core::family::{ChildLink, Family, PartnerLink, Relationship, RelationshipType};
use rustygene_core::person::{Person, PersonName, Surname};
use rustygene_core::types::{EntityId, Gender};
use rustygene_storage::run_migrations;
use rustygene_storage::sqlite_impl::SqliteBackend;
use rustygene_storage::{Pagination, Storage};

fn in_memory_backend() -> Arc<SqliteBackend> {
    let mut conn = Connection::open_in_memory().expect("open in-memory sqlite connection");
    run_migrations(&mut conn).expect("run sqlite migrations");
    Arc::new(SqliteBackend::new(conn))
}

async fn create_test_person(
    backend: &Arc<SqliteBackend>,
    given_names: &str,
    surname: &str,
) -> EntityId {
    let person_id = EntityId::new();
    let person = Person {
        id: person_id,
        names: vec![PersonName {
            given_names: given_names.to_string(),
            surnames: vec![Surname {
                value: surname.to_string(),
                origin_type: Default::default(),
                connector: None,
            }],
            ..Default::default()
        }],
        gender: Gender::Unknown,
        living: true,
        private: false,
        original_xref: None,
        _raw_gedcom: Default::default(),
    };
    backend.create_person(&person).await.expect("create person");
    person_id
}

#[tokio::test]
async fn family_crud_create_family_with_partners() {
    let backend = in_memory_backend();

    let person1_id = create_test_person(&backend, "John", "Smith").await;
    let person2_id = create_test_person(&backend, "Jane", "Smith").await;

    let family_id = EntityId::new();
    let family = Family {
        id: family_id,
        partner1_id: Some(person1_id),
        partner2_id: Some(person2_id),
        partner_link: PartnerLink::Married,
        couple_relationship: None,
        child_links: vec![],
        original_xref: None,
        _raw_gedcom: Default::default(),
    };

    backend.create_family(&family).await.expect("create family");

    // Retrieve and verify
    let retrieved = backend.get_family(family_id).await.expect("get_family");

    assert_eq!(retrieved.partner1_id, Some(person1_id));
    assert_eq!(retrieved.partner2_id, Some(person2_id));
    assert_eq!(retrieved.partner_link, PartnerLink::Married);
}

#[tokio::test]
async fn family_crud_list_families_returns_all() {
    let backend = in_memory_backend();

    let person1_id = create_test_person(&backend, "John", "Smith").await;
    let person2_id = create_test_person(&backend, "Jane", "Smith").await;

    let family_id = EntityId::new();
    let family = Family {
        id: family_id,
        partner1_id: Some(person1_id),
        partner2_id: Some(person2_id),
        partner_link: PartnerLink::Married,
        couple_relationship: None,
        child_links: vec![],
        original_xref: None,
        _raw_gedcom: Default::default(),
    };

    backend.create_family(&family).await.expect("create family");

    let families = backend
        .list_families(Pagination::default())
        .await
        .expect("list_families");

    assert_eq!(families.len(), 1);
    assert_eq!(families[0].id, family_id);
}

#[tokio::test]
async fn family_crud_update_family() {
    let backend = in_memory_backend();

    let person1_id = create_test_person(&backend, "John", "Smith").await;
    let person2_id = create_test_person(&backend, "Jane", "Smith").await;
    let child_id = create_test_person(&backend, "Child", "Smith").await;

    let family_id = EntityId::new();
    let mut family = Family {
        id: family_id,
        partner1_id: Some(person1_id),
        partner2_id: Some(person2_id),
        partner_link: PartnerLink::Married,
        couple_relationship: None,
        child_links: vec![],
        original_xref: None,
        _raw_gedcom: Default::default(),
    };

    backend.create_family(&family).await.expect("create family");

    family.child_links = vec![ChildLink {
        child_id,
        lineage_type: Default::default(),
    }];

    backend.update_family(&family).await.expect("update_family");

    let retrieved = backend.get_family(family_id).await.expect("get_family");

    assert_eq!(retrieved.child_links.len(), 1);
    assert_eq!(retrieved.child_links[0].child_id, child_id);
}

#[tokio::test]
async fn family_crud_delete_family() {
    let backend = in_memory_backend();

    let person1_id = create_test_person(&backend, "John", "Smith").await;
    let person2_id = create_test_person(&backend, "Jane", "Smith").await;

    let family_id = EntityId::new();
    let family = Family {
        id: family_id,
        partner1_id: Some(person1_id),
        partner2_id: Some(person2_id),
        partner_link: PartnerLink::Married,
        couple_relationship: None,
        child_links: vec![],
        original_xref: None,
        _raw_gedcom: Default::default(),
    };

    backend.create_family(&family).await.expect("create family");

    backend
        .delete_family(family_id)
        .await
        .expect("delete_family");

    let result = backend.get_family(family_id).await;

    assert!(result.is_err(), "family should be deleted");
}

#[tokio::test]
async fn family_principle2_linking_creates_relationship() {
    let backend = in_memory_backend();

    let person1_id = create_test_person(&backend, "John", "Smith").await;
    let person2_id = create_test_person(&backend, "Jane", "Smith").await;

    let family_id = EntityId::new();
    let family = Family {
        id: family_id,
        partner1_id: Some(person1_id),
        partner2_id: Some(person2_id),
        partner_link: PartnerLink::Married,
        couple_relationship: None,
        child_links: vec![],
        original_xref: None,
        _raw_gedcom: Default::default(),
    };

    backend.create_family(&family).await.expect("create family");

    let relationship_id = EntityId::new();
    let relationship = Relationship {
        id: relationship_id,
        person1_id,
        person2_id,
        relationship_type: RelationshipType::Couple,
        supporting_event: None,
        _raw_gedcom: Default::default(),
    };

    backend
        .create_relationship(&relationship)
        .await
        .expect("create_relationship");

    let relationships = backend
        .list_relationships(Pagination::default())
        .await
        .expect("list_relationships");

    let found = relationships.iter().find(|r| {
        r.person1_id == person1_id
            && r.person2_id == person2_id
            && r.relationship_type == RelationshipType::Couple
    });

    assert!(found.is_some(), "Couple relationship should exist");
}

/// Regression test for rustygene-0dx: family children must show real display names,
/// not "Person {uuid}" fallbacks. Previously the children lookup only searched
/// partner1/partner2 person objects, so child names were always the fallback string.
#[tokio::test]
async fn family_api_children_show_display_names_not_uuid() {
    let backend = in_memory_backend();
    let state = AppState::with_default_cors(backend.clone(), 0).expect("build app state");
    let server = start_server(state, 0).await.expect("start server");
    let client = reqwest::Client::new();
    let base = format!("http://{}/api/v1", server.local_addr);

    // Create persons via storage directly so we control the IDs
    let parent1_id = create_test_person(&backend, "John", "Senior").await;
    let parent2_id = create_test_person(&backend, "Jane", "Senior").await;
    let child_id = create_test_person(&backend, "Alice", "Senior").await;

    // Create family via the API with the child included
    let create_body = serde_json::json!({
        "partner1_id": parent1_id,
        "partner2_id": parent2_id,
        "partner_link": "married",
        "child_ids": [child_id]
    });
    let create_resp = client
        .post(format!("{base}/families"))
        .json(&create_body)
        .send()
        .await
        .expect("create family");
    assert_eq!(create_resp.status(), StatusCode::CREATED);
    let created: serde_json::Value = create_resp.json().await.expect("parse create response");
    let family_id = created["id"].as_str().expect("family id");

    // --- Test GET /api/v1/families/{id} ---
    let detail_resp = client
        .get(format!("{base}/families/{family_id}"))
        .send()
        .await
        .expect("get family detail");
    assert_eq!(detail_resp.status(), StatusCode::OK);
    let detail: serde_json::Value = detail_resp.json().await.expect("parse detail response");

    let children = detail["children"].as_array().expect("children array");
    assert_eq!(children.len(), 1, "expected one child");
    let child_name = children[0]["display_name"].as_str().expect("display_name");
    assert!(
        child_name.contains("Alice") || child_name.contains("Senior"),
        "child display_name should contain the person's real name, got: {child_name}"
    );
    assert!(
        !child_name.starts_with("Person "),
        "child display_name must not be the UUID fallback, got: {child_name}"
    );

    // --- Test GET /api/v1/families (list) ---
    let list_resp = client
        .get(format!("{base}/families"))
        .send()
        .await
        .expect("list families");
    assert_eq!(list_resp.status(), StatusCode::OK);
    let list: serde_json::Value = list_resp.json().await.expect("parse list response");
    let items = list["items"].as_array().expect("items array");
    let family_in_list = items
        .iter()
        .find(|f| f["id"].as_str() == Some(family_id))
        .expect("family in list");
    let list_children = family_in_list["children"].as_array().expect("children");
    assert_eq!(list_children.len(), 1);
    let list_child_name = list_children[0]["display_name"]
        .as_str()
        .expect("display_name in list");
    assert!(
        list_child_name.contains("Alice") || list_child_name.contains("Senior"),
        "list child display_name should contain real name, got: {list_child_name}"
    );
    assert!(
        !list_child_name.starts_with("Person "),
        "list child display_name must not be the UUID fallback, got: {list_child_name}"
    );

    server.shutdown().await.ok();
}
