use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use chrono::Utc;
use rusqlite::Connection;
use rustygene_core::assertion::{Assertion, AssertionStatus, EvidenceType};
use rustygene_core::event::{Event, EventParticipant, EventRole, EventType};
use rustygene_core::evidence::{Citation, CitationRef, Media, Note, NoteType, Repository, RepositoryType, Source};
use rustygene_core::family::{ChildLink, Family, LineageType, PartnerLink};
use rustygene_core::lds::{LdsOrdinance, LdsOrdinanceType, LdsStatus};
use rustygene_core::person::Person;
use rustygene_core::place::{Place, PlaceName, PlaceType};
use rustygene_core::research::{ResearchLogEntry, SearchResult};
use rustygene_core::types::{ActorRef, EntityId, Gender};
use rustygene_storage::sqlite_impl::SqliteBackend;
use rustygene_storage::{
    AuditLogEntry, EntityType, Pagination, RelationshipEdge, ResearchLogFilter, Storage,
    run_migrations,
};
use serde_json::json;

fn temp_db_path() -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "rustygene-storage-it-{}-{}.sqlite",
        std::process::id(),
        nanos
    ))
}

fn setup_backend() -> (SqliteBackend, PathBuf) {
    let db_path = temp_db_path();
    let mut conn = Connection::open(&db_path).expect("open sqlite file");
    run_migrations(&mut conn).expect("run migrations");
    (SqliteBackend::new(conn), db_path)
}

fn person(id: EntityId) -> Person {
    Person {
        id,
        names: vec![],
        gender: Gender::Unknown,
        living: true,
        private: false,
        original_xref: None,
        _raw_gedcom: BTreeMap::new(),
    }
}

