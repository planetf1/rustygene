use std::collections::BTreeMap;
use std::path::PathBuf;

use rusqlite::Connection;
use rustygene_core::event::Event;
use rustygene_core::evidence::{Media, Note, Repository, Source};
use rustygene_core::family::Family;
use rustygene_core::person::Person;
use rustygene_core::place::Place;
use rustygene_gedcom::{
    ExportPrivacyPolicy, GedcomNode, family_to_fam_node, import_gedcom_to_sqlite,
    media_to_obje_node, note_to_note_node, person_to_indi_node_with_policy, render_gedcom_file,
    repository_to_repo_node, source_to_sour_node,
};
use rustygene_storage::run_migrations;

fn temp_db_path(label: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "rustygene-asso-roundtrip-{}-{}-{}.sqlite",
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

fn load_raw_entities(conn: &Connection, table: &str) -> Vec<String> {
    let sql = format!("SELECT data FROM {table} ORDER BY created_at");
    let mut stmt = conn.prepare(&sql).expect("prepare query");
    stmt.query_map([], |row| row.get::<_, String>(0))
        .expect("query rows")
        .collect::<Result<Vec<_>, _>>()
        .expect("collect rows")
}

fn load_persons(conn: &Connection) -> Vec<Person> {
    load_raw_entities(conn, "persons")
        .into_iter()
        .map(|raw| serde_json::from_str::<Person>(&raw).expect("parse person json"))
        .collect()
}

fn load_events(conn: &Connection) -> Vec<Event> {
    load_raw_entities(conn, "events")
        .into_iter()
        .map(|raw| serde_json::from_str::<Event>(&raw).expect("parse event json"))
        .collect()
}

fn load_places(conn: &Connection) -> Vec<Place> {
    load_raw_entities(conn, "places")
        .into_iter()
        .map(|raw| serde_json::from_str::<Place>(&raw).expect("parse place json"))
        .collect()
}

fn load_sources(conn: &Connection) -> Vec<Source> {
    load_raw_entities(conn, "sources")
        .into_iter()
        .map(|raw| serde_json::from_str::<Source>(&raw).expect("parse source json"))
        .collect()
}

fn load_repositories(conn: &Connection) -> Vec<Repository> {
    load_raw_entities(conn, "repositories")
        .into_iter()
        .map(|raw| serde_json::from_str::<Repository>(&raw).expect("parse repository json"))
        .collect()
}

fn load_notes(conn: &Connection) -> Vec<Note> {
    load_raw_entities(conn, "notes")
        .into_iter()
        .map(|raw| serde_json::from_str::<Note>(&raw).expect("parse note json"))
        .collect()
}

fn load_media(conn: &Connection) -> Vec<Media> {
    load_raw_entities(conn, "media")
        .into_iter()
        .map(|raw| serde_json::from_str::<Media>(&raw).expect("parse media json"))
        .collect()
}

fn load_families(conn: &Connection) -> Vec<Family> {
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

fn rewrite_inline_source_references(
    node: &mut GedcomNode,
    anonymous_source_xrefs: &BTreeMap<String, String>,
) {
    if node.tag == "SOUR"
        && let Some(value) = &node.value
        && !value.starts_with('@')
        && let Some(rewritten) = anonymous_source_xrefs.get(value.trim())
    {
        node.value = Some(rewritten.clone());
    }

    for child in &mut node.children {
        rewrite_inline_source_references(child, anonymous_source_xrefs);
    }
}

fn export_db_as_gedcom(conn: &Connection) -> String {
    let persons: Vec<Person> = load_persons(conn);
    let families: Vec<Family> = load_families(conn);
    let events: Vec<Event> = load_events(conn);
    let places: Vec<Place> = load_places(conn);
    let sources: Vec<Source> = load_sources(conn);
    let repositories: Vec<Repository> = load_repositories(conn);
    let notes: Vec<Note> = load_notes(conn);
    let media: Vec<Media> = load_media(conn);

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
        nodes.push(family_to_fam_node(family, &persons, &events, &places, &xref));
    }

    let mut anonymous_source_xrefs: BTreeMap<String, String> = BTreeMap::new();
    for source in &sources {
        let xref = source.original_xref.clone().unwrap_or_else(|| {
            let generated = format!("@SX{}@", source.id.0.simple());
            anonymous_source_xrefs.insert(source.title.clone(), generated.clone());
            generated
        });
        nodes.push(source_to_sour_node(source, &xref));
    }

    for repository in &repositories {
        let xref = format!("@R{}@", repository.id.0.simple());
        nodes.push(repository_to_repo_node(repository, &xref));
    }

    for (idx, note) in notes.iter().enumerate() {
        let xref = note
            .original_xref
            .clone()
            .unwrap_or_else(|| format!("@N{}@", idx + 1));
        nodes.push(note_to_note_node(note, &xref));
    }

    for (idx, media_item) in media.iter().enumerate() {
        let xref = media_item
            .original_xref
            .clone()
            .unwrap_or_else(|| format!("@M{}@", idx + 1));
        nodes.push(media_to_obje_node(media_item, &xref));
    }

    for node in &mut nodes {
        rewrite_inline_source_references(node, &anonymous_source_xrefs);
    }

    render_gedcom_file(&nodes)
}

fn assertion_distribution(conn: &Connection) -> BTreeMap<(String, String), i64> {
    let mut stmt = conn
        .prepare(
            "SELECT entity_type, field, COUNT(*) \
             FROM assertions \
             WHERE status = 'confirmed' \
             GROUP BY entity_type, field \
             ORDER BY entity_type, field",
        )
        .expect("prepare assertion distribution query");

    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })
        .expect("query assertion distribution");

    let mut distribution = BTreeMap::new();
    for row in rows {
        let (entity_type, field, count) = row.expect("distribution row");
        distribution.insert((entity_type, field), count);
    }

    distribution
}

