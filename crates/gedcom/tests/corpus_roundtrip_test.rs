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
        "rustygene-corpus-roundtrip-{}-{}-{}.sqlite",
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

fn read_gedcom_fixture(file_name: &str) -> String {
    let path = PathBuf::from(format!("../../testdata/gedcom/{file_name}"));
    match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::InvalidData => {
            let bytes = std::fs::read(&path).expect("read GEDCOM bytes");
            bytes.iter().map(|&b| b as char).collect()
        }
        Err(e) => panic!("failed to read fixture {file_name}: {e}"),
    }
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

    // Only export sources that have an original xref.  Inline/anonymous sources
    // (created from verbatim SOUR text via resolve_source_id) are preserved in
    // the owning entity's _raw_gedcom as CUSTOM_SOUR_N entries; re-exporting them
    // as root SOUR records would produce a duplicate anonymous Source on re-import.
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

    if !anonymous_source_xrefs.is_empty() {
        for node in &mut nodes {
            rewrite_inline_source_references(node, &anonymous_source_xrefs);
        }
    }

    render_gedcom_file(&nodes)
}

fn count_table_rows(conn: &Connection, table: &str) -> usize {
    let sql = format!("SELECT COUNT(*) FROM {table}");
    conn.query_row(&sql, [], |row| row.get::<_, usize>(0))
        .expect("count rows")
}

fn assertion_distribution(conn: &Connection) -> BTreeMap<(String, String), usize> {
    let mut stmt = conn
        .prepare(
            "SELECT entity_type, field, COUNT(*) as cnt \
             FROM assertions \
             GROUP BY entity_type, field \
             ORDER BY entity_type, field",
        )
        .expect("prepare assertion distribution query");

    let rows = stmt
        .query_map([], |row| {
            Ok((
                (row.get::<_, String>(0)?, row.get::<_, String>(1)?),
                row.get::<_, usize>(2)?,
            ))
        })
        .expect("query assertion distribution");

    rows.into_iter()
        .map(|row| row.expect("distribution row"))
        .collect()
}

#[test]
fn corpus_roundtrip_hardening_for_seven_vendor_fixtures() {
    let corpus = [
        ("ancestry_sample.ged", "Ancestry"),
        ("rootsmagic_sample.ged", "RootsMagic"),
        ("gramps_sample.ged", "Gramps"),
        ("legacy_sample.ged", "Legacy"),
        ("paf_sample.ged", "PAF"),
        ("simpsons.ged", "Simpsons"),
        ("vendor_metadata_sample.ged", "VendorMeta"),
    ];

    let mut aggregate_deferred: BTreeMap<String, usize> = BTreeMap::new();

    for (file_name, vendor) in corpus {
        let input = read_gedcom_fixture(file_name);
        let db1 = temp_db_path(&format!("{vendor}-db1"));
        let db2 = temp_db_path(&format!("{vendor}-db2"));

        let mut conn1 = setup_db(&db1);
        let report1 =
            import_gedcom_to_sqlite(&mut conn1, &format!("corpus-import-{vendor}"), &input)
                .expect("import corpus fixture");

        let unhandled_standard_tags: BTreeMap<String, usize> = report1
            .unhandled_tags
            .iter()
            .filter(|(tag, _)| !tag.starts_with('_'))
            .map(|(tag, count)| (tag.clone(), *count))
            .collect();

        assert!(
            unhandled_standard_tags.is_empty(),
            "{vendor}: found unhandled standard GEDCOM tags: {unhandled_standard_tags:?}"
        );

        for (tag, count) in &report1.deferred_standard_tags {
            *aggregate_deferred.entry(tag.clone()).or_insert(0) += count;
        }

        let table_counts_before = [
            ("persons", count_table_rows(&conn1, "persons")),
            ("families", count_table_rows(&conn1, "families")),
            ("events", count_table_rows(&conn1, "events")),
            ("places", count_table_rows(&conn1, "places")),
            ("sources", count_table_rows(&conn1, "sources")),
            ("citations", count_table_rows(&conn1, "citations")),
            ("repositories", count_table_rows(&conn1, "repositories")),
            ("notes", count_table_rows(&conn1, "notes")),
            ("media", count_table_rows(&conn1, "media")),
        ];
        let dist_before = assertion_distribution(&conn1);

        if vendor == "VendorMeta" {
            let vendor_metadata_count = dist_before
                .get(&("person".to_string(), "vendor_metadata".to_string()))
                .copied()
                .unwrap_or(0);
            assert!(
                vendor_metadata_count > 0,
                "{vendor}: expected typed person.vendor_metadata assertions"
            );
        }

        let exported = export_db_as_gedcom(&conn1);
        assert!(exported.contains("0 HEAD"));
        assert!(exported.contains("0 TRLR"));
        if vendor == "VendorMeta" {
            assert!(
                exported.contains("1 _MSER ancestry-series-42"),
                "{vendor}: expected _MSER to round-trip in export"
            );
            assert!(
                exported.contains("1 _OID ancestry-oid-123"),
                "{vendor}: expected _OID to round-trip in export"
            );
            assert!(
                exported.contains("2 _CROP 10,20,100,200"),
                "{vendor}: expected nested _CROP to round-trip in export"
            );
        }

        let mut conn2 = setup_db(&db2);
        import_gedcom_to_sqlite(&mut conn2, &format!("corpus-reimport-{vendor}"), &exported)
            .expect("re-import exported fixture");

        for (table, expected_count) in table_counts_before {
            let actual_count = count_table_rows(&conn2, table);
            assert_eq!(
                actual_count, expected_count,
                "{vendor}: row count mismatch for table {table}"
            );
        }

        let dist_after = assertion_distribution(&conn2);
        assert_eq!(
            dist_after, dist_before,
            "{vendor}: assertion distribution mismatch after round-trip"
        );

        let _ = std::fs::remove_file(&db1);
        let _ = std::fs::remove_file(&db2);
    }

    for required_tag in ["ASSO", "CHAN", "DATA"] {
        let count = aggregate_deferred.get(required_tag).copied().unwrap_or(0);
        assert!(
            count > 0,
            "expected deferred standard-tag counter for {required_tag} to be > 0 across corpus"
        );
    }
}