#[tokio::test]
async fn sqlite_end_to_end_storage_flow() {
    let (backend, db_path) = setup_backend();

    // Core people and family
    let father_id = EntityId::new();
    let mother_id = EntityId::new();
    let child1_id = EntityId::new();
    let child2_id = EntityId::new();
    let child3_id = EntityId::new();

    backend
        .create_person(&person(father_id))
        .await
        .expect("create father");
    backend
        .create_person(&person(mother_id))
        .await
        .expect("create mother");
    backend
        .create_person(&person(child1_id))
        .await
        .expect("create child1");
    backend
        .create_person(&person(child2_id))
        .await
        .expect("create child2");
    backend
        .create_person(&person(child3_id))
        .await
        .expect("create child3");

    let family = Family {
        id: EntityId::new(),
        partner1_id: Some(father_id),
        partner2_id: Some(mother_id),
        partner_link: PartnerLink::Married,
        couple_relationship: None,
        child_links: vec![
            ChildLink {
                child_id: child1_id,
                lineage_type: LineageType::Biological,
            },
            ChildLink {
                child_id: child2_id,
                lineage_type: LineageType::Biological,
            },
            ChildLink {
                child_id: child3_id,
                lineage_type: LineageType::Biological,
            },
        ],
        original_xref: None,
        _raw_gedcom: BTreeMap::new(),
    };
    backend.create_family(&family).await.expect("create family");
    let fetched_family = backend.get_family(family.id).await.expect("get family");
    assert_eq!(fetched_family.child_links.len(), 3);

    // Marriage event
    let marriage_event = Event {
        id: EntityId::new(),
        event_type: EventType::Marriage,
        date: None,
        place_ref: None,
        participants: vec![
            EventParticipant {
                person_id: father_id,
                role: EventRole::Spouse,
                census_role: None,
            },
            EventParticipant {
                person_id: mother_id,
                role: EventRole::Spouse,
                census_role: None,
            },
        ],
        description: Some("Marriage event".to_string()),
        _raw_gedcom: BTreeMap::new(),
    };
    backend
        .create_event(&marriage_event)
        .await
        .expect("create marriage event");
    let fetched_event = backend
        .get_event(marriage_event.id)
        .await
        .expect("get marriage event");
    assert_eq!(fetched_event.participants.len(), 2);

    // Place CRUD coverage
    let place = Place {
        id: EntityId::new(),
        place_type: PlaceType::City,
        names: vec![PlaceName {
            name: "Cardiff".to_string(),
            language: Some("en".to_string()),
            date_range: None,
        }],
        coordinates: Some((51.4816, -3.1791)),
        enclosed_by: vec![],
        external_ids: vec![],
    };
    backend.create_place(&place).await.expect("create place");
    let fetched_place = backend.get_place(place.id).await.expect("get place");
    assert_eq!(fetched_place.place_type, PlaceType::City);
    assert_eq!(fetched_place.names[0].name, "Cardiff");
    let listed_places = backend
        .list_places(Pagination {
            limit: 50,
            offset: 0,
        })
        .await
        .expect("list places");
    assert!(listed_places.iter().any(|p| p.id == place.id));

    // Note CRUD coverage
    let note = Note {
        id: EntityId::new(),
        text: "Integration test note".to_string(),
        note_type: NoteType::Research,
        original_xref: Some("@N1@".to_string()),
        _raw_gedcom: BTreeMap::new(),
    };
    backend.create_note(&note).await.expect("create note");
    let fetched_note = backend.get_note(note.id).await.expect("get note");
    assert_eq!(fetched_note.text, "Integration test note");
    assert_eq!(fetched_note.note_type, NoteType::Research);
    let listed_notes = backend
        .list_notes(Pagination {
            limit: 50,
            offset: 0,
        })
        .await
        .expect("list notes");
    assert!(listed_notes.iter().any(|n| n.id == note.id));

    // Media CRUD coverage
    let media = Media {
        id: EntityId::new(),
        file_path: "media/marriage-register.jpg".to_string(),
        content_hash: "sha256:integration-test".to_string(),
        mime_type: "image/jpeg".to_string(),
        thumbnail_path: Some("media/thumbs/marriage-register.jpg".to_string()),
        ocr_text: Some("Marriage register OCR sample".to_string()),
        dimensions_px: None,
        physical_dimensions_mm: None,
        caption: Some("Marriage Register Scan".to_string()),
        original_xref: Some("@O1@".to_string()),
        _raw_gedcom: BTreeMap::new(),
    };
    backend.create_media(&media).await.expect("create media");
    let fetched_media = backend.get_media(media.id).await.expect("get media");
    assert_eq!(fetched_media.mime_type, "image/jpeg");
    assert_eq!(fetched_media.caption.as_deref(), Some("Marriage Register Scan"));
    let listed_media = backend
        .list_media(Pagination {
            limit: 50,
            offset: 0,
        })
        .await
        .expect("list media");
    assert!(listed_media.iter().any(|m| m.id == media.id));

    // LDS Ordinance CRUD coverage
    let lds_ordinance = LdsOrdinance {
        id: EntityId::new(),
        ordinance_type: LdsOrdinanceType::Baptism,
        status: LdsStatus::Completed,
        temple_code: Some("LON".to_string()),
        date: None,
        place_ref: Some(place.id),
        family_ref: Some(family.id),
        _raw_gedcom: BTreeMap::new(),
    };
    backend
        .create_lds_ordinance(&lds_ordinance)
        .await
        .expect("create lds ordinance");
    let fetched_lds = backend
        .get_lds_ordinance(lds_ordinance.id)
        .await
        .expect("get lds ordinance");
    assert_eq!(fetched_lds.ordinance_type, LdsOrdinanceType::Baptism);
    assert_eq!(fetched_lds.status, LdsStatus::Completed);
    assert_eq!(fetched_lds.temple_code.as_deref(), Some("LON"));
    let listed_lds = backend
        .list_lds_ordinances(Pagination {
            limit: 50,
            offset: 0,
        })
        .await
        .expect("list lds ordinances");
    assert!(listed_lds.iter().any(|o| o.id == lds_ordinance.id));

    // Source chain: repository -> source -> citation
    let repository = Repository {
        id: EntityId::new(),
        name: "National Archive".to_string(),
        repository_type: RepositoryType::Archive,
        address: None,
        urls: vec![],
        original_xref: None,
        _raw_gedcom: BTreeMap::new(),
    };
    backend
        .create_repository(&repository)
        .await
        .expect("create repository");

    let source = Source {
        id: EntityId::new(),
        title: "Marriage Register".to_string(),
        author: None,
        publication_info: None,
        abbreviation: None,
        repository_refs: vec![],
        original_xref: None,
        _raw_gedcom: BTreeMap::new(),
    };
    backend.create_source(&source).await.expect("create source");

    let citation = Citation {
        id: EntityId::new(),
        source_id: source.id,
        volume: None,
        page: Some("42".to_string()),
        folio: None,
        entry: None,
        confidence_level: Some(3),
        date_accessed: None,
        transcription: Some("Marriage line item".to_string()),
        _raw_gedcom: BTreeMap::new(),
    };
    backend
        .create_citation(&citation)
        .await
        .expect("create citation");

    // Assertions: idempotency + preferred/status + snapshot recomputation
    let first_name_assertion: Assertion<serde_json::Value> = Assertion {
        id: EntityId::new(),
        value: json!("John Original"),
        confidence: 0.9,
        status: AssertionStatus::Confirmed,
        evidence_type: EvidenceType::Direct,
        source_citations: vec![CitationRef {
            citation_id: citation.id,
            note: None,
        }],
        proposed_by: ActorRef::Import("integration".to_string()),
        created_at: Utc::now(),
        reviewed_at: None,
        reviewed_by: None,
    };
    backend
        .create_assertion(father_id, EntityType::Person, "name", &first_name_assertion)
        .await
        .expect("create first name assertion");

    // Duplicate factual assertion should be no-op even with different assertion id.
    let duplicate = Assertion {
        id: EntityId::new(),
        ..first_name_assertion.clone()
    };
    backend
        .create_assertion(father_id, EntityType::Person, "name", &duplicate)
        .await
        .expect("duplicate assertion should be ignored");

    let second_name_assertion: Assertion<serde_json::Value> = Assertion {
        id: EntityId::new(),
        value: json!("John Preferred"),
        confidence: 0.95,
        status: AssertionStatus::Proposed,
        evidence_type: EvidenceType::Direct,
        source_citations: vec![CitationRef {
            citation_id: citation.id,
            note: Some("alt spelling".to_string()),
        }],
        proposed_by: ActorRef::User("tester".to_string()),
        created_at: Utc::now(),
        reviewed_at: None,
        reviewed_by: None,
    };
    backend
        .create_assertion(
            father_id,
            EntityType::Person,
            "name",
            &second_name_assertion,
        )
        .await
        .expect("create second name assertion");

    backend
        .update_assertion_status(second_name_assertion.id, AssertionStatus::Confirmed)
        .await
        .expect("promote second assertion to confirmed/preferred");

    let name_assertions = backend
        .list_assertions_for_field(father_id, "name")
        .await
        .expect("list name assertions");
    assert_eq!(
        name_assertions.len(),
        2,
        "duplicate assertion must not create a 3rd row"
    );

    // Relationship graph edges and traversal
    backend
        .upsert_relationship_edge(&RelationshipEdge {
            from_entity: father_id,
            to_entity: child1_id,
            rel_type: "parent_of".to_string(),
            directed: true,
            assertion_id: None,
        })
        .await
        .expect("upsert father->child1");
    backend
        .upsert_relationship_edge(&RelationshipEdge {
            from_entity: father_id,
            to_entity: child2_id,
            rel_type: "parent_of".to_string(),
            directed: true,
            assertion_id: None,
        })
        .await
        .expect("upsert father->child2");
    backend
        .upsert_relationship_edge(&RelationshipEdge {
            from_entity: father_id,
            to_entity: child3_id,
            rel_type: "parent_of".to_string(),
            directed: true,
            assertion_id: None,
        })
        .await
        .expect("upsert father->child3");

    let descendants = backend
        .descendants(father_id, 2)
        .await
        .expect("query descendants");
    assert_eq!(descendants.len(), 3);

    let ancestors = backend
        .ancestors(child1_id, 2)
        .await
        .expect("query ancestors");
    assert_eq!(ancestors, vec![father_id]);

    // Research log CRUD and filtering
    let research_entry = ResearchLogEntry {
        id: EntityId::new(),
        date: Utc::now(),
        objective: "Find corroborating birth records".to_string(),
        repository: Some(repository.id),
        repository_name: Some(repository.name.clone()),
        search_terms: vec!["john".to_string(), "birth".to_string()],
        source_searched: Some(source.id),
        result: SearchResult::PartiallyFound,
        findings: Some("Located two candidate entries".to_string()),
        citations_created: vec![citation.id],
        next_steps: Some("Inspect parish microfilm".to_string()),
        person_refs: vec![father_id],
        tags: vec!["birth".to_string(), "followup".to_string()],
    };
    backend
        .create_research_log_entry(&research_entry)
        .await
        .expect("create research entry");

    let filtered = backend
        .list_research_log_entries(
            &ResearchLogFilter {
                person_ref: Some(father_id),
                result: Some(SearchResult::PartiallyFound),
                date_from_iso: None,
                date_to_iso: None,
            },
            Pagination {
                limit: 10,
                offset: 0,
            },
        )
        .await
        .expect("filter research entries");
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].id, research_entry.id);

    // Audit log append
    backend
        .append_audit_log_entry(&AuditLogEntry {
            actor: "user:integration".to_string(),
            entity_id: father_id,
            entity_type: EntityType::Person,
            action: "set_preferred_name".to_string(),
            old_value_json: Some(json!({"name": "John Original"})),
            new_value_json: Some(json!({"name": "John Preferred"})),
            timestamp_iso: Utc::now().to_rfc3339(),
        })
        .await
        .expect("append audit log");

    // Direct DB checks for write-through snapshot and audit persistence.
    let db_conn = Connection::open(&db_path).expect("open db for verification");
    let person_data: String = db_conn
        .query_row(
            "SELECT data FROM persons WHERE id = ?",
            rusqlite::params![father_id.to_string()],
            |row| row.get(0),
        )
        .expect("read person data");
    let person_json: serde_json::Value =
        serde_json::from_str(&person_data).expect("parse person data");
    assert_eq!(person_json["name"], json!("John Preferred"));

    let audit_count: i64 = db_conn
        .query_row("SELECT COUNT(*) FROM audit_log", [], |row| row.get(0))
        .expect("count audit rows");
    assert!(audit_count >= 1);

    drop(db_conn);
    drop(backend);
    let _ = fs::remove_file(&db_path);
}