#[test]
fn asso_assoc_roundtrip_preserves_distribution_and_export_fidelity() {
    let input = r#"0 HEAD
1 SOUR TEST
1 GEDC
2 VERS 5.5.1
1 CHAR UTF-8
0 @S1@ SOUR
1 TITL Witness source
0 @N1@ NOTE Witness detail note
0 @I1@ INDI
1 NAME Principal /Person/
1 ASSO @I2@
2 RELA Witness
2 NOTE @N1@
2 NOTE Inline witness note
2 SOUR @S1@
1 ASSOC @I3@
2 RELA Neighbor
2 NOTE Inline neighbor note
0 @I2@ INDI
1 NAME Witness /Person/
0 @I3@ INDI
1 NAME Neighbor /Person/
0 TRLR
"#;

    let db1 = temp_db_path("db1");
    let db2 = temp_db_path("db2");

    let mut conn1 = setup_db(&db1);
    let report1 = import_gedcom_to_sqlite(&mut conn1, "asso-import-1", input)
        .expect("import synthetic ASSO fixture");

    assert!(
        !report1.unhandled_tags.contains_key("ASSO") && !report1.unhandled_tags.contains_key("ASSOC"),
        "ASSO/ASSOC must not be unhandled tags"
    );

    let association_count: i64 = conn1
        .query_row(
            "SELECT COUNT(*) FROM assertions WHERE field = 'association' AND status = 'confirmed'",
            [],
            |row| row.get(0),
        )
        .expect("count association assertions");
    assert!(association_count >= 2, "expected typed association assertions");

    let dist_before = assertion_distribution(&conn1);

    let exported = export_db_as_gedcom(&conn1);
    assert!(exported.contains("1 ASSO @I2@"));
    assert!(exported.contains("2 RELA Witness"));
    assert!(exported.contains("2 SOUR @S1@"));
    assert!(exported.contains("2 NOTE @N1@"));
    assert!(exported.contains("1 ASSOC @I3@"));

    let mut conn2 = setup_db(&db2);
    import_gedcom_to_sqlite(&mut conn2, "asso-import-2", &exported)
        .expect("re-import exported ASSO fixture");

    let dist_after = assertion_distribution(&conn2);
    assert_eq!(
        dist_after, dist_before,
        "ASSO/ASSOC round-trip must preserve assertion distribution"
    );

    let _ = std::fs::remove_file(&db1);
    let _ = std::fs::remove_file(&db2);
}