#[test]
fn corpus_roundtrip_simpsons_ged_diagnostic() {
    let input = read_gedcom_fixture("simpsons.ged");
    let db1 = temp_db_path("simpsons-db1");
    let db2 = temp_db_path("simpsons-db2");

    let mut conn1 = setup_db(&db1);
    let _report1 = import_gedcom_to_sqlite(&mut conn1, "corpus-import-simpsons", &input)
        .expect("import simpsons fixture");

    let table_counts_before = [
        ("persons", count_table_rows(&conn1, "persons")),
        ("families", count_table_rows(&conn1, "families")),
        ("events", count_table_rows(&conn1, "events")),
        ("places", count_table_rows(&conn1, "places")),
        ("sources", count_table_rows(&conn1, "sources")),
        ("citations", count_table_rows(&conn1, "citations")),
        ("repositories", count_table_rows(&conn1, "repositories")),
        ("notes", count_table_rows(&conn1, "notes")),
        ("media", count_table_rows(&conn1, "media")),
    ];
    let dist_before = assertion_distribution(&conn1);

    let exported = export_db_as_gedcom(&conn1);
    assert!(exported.contains("0 HEAD"));
    assert!(exported.contains("0 TRLR"));

    let mut conn2 = setup_db(&db2);
    import_gedcom_to_sqlite(&mut conn2, "corpus-reimport-simpsons", &exported)
        .expect("re-import exported simpsons");

    for (table, expected_count) in table_counts_before {
        let actual_count = count_table_rows(&conn2, table);
        assert_eq!(
            actual_count, expected_count,
            "Simpsons: row count mismatch for table {table}: expected {expected_count}, got {actual_count}"
        );
    }

    let dist_after = assertion_distribution(&conn2);
    assert_eq!(
        dist_after, dist_before,
        "Simpsons: assertion distribution mismatch after round-trip"
    );

    let _ = std::fs::remove_file(&db1);
    let _ = std::fs::remove_file(&db2);
}

#[test]
fn corpus_roundtrip_torture551_event_count_regression() {
    let input = read_gedcom_fixture("torture551.ged");
    let db1 = temp_db_path("torture551-db1");
    let db2 = temp_db_path("torture551-db2");

    let mut conn1 = setup_db(&db1);
    let _report1 = import_gedcom_to_sqlite(&mut conn1, "corpus-import-torture551", &input)
        .expect("import torture551 fixture");

    let expected_event_count = count_table_rows(&conn1, "events");

    let exported = export_db_as_gedcom(&conn1);
    assert!(exported.contains("0 HEAD"));
    assert!(exported.contains("0 TRLR"));

    let mut conn2 = setup_db(&db2);
    import_gedcom_to_sqlite(&mut conn2, "corpus-reimport-torture551", &exported)
        .expect("re-import exported torture551");

    let actual_event_count = count_table_rows(&conn2, "events");
    assert_eq!(
        actual_event_count, expected_event_count,
        "Torture551: row count mismatch for table events: expected {expected_event_count}, got {actual_event_count}"
    );

    let _ = std::fs::remove_file(&db1);
    let _ = std::fs::remove_file(&db2);
}

#[test]
fn corpus_roundtrip_torture551_ged_diagnostic() {
    let input = read_gedcom_fixture("torture551.ged");
    let db1 = temp_db_path("torture551-db1");
    let db2 = temp_db_path("torture551-db2");

    let mut conn1 = setup_db(&db1);
    let _report1 = import_gedcom_to_sqlite(&mut conn1, "corpus-import-torture551", &input)
        .expect("import torture551 fixture");

    let table_counts_before = [
        ("persons", count_table_rows(&conn1, "persons")),
        ("families", count_table_rows(&conn1, "families")),
        ("events", count_table_rows(&conn1, "events")),
        ("places", count_table_rows(&conn1, "places")),
        ("sources", count_table_rows(&conn1, "sources")),
        ("citations", count_table_rows(&conn1, "citations")),
        ("repositories", count_table_rows(&conn1, "repositories")),
        ("notes", count_table_rows(&conn1, "notes")),
        ("media", count_table_rows(&conn1, "media")),
    ];
    let dist_before = assertion_distribution(&conn1);

    let exported = export_db_as_gedcom(&conn1);
    assert!(exported.contains("0 HEAD"));
    assert!(exported.contains("0 TRLR"));


    let mut conn2 = setup_db(&db2);
    import_gedcom_to_sqlite(&mut conn2, "corpus-reimport-torture551", &exported)
        .expect("re-import exported torture551");

    for (table, expected_count) in table_counts_before {
        let actual_count = count_table_rows(&conn2, table);
        assert_eq!(
            actual_count, expected_count,
            "Torture551: row count mismatch for table {table}: expected {expected_count}, got {actual_count}"
        );
    }

    let dist_after = assertion_distribution(&conn2);
    assert_eq!(
        dist_after, dist_before,
        "Torture551: assertion distribution mismatch after round-trip"
    );

    let _ = std::fs::remove_file(&db1);
    let _ = std::fs::remove_file(&db2);
}
