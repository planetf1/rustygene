use std::sync::Arc;

use rusqlite::Connection;
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
