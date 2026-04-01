/// Phase 1A end-to-end acceptance gate test.
///
/// Exercises the full pipeline: GEDCOM import → person query → person show →
/// GEDCOM export → GEDCOM re-import → assertion comparison → JSON export →
/// JSON re-import → assertion comparison.
///
/// Spec reference: docs/INITIAL_SPEC.md §8.3
use std::path::PathBuf;

use rusqlite::Connection;
use rustygene_core::event::Event;
use rustygene_core::family::Family;
use rustygene_core::person::Person;
use rustygene_core::place::Place;
use rustygene_gedcom::{
    ExportPrivacyPolicy, family_to_fam_node, import_gedcom_to_sqlite,
    person_to_indi_node_with_policy, render_gedcom_file, repository_to_repo_node,
    source_to_sour_node,
};
use rustygene_storage::{
    JsonExportMode, JsonImportMode, Pagination, Storage, run_migrations, sqlite_impl::SqliteBackend,
};

fn temp_db_path(label: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "rustygene-e2e-{}-{}-{}.sqlite",
        label,
        std::process::id(),
        nanos
    ))
}

fn setup_db(path: &PathBuf) -> Connection {
    let mut conn = Connection::open(path).expect("open db");
    run_migrations(&mut conn).expect("run migrations");
    conn
}

/// Read kennedy.ged with UTF-8 / Latin-1 fallback.
/// Test binaries run from the crate root (`crates/gedcom/`), so the
/// workspace testdata is two directories up.
fn kennedy_ged_content() -> String {
    let path = PathBuf::from("../../testdata/gedcom/kennedy.ged");
    match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::InvalidData => {
            let bytes = std::fs::read(&path).expect("read kennedy.ged bytes");
            bytes.iter().map(|&b| b as char).collect()
        }
        Err(e) => panic!("failed to read kennedy.ged: {}", e),
    }
}

fn load_persons_from_snapshot(conn: &Connection) -> Vec<Person> {
    let mut stmt = conn
        .prepare("SELECT data FROM persons ORDER BY created_at")
        .expect("prepare persons");
    stmt.query_map([], |row| row.get::<_, String>(0))
        .expect("query persons")
        .collect::<Result<Vec<_>, _>>()
        .expect("collect persons")
        .into_iter()
        .map(|raw| serde_json::from_str::<Person>(&raw).expect("parse person json"))
        .collect()
}

fn load_families_from_snapshot(conn: &Connection) -> Vec<Family> {
    let mut stmt = conn
        .prepare(
            "SELECT data FROM families \
             WHERE json_extract(data, '$.relationship_type') IS NULL \
             ORDER BY created_at",
        )
        .expect("prepare families");
    stmt.query_map([], |row| row.get::<_, String>(0))
        .expect("query families")
        .collect::<Result<Vec<_>, _>>()
        .expect("collect families")
        .into_iter()
        .map(|raw| serde_json::from_str::<Family>(&raw).expect("parse family json"))
        .collect()
}

/// Count total confirmed assertions in a database.
fn load_places_from_snapshot(conn: &Connection) -> Vec<Place> {
    let mut stmt = conn
        .prepare("SELECT data FROM places ORDER BY created_at")
        .expect("prepare places");
    stmt.query_map([], |row| row.get::<_, String>(0))
        .expect("query places")
        .collect::<Result<Vec<_>, _>>()
        .expect("collect places")
        .into_iter()
        .map(|raw| serde_json::from_str::<Place>(&raw).expect("parse place json"))
        .collect()
}

fn load_events_from_snapshot(conn: &Connection) -> Vec<Event> {
    let mut stmt = conn
        .prepare("SELECT data FROM events ORDER BY created_at")
        .expect("prepare events");
    stmt.query_map([], |row| row.get::<_, String>(0))
        .expect("query events")
        .collect::<Result<Vec<_>, _>>()
        .expect("collect events")
        .into_iter()
        .map(|raw| serde_json::from_str::<Event>(&raw).expect("parse event json"))
        .collect()
}

