use std::path::PathBuf;

use rusqlite::Connection;
use rustygene_core::event::Event;
use rustygene_core::evidence::{Citation, Repository, Source};
use rustygene_core::family::Family;
use rustygene_core::person::Person;
use rustygene_core::place::Place;
use rustygene_gedcom::{
    ExportPrivacyPolicy, family_to_fam_node, import_gedcom_to_sqlite,
    person_to_indi_node_with_policy, render_gedcom_file, repository_to_repo_node,
    source_to_sour_node,
};
use rustygene_storage::run_migrations;

fn temp_db_path(label: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "rustygene-citation-roundtrip-{}-{}-{}.sqlite",
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

fn load_persons(conn: &Connection) -> Vec<Person> {
    let mut stmt = conn
        .prepare("SELECT data FROM persons ORDER BY created_at")
        .expect("prepare persons query");
    stmt.query_map([], |row| row.get::<_, String>(0))
        .expect("query persons")
        .collect::<Result<Vec<_>, _>>()
        .expect("collect persons")
        .into_iter()
        .map(|raw| serde_json::from_str::<Person>(&raw).expect("parse person json"))
        .collect()
}

fn load_events(conn: &Connection) -> Vec<Event> {
    let mut stmt = conn
        .prepare("SELECT data FROM events ORDER BY created_at")
        .expect("prepare events query");
    stmt.query_map([], |row| row.get::<_, String>(0))
        .expect("query events")
        .collect::<Result<Vec<_>, _>>()
        .expect("collect events")
        .into_iter()
        .map(|raw| serde_json::from_str::<Event>(&raw).expect("parse event json"))
        .collect()
}

fn load_places(conn: &Connection) -> Vec<Place> {
    let mut stmt = conn
        .prepare("SELECT data FROM places ORDER BY created_at")
        .expect("prepare places query");
    stmt.query_map([], |row| row.get::<_, String>(0))
        .expect("query places")
        .collect::<Result<Vec<_>, _>>()
        .expect("collect places")
        .into_iter()
        .map(|raw| serde_json::from_str::<Place>(&raw).expect("parse place json"))
        .collect()
}

fn load_sources(conn: &Connection) -> Vec<Source> {
    let mut stmt = conn
        .prepare("SELECT data FROM sources ORDER BY created_at")
        .expect("prepare sources query");
    stmt.query_map([], |row| row.get::<_, String>(0))
        .expect("query sources")
        .collect::<Result<Vec<_>, _>>()
        .expect("collect sources")
        .into_iter()
        .map(|raw| serde_json::from_str::<Source>(&raw).expect("parse source json"))
        .collect()
}

fn load_repositories(conn: &Connection) -> Vec<Repository> {
    let mut stmt = conn
        .prepare("SELECT data FROM repositories ORDER BY created_at")
        .expect("prepare repositories query");
    stmt.query_map([], |row| row.get::<_, String>(0))
        .expect("query repositories")
        .collect::<Result<Vec<_>, _>>()
        .expect("collect repositories")
        .into_iter()
        .map(|raw| serde_json::from_str::<Repository>(&raw).expect("parse repository json"))
        .collect()
}

fn load_citations(conn: &Connection) -> Vec<Citation> {
    let mut stmt = conn
        .prepare("SELECT data FROM citations ORDER BY created_at")
        .expect("prepare citations query");
    stmt.query_map([], |row| row.get::<_, String>(0))
        .expect("query citations")
        .collect::<Result<Vec<_>, _>>()
        .expect("collect citations")
        .into_iter()
        .map(|raw| serde_json::from_str::<Citation>(&raw).expect("parse citation json"))
        .collect()
}

fn count_rows(conn: &Connection, table: &str) -> usize {
    let sql = format!("SELECT COUNT(*) FROM {table}");
    conn.query_row(&sql, [], |row| row.get::<_, usize>(0))
        .expect("count rows")
}

fn count_assertions_with_citation_refs(conn: &Connection) -> usize {
    conn.query_row(
        "SELECT COUNT(*) FROM assertions WHERE json_array_length(source_citations) > 0",
        [],
        |row| row.get::<_, usize>(0),
    )
    .expect("count assertions with source_citations")
}

fn export_db_as_gedcom(conn: &Connection) -> String {
    let persons: Vec<Person> = load_persons(conn);
    let families: Vec<Family> = {
        let mut stmt = conn
            .prepare(
                "SELECT data FROM families \
                 WHERE json_extract(data, '$.relationship_type') IS NULL \
                 ORDER BY created_at",
            )
            .expect("prepare families query");
        stmt.query_map([], |row| row.get::<_, String>(0))
            .expect("query families")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect families")
            .into_iter()
            .map(|raw| serde_json::from_str::<Family>(&raw).expect("parse family json"))
            .collect()
    };
    let events: Vec<Event> = load_events(conn);
    let places: Vec<Place> = load_places(conn);
    let sources: Vec<Source> = load_sources(conn);
    let repositories: Vec<Repository> = load_repositories(conn);

    let mut nodes = Vec::new();

    for (idx, person) in persons.iter().enumerate() {
        let xref = person
            .original_xref
            .clone()
            .unwrap_or_else(|| format!("@I{}@", idx + 1));
        if let Some(node) = person_to_indi_node_with_policy(
            person,
            &events,
            &places,
            &xref,
            ExportPrivacyPolicy::None,
        ) {
            nodes.push(node);
        }
    }

    for (idx, family) in families.iter().enumerate() {
        let xref = family
            .original_xref
            .clone()
            .unwrap_or_else(|| format!("@F{}@", idx + 1));
        nodes.push(family_to_fam_node(
            family, &persons, &events, &places, &xref,
        ));
    }

    for (idx, source) in sources.iter().enumerate() {
        let xref = source
            .original_xref
            .clone()
            .unwrap_or_else(|| format!("@S{}@", idx + 1));
        nodes.push(source_to_sour_node(source, &xref));
    }

    for repository in &repositories {
        let xref = format!("@R{}@", repository.id.0.simple());
        nodes.push(repository_to_repo_node(repository, &xref));
    }

    render_gedcom_file(&nodes)
}

#[test]
fn synthetic_inline_citation_roundtrip_preserves_citations() {
    let input = "0 HEAD\n1 SOUR TEST\n1 GEDC\n2 VERS 5.5.1\n2 FORM LINEAGE-LINKED\n1 CHAR UTF-8\n0 @S1@ SOUR\n1 TITL Test Source\n0 @I1@ INDI\n1 NAME John /Smith/\n1 BIRT\n2 DATE 1 JAN 1900\n2 SOUR @S1@\n3 PAGE 42\n3 QUAY 2\n3 DATA\n4 TEXT Household entry\n0 TRLR\n";

    let db1_path = temp_db_path("db1");
    let db2_path = temp_db_path("db2");

    let mut conn1 = setup_db(&db1_path);
    let import_report = import_gedcom_to_sqlite(&mut conn1, "citation-roundtrip-import-1", input)
        .expect("import synthetic GEDCOM");

    assert_eq!(
        import_report.entities_created_by_type.get("source"),
        Some(&1)
    );

    let citation_count1 = count_rows(&conn1, "citations");
    assert!(
        citation_count1 > 0,
        "inline SOUR must create Citation entities"
    );

    let citation_linked_assertions1 = count_assertions_with_citation_refs(&conn1);
    assert!(
        citation_linked_assertions1 > 0,
        "inline SOUR must produce assertion linkages with source_citations"
    );

    let citations1: Vec<Citation> = load_citations(&conn1);
    assert!(
        citations1
            .iter()
            .any(|citation| citation.page.as_deref() == Some("42")),
        "PAGE sub-node must map to Citation.page"
    );
    assert!(
        citations1
            .iter()
            .any(|citation| citation.confidence_level == Some(2)),
        "QUAY sub-node must map to Citation.confidence_level"
    );
    assert!(
        citations1
            .iter()
            .any(|citation| citation.transcription.as_deref() == Some("Household entry")),
        "DATA/TEXT sub-nodes must map to Citation.transcription"
    );

    let exported = export_db_as_gedcom(&conn1);
    assert!(
        exported.contains("2 SOUR @S1@"),
        "GEDCOM export must emit inline SOUR refs on event nodes"
    );
    assert!(
        exported.contains("3 PAGE 42"),
        "GEDCOM export must emit citation PAGE sub-node"
    );
    assert!(
        exported.contains("3 QUAY 2"),
        "GEDCOM export must emit citation QUAY sub-node"
    );

    let mut conn2 = setup_db(&db2_path);
    import_gedcom_to_sqlite(&mut conn2, "citation-roundtrip-import-2", &exported)
        .expect("re-import exported GEDCOM");

    let citation_count2 = count_rows(&conn2, "citations");
    assert_eq!(
        citation_count1, citation_count2,
        "citation count must match after synthetic GEDCOM round-trip"
    );

    let citation_linked_assertions2 = count_assertions_with_citation_refs(&conn2);
    assert!(citation_linked_assertions2 > 0);

    let _ = std::fs::remove_file(&db1_path);
    let _ = std::fs::remove_file(&db2_path);
}
