use std::sync::Arc;

use rusqlite::Connection;
use rustygene_core::event::{Event, EventParticipant, EventRole, EventType};
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
async fn event_crud_create_event_with_participants() {
    let backend = in_memory_backend();
    let person_id = create_test_person(&backend, "John", "Smith").await;

    let event_id = EntityId::new();
    let event = Event {
        id: event_id,
        event_type: EventType::Marriage,
        date: None,
        place_ref: None,
        description: Some("Wedding ceremony".to_string()),
        participants: vec![EventParticipant {
            person_id,
            role: EventRole::Principal,
            census_role: None,
        }],
        _raw_gedcom: Default::default(),
    };

    backend.create_event(&event).await.expect("create event");

    let retrieved = backend.get_event(event_id).await.expect("get_event");

    assert_eq!(retrieved.event_type, EventType::Marriage);
    assert_eq!(retrieved.participants.len(), 1);
    assert_eq!(retrieved.participants[0].person_id, person_id);
}

#[tokio::test]
async fn event_crud_list_events_returns_all() {
    let backend = in_memory_backend();

    let event1_id = EntityId::new();
    let event1 = Event {
        id: event1_id,
        event_type: EventType::Birth,
        date: None,
        place_ref: None,
        description: Some("Birth event".to_string()),
        participants: vec![],
        _raw_gedcom: Default::default(),
    };

    let event2_id = EntityId::new();
    let event2 = Event {
        id: event2_id,
        event_type: EventType::Death,
        date: None,
        place_ref: None,
        description: Some("Death event".to_string()),
        participants: vec![],
        _raw_gedcom: Default::default(),
    };

    backend.create_event(&event1).await.expect("create event 1");
    backend.create_event(&event2).await.expect("create event 2");

    let events = backend
        .list_events(Pagination::default())
        .await
        .expect("list_events");

    assert_eq!(events.len(), 2);
}

#[tokio::test]
async fn event_crud_update_event() {
    let backend = in_memory_backend();

    let event_id = EntityId::new();
    let mut event = Event {
        id: event_id,
        event_type: EventType::Marriage,
        date: None,
        place_ref: None,
        description: Some("Initial description".to_string()),
        participants: vec![],
        _raw_gedcom: Default::default(),
    };

    backend.create_event(&event).await.expect("create event");

    event.event_type = EventType::Death;
    event.description = Some("Updated description".to_string());

    backend.update_event(&event).await.expect("update_event");

    let retrieved = backend.get_event(event_id).await.expect("get_event");

    assert_eq!(retrieved.event_type, EventType::Death);
    assert_eq!(
        retrieved.description,
        Some("Updated description".to_string())
    );
}

#[tokio::test]
async fn event_crud_delete_event() {
    let backend = in_memory_backend();

    let event_id = EntityId::new();
    let event = Event {
        id: event_id,
        event_type: EventType::Birth,
        date: None,
        place_ref: None,
        description: Some("Birth event".to_string()),
        participants: vec![],
        _raw_gedcom: Default::default(),
    };

    backend.create_event(&event).await.expect("create event");

    backend.delete_event(event_id).await.expect("delete_event");

    let result = backend.get_event(event_id).await;

    assert!(result.is_err(), "event should be deleted");
}

#[tokio::test]
async fn event_participant_add_and_remove() {
    let backend = in_memory_backend();
    let person1_id = create_test_person(&backend, "John", "Smith").await;
    let person2_id = create_test_person(&backend, "Jane", "Smith").await;

    let event_id = EntityId::new();
    let mut event = Event {
        id: event_id,
        event_type: EventType::Marriage,
        date: None,
        place_ref: None,
        description: None,
        participants: vec![
            EventParticipant {
                person_id: person1_id,
                role: EventRole::Principal,
                census_role: None,
            },
            EventParticipant {
                person_id: person2_id,
                role: EventRole::Principal,
                census_role: None,
            },
        ],
        _raw_gedcom: Default::default(),
    };

    backend.create_event(&event).await.expect("create event");

    // Verify two participants
    let retrieved = backend.get_event(event_id).await.expect("get_event");
    assert_eq!(retrieved.participants.len(), 2);

    // Remove one participant
    event.participants.retain(|p| p.person_id != person1_id);
    backend.update_event(&event).await.expect("update_event");

    // Verify only one remains
    let updated = backend.get_event(event_id).await.expect("get_event");
    assert_eq!(updated.participants.len(), 1);
    assert_eq!(updated.participants[0].person_id, person2_id);
}

#[tokio::test]
async fn event_type_validation_accepts_custom_types() {
    let backend = in_memory_backend();

    let event_id = EntityId::new();
    let event = Event {
        id: event_id,
        event_type: EventType::Custom("custom_event_type".to_string()),
        date: None,
        place_ref: None,
        description: None,
        participants: vec![],
        _raw_gedcom: Default::default(),
    };

    // Should create successfully with custom type
    backend
        .create_event(&event)
        .await
        .expect("create event with custom type");

    let retrieved = backend.get_event(event_id).await.expect("get_event");

    match retrieved.event_type {
        EventType::Custom(ref s) => assert_eq!(s, "custom_event_type"),
        _ => panic!("Expected custom event type"),
    }
}