/// Count total assertions (any status) in a database.
fn count_all_assertions(conn: &Connection) -> usize {
    conn.query_row("SELECT COUNT(*) FROM assertions", [], |row| {
        row.get::<_, usize>(0)
    })
    .expect("count all assertions")
}

fn assertion_field_distribution_for_gedcom(
    conn: &Connection,
) -> std::collections::BTreeMap<(String, String), usize> {
    let mut stmt = conn
        .prepare(
            "SELECT entity_type, field, COUNT(*) as cnt \
             FROM assertions \
             GROUP BY entity_type, field \
             ORDER BY entity_type, field",
        )
        .expect("prepare assertion distribution query (gedcom)");
    let rows = stmt
        .query_map([], |row| {
            Ok((
                (row.get::<_, String>(0)?, row.get::<_, String>(1)?),
                row.get::<_, usize>(2)?,
            ))
        })
        .expect("query assertion distribution (gedcom)");
    rows.into_iter()
        .map(|row| row.expect("distribution row"))
        .collect()
}

fn assertion_field_distribution_for_json(
    conn: &Connection,
) -> std::collections::BTreeMap<(String, String), usize> {
    let mut stmt = conn
        .prepare(
            "SELECT entity_type, field, COUNT(*) as cnt \
             FROM assertions \
             GROUP BY entity_type, field \
             ORDER BY entity_type, field",
        )
        .expect("prepare assertion distribution query (json)");
    let rows = stmt
        .query_map([], |row| {
            Ok((
                (row.get::<_, String>(0)?, row.get::<_, String>(1)?),
                row.get::<_, usize>(2)?,
            ))
        })
        .expect("query assertion distribution (json)");
    rows.into_iter()
        .map(|row| row.expect("distribution row"))
        .collect()
}

fn event_type_distribution(events: &[Event]) -> std::collections::BTreeMap<String, usize> {
    let mut out = std::collections::BTreeMap::new();
    for event in events {
        let key = serde_json::to_string(&event.event_type).expect("serialize event_type");
        *out.entry(key).or_insert(0) += 1;
    }
    out
}

fn family_structure_distribution(
    families: &[Family],
) -> std::collections::BTreeMap<(bool, bool, usize), usize> {
    let mut out = std::collections::BTreeMap::new();
    for family in families {
        let key = (
            family.partner1_id.is_some(),
            family.partner2_id.is_some(),
            family.child_links.len(),
        );
        *out.entry(key).or_insert(0) += 1;
    }
    out
}

#[tokio::test]
async fn e2e_phase1a_gate_test() {
    let db1_path = temp_db_path("db1");
    let db2_path = temp_db_path("db2");
    let db3_path = temp_db_path("db3");

    let json_export_dir = std::env::temp_dir().join(format!(
        "rustygene-e2e-json-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    std::fs::create_dir_all(&json_export_dir).expect("create json export dir");

    // =========================================================================
    // Step 1: Import kennedy.ged into DB1
    // =========================================================================
    let content = kennedy_ged_content();
    let mut conn1 = setup_db(&db1_path);

    let report1 = import_gedcom_to_sqlite(&mut conn1, "e2e-gate-import-1", &content)
        .expect("import kennedy.ged to DB1 must succeed");

    let person_count1 = *report1.entities_created_by_type.get("person").unwrap_or(&0);
    let family_count1 = *report1.entities_created_by_type.get("family").unwrap_or(&0);
    let place_count1 = *report1.entities_created_by_type.get("place").unwrap_or(&0);
    let source_count1 = *report1.entities_created_by_type.get("source").unwrap_or(&0);

    assert!(person_count1 > 0, "DB1 must contain persons after import");
    assert!(place_count1 > 0, "DB1 must contain places after import");
    assert!(
        report1.assertions_created > 0,
        "DB1 must contain assertions after import"
    );

    // =========================================================================
    // Step 2: Query persons from DB1 via the Storage trait
    // =========================================================================
    // Open a second connection for read queries; backend1 takes ownership of conn1.
    let conn1_for_reads = Connection::open(&db1_path).expect("open DB1 for reads");
    let assertion_count1 = count_all_assertions(&conn1_for_reads);

    let backend1 = SqliteBackend::new(conn1);
    let persons_queried = backend1
        .list_persons(Pagination {
            limit: 1000,
            offset: 0,
        })
        .await
        .expect("list_persons from DB1 must succeed");

    assert_eq!(
        persons_queried.len(),
        person_count1,
        "Storage list_persons count must match the import report"
    );

    let families_queried = backend1
        .list_families(Pagination {
            limit: 1000,
            offset: 0,
        })
        .await
        .expect("list_families from DB1 must succeed");

    // Families are now stored in their own dedicated table.
    // We expect exactly family_count1 entries (no relationship rows mixed in).
    assert_eq!(
        families_queried.len(),
        family_count1,
        "Storage list_families must return only Family entities (no Relationships)"
    );

    // =========================================================================
    // Step 3: Show assertions for the first person (person detail)
    // =========================================================================
    let first_person = persons_queried
        .first()
        .expect("DB1 must have at least one person");

    let person_assertions = backend1
        .list_assertions_for_entity(first_person.id)
        .await
        .expect("list_assertions_for_entity must succeed for first person");

    assert!(
        !person_assertions.is_empty(),
        "first imported person must have at least one assertion"
    );

    // =========================================================================
    // Step 4: Export DB1 as GEDCOM
    // =========================================================================
    let persons_for_export = load_persons_from_snapshot(&conn1_for_reads);
    let families_for_export = load_families_from_snapshot(&conn1_for_reads);
    let places_for_export = load_places_from_snapshot(&conn1_for_reads);
    let events_for_export = load_events_from_snapshot(&conn1_for_reads);

    let mut export_nodes = Vec::new();
    let original_person_xrefs: std::collections::BTreeSet<String> = persons_for_export
        .iter()
        .filter_map(|person| person.original_xref.clone())
        .collect();
    let original_family_xrefs: std::collections::BTreeSet<String> = families_for_export
        .iter()
        .filter_map(|family| family.original_xref.clone())
        .collect();

    for (idx, person) in persons_for_export.iter().enumerate() {
        let xref = person
            .original_xref
            .clone()
            .unwrap_or_else(|| format!("@I{}@", idx + 1));
        if let Some(node) = person_to_indi_node_with_policy(
            person,
            &events_for_export,
            &places_for_export,
            &xref,
            ExportPrivacyPolicy::None,
        ) {
            export_nodes.push(node);
        }
    }
    for (idx, family) in families_for_export.iter().enumerate() {
        let xref = family
            .original_xref
            .clone()
            .unwrap_or_else(|| format!("@F{}@", idx + 1));
        export_nodes.push(family_to_fam_node(
            family,
            &events_for_export,
            &places_for_export,
            &xref,
        ));
    }
    // Also export sources (they survive round-trip via raw GEDCOM or structured fields)
    let sources_for_export: Vec<rustygene_core::evidence::Source> = {
        let mut stmt = conn1_for_reads
            .prepare("SELECT data FROM sources ORDER BY created_at")
            .expect("prepare sources");
        stmt.query_map([], |row| row.get::<_, String>(0))
            .expect("query sources")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect sources")
            .into_iter()
            .map(|raw| {
                serde_json::from_str::<rustygene_core::evidence::Source>(&raw)
                    .expect("parse source json")
            })
            .collect()
    };
    let original_source_xrefs: std::collections::BTreeSet<String> = sources_for_export
        .iter()
        .filter_map(|source| source.original_xref.clone())
        .collect();
    for (idx, source) in sources_for_export.iter().enumerate() {
        let xref = source
            .original_xref
            .clone()
            .unwrap_or_else(|| format!("@S{}@", idx + 1));
        export_nodes.push(source_to_sour_node(source, &xref));
    }

    let repositories_for_export: Vec<rustygene_core::evidence::Repository> = {
        let mut stmt = conn1_for_reads
            .prepare("SELECT data FROM repositories ORDER BY created_at")
            .expect("prepare repositories");
        stmt.query_map([], |row| row.get::<_, String>(0))
            .expect("query repositories")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect repositories")
            .into_iter()
            .map(|raw| {
                serde_json::from_str::<rustygene_core::evidence::Repository>(&raw)
                    .expect("parse repository json")
            })
            .collect()
    };

    for repository in &repositories_for_export {
        let xref = format!("@R{}@", repository.id.0.simple());
        export_nodes.push(repository_to_repo_node(repository, &xref));
    }

    let exported_gedcom = render_gedcom_file(&export_nodes);
    assert!(
        !exported_gedcom.is_empty(),
        "GEDCOM export must produce non-empty output"
    );
    assert!(
        exported_gedcom.contains("0 HEAD"),
        "GEDCOM export must include a HEAD record"
    );
    assert!(
        exported_gedcom.contains("0 TRLR"),
        "GEDCOM export must include a TRLR record"
    );
    assert!(
        persons_for_export
            .iter()
            .filter(|p| !p.names.is_empty())
            .count()
            > 0,
        "exported persons must include at least one named person"
    );

    // =========================================================================
    // Step 5: Re-import exported GEDCOM into DB2
    // =========================================================================
    let mut conn2 = setup_db(&db2_path);
    let report2 = import_gedcom_to_sqlite(&mut conn2, "e2e-gate-reimport-gedcom", &exported_gedcom)
        .expect("re-import of exported GEDCOM to DB2 must succeed");

    let person_count2 = *report2.entities_created_by_type.get("person").unwrap_or(&0);
    let family_count2 = *report2.entities_created_by_type.get("family").unwrap_or(&0);
    let place_count2 = *report2.entities_created_by_type.get("place").unwrap_or(&0);
    let event_count1 = *report1.entities_created_by_type.get("event").unwrap_or(&0);
    let event_count2 = *report2.entities_created_by_type.get("event").unwrap_or(&0);
    let conn2_for_reads = Connection::open(&db2_path).expect("open DB2 for reads");

    // =========================================================================
    // Step 6: Compare assertion graphs after GEDCOM round-trip
    // =========================================================================
    // GEDCOM round-trip preserves structural data: persons, names, families.
    // Events and some metadata are noted as known gaps in docs/GEDCOM_GAPS.md.
    assert_eq!(
        person_count2, person_count1,
        "GEDCOM round-trip must preserve the same number of persons \
         (original={person_count1}, reimport={person_count2})"
    );
    assert_eq!(
        family_count2, family_count1,
        "GEDCOM round-trip must preserve the same number of families \
         (original={family_count1}, reimport={family_count2})"
    );
    assert_eq!(
        place_count2, place_count1,
        "GEDCOM round-trip must preserve the same number of places \
         (original={place_count1}, reimport={place_count2})"
    );
    assert_eq!(
        event_count2, event_count1,
        "GEDCOM round-trip must preserve the same number of events \
         (original={event_count1}, reimport={event_count2})"
    );

    let dist1 = assertion_field_distribution_for_gedcom(&conn1_for_reads);
    let dist2 = assertion_field_distribution_for_gedcom(&conn2_for_reads);

    let required_fields = [
        ("person", "name"),
        ("person", "gender"),
        ("event", "event_type"),
        ("event", "date"),
        ("event", "place_ref"),
        ("event", "participant"),
        ("person", "event_participation"),
        ("family", "partner1_id"),
        ("family", "partner2_id"),
        ("family", "child_link"),
        ("place", "name"),
        ("family", "partner_link"),
        ("source", "title"),
        ("source", "author"),
    ];

    for required_field in required_fields {
        let key = (required_field.0.to_string(), required_field.1.to_string());
        let count1 = dist1.get(&key).copied().unwrap_or(0);
        let count2 = dist2.get(&key).copied().unwrap_or(0);
        assert_eq!(
            count1, count2,
            "GEDCOM round-trip assertion mismatch for required bucket ({}, {}): DB1={}, DB2={}",
            required_field.0, required_field.1, count1, count2
        );
    }

    // Compare full PersonName structs, not just given names.
    let names1: std::collections::BTreeSet<String> = persons_for_export
        .iter()
        .flat_map(|p| {
            p.names
                .iter()
                .map(|n| serde_json::to_string(n).expect("serialize person name"))
        })
        .collect();
    let persons2 = load_persons_from_snapshot(&conn2_for_reads);
    let names2: std::collections::BTreeSet<String> = persons2
        .iter()
        .flat_map(|p| {
            p.names
                .iter()
                .map(|n| serde_json::to_string(n).expect("serialize person name"))
        })
        .collect();
    assert_eq!(
        names1, names2,
        "GEDCOM round-trip must preserve full PersonName structs"
    );

    let person_xrefs2: std::collections::BTreeSet<String> = persons2
        .iter()
        .filter_map(|person| person.original_xref.clone())
        .collect();
    assert_eq!(
        person_xrefs2, original_person_xrefs,
        "GEDCOM round-trip must preserve original person xref IDs"
    );

    let families2 = load_families_from_snapshot(&conn2_for_reads);
    let family_structures1 = family_structure_distribution(&families_for_export);
    let family_structures2 = family_structure_distribution(&families2);
    assert_eq!(
        family_structures1, family_structures2,
        "GEDCOM round-trip must preserve family partner presence and child-link counts"
    );

    let family_xrefs2: std::collections::BTreeSet<String> = families2
        .iter()
        .filter_map(|family| family.original_xref.clone())
        .collect();
    assert_eq!(
        family_xrefs2, original_family_xrefs,
        "GEDCOM round-trip must preserve original family xref IDs"
    );

    let sources2: Vec<rustygene_core::evidence::Source> = {
        let mut stmt = conn2_for_reads
            .prepare("SELECT data FROM sources ORDER BY created_at")
            .expect("prepare DB2 sources");
        stmt.query_map([], |row| row.get::<_, String>(0))
            .expect("query DB2 sources")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect DB2 sources")
            .into_iter()
            .map(|raw| {
                serde_json::from_str::<rustygene_core::evidence::Source>(&raw)
                    .expect("parse DB2 source json")
            })
            .collect()
    };
    let source_xrefs2: std::collections::BTreeSet<String> = sources2
        .iter()
        .filter_map(|source| source.original_xref.clone())
        .collect();
    assert_eq!(
        source_xrefs2, original_source_xrefs,
        "GEDCOM round-trip must preserve original source xref IDs"
    );

    let events2 = load_events_from_snapshot(&conn2_for_reads);
    let places2 = load_places_from_snapshot(&conn2_for_reads);
    let place_names1: std::collections::BTreeSet<String> = places_for_export
        .iter()
        .flat_map(|place| place.names.iter().map(|name| name.name.clone()))
        .collect();
    let place_names2: std::collections::BTreeSet<String> = places2
        .iter()
        .flat_map(|place| place.names.iter().map(|name| name.name.clone()))
        .collect();
    assert_eq!(
        place_names1, place_names2,
        "GEDCOM round-trip must preserve place names"
    );
    let event_types1 = event_type_distribution(&events_for_export);
    let event_types2 = event_type_distribution(&events2);
    assert_eq!(
        event_types1, event_types2,
        "GEDCOM round-trip must preserve event type distribution"
    );

    let citation_count1: usize = conn1_for_reads
        .query_row("SELECT COUNT(*) FROM citations", [], |row| {
            row.get::<_, usize>(0)
        })
        .expect("count DB1 citations");
    let citation_count2: usize = conn2_for_reads
        .query_row("SELECT COUNT(*) FROM citations", [], |row| {
            row.get::<_, usize>(0)
        })
        .expect("count DB2 citations");
    assert_eq!(
        citation_count1, citation_count2,
        "GEDCOM round-trip must preserve citation count"
    );

    let repository_count1: usize = conn1_for_reads
        .query_row("SELECT COUNT(*) FROM repositories", [], |row| {
            row.get::<_, usize>(0)
        })
        .expect("count DB1 repositories");
    let repository_count2: usize = conn2_for_reads
        .query_row("SELECT COUNT(*) FROM repositories", [], |row| {
            row.get::<_, usize>(0)
        })
        .expect("count DB2 repositories");
    assert_eq!(
        repository_count1, repository_count2,
        "GEDCOM round-trip must preserve repository count"
    );

    // =========================================================================
    // Step 7: Export DB1 as JSON
    // =========================================================================
    let json_file_path = json_export_dir.join("rustygene_e2e_export.json");
    let json_export_result = backend1
        .export_json_dump(JsonExportMode::SingleFile {
            output_file: json_file_path.clone(),
        })
        .expect("JSON export of DB1 must succeed");

    assert!(
        json_export_result.output_path.exists(),
        "JSON export output path must exist on disk"
    );

    // =========================================================================
    // Step 8: Re-import JSON into DB3
    // =========================================================================
    let conn3 = setup_db(&db3_path);
    let backend3 = SqliteBackend::new(conn3);

    let json_import_report = backend3
        .import_json_dump(JsonImportMode::SingleFile {
            input_file: json_file_path.clone(),
        })
        .expect("JSON import into DB3 must succeed");

    assert!(
        json_import_report.assertions_imported > 0,
        "JSON import into DB3 must import assertions"
    );

    // =========================================================================
    // Step 9: Compare assertion graphs after JSON round-trip (must be lossless)
    // =========================================================================
    let conn3_for_reads = Connection::open(&db3_path).expect("open DB3 for reads");
    let assertion_count3 = count_all_assertions(&conn3_for_reads);
    let dist3 = assertion_field_distribution_for_json(&conn3_for_reads);

    assert_eq!(
        assertion_count3, assertion_count1,
        "JSON round-trip must preserve the exact assertion count \
         (original={assertion_count1}, json-reimport={assertion_count3})"
    );

    assert_eq!(
        dist1, dist3,
        "JSON round-trip must produce identical assertion field distributions"
    );

    let persons3 = backend3
        .list_persons(Pagination {
            limit: 1000,
            offset: 0,
        })
        .await
        .expect("list_persons from DB3 must succeed");

    assert_eq!(
        persons3.len(),
        person_count1,
        "JSON round-trip must preserve the same number of persons"
    );

    // Verify source count survives JSON round-trip.
    let sources3 = backend3
        .list_sources(Pagination {
            limit: 1000,
            offset: 0,
        })
        .await
        .expect("list_sources from DB3 must succeed");
    assert_eq!(
        sources3.len(),
        source_count1,
        "JSON round-trip must preserve the same number of sources"
    );

    let citations3: usize = conn3_for_reads
        .query_row("SELECT COUNT(*) FROM citations", [], |row| {
            row.get::<_, usize>(0)
        })
        .expect("count DB3 citations");
    assert_eq!(
        citations3, citation_count1,
        "JSON round-trip must preserve citation count"
    );

    let repositories3: usize = conn3_for_reads
        .query_row("SELECT COUNT(*) FROM repositories", [], |row| {
            row.get::<_, usize>(0)
        })
        .expect("count DB3 repositories");
    assert_eq!(
        repositories3, repository_count1,
        "JSON round-trip must preserve repository count"
    );

    // =========================================================================
    // Cleanup
    // =========================================================================
    drop(conn1_for_reads);
    drop(conn2_for_reads);
    drop(conn3_for_reads);
    let _ = std::fs::remove_file(&db1_path);
    let _ = std::fs::remove_file(&db2_path);
    let _ = std::fs::remove_file(&db3_path);
    let _ = std::fs::remove_dir_all(&json_export_dir);
}
