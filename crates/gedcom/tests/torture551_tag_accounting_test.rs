use std::collections::BTreeMap;
use std::path::PathBuf;

use rusqlite::Connection;
use rustygene_gedcom::import_gedcom_to_sqlite;
use rustygene_storage::run_migrations;

fn temp_db_path(label: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "rustygene-torture551-tags-{}-{}-{}.sqlite",
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

fn read_torture_fixture() -> String {
    let path = PathBuf::from("../../testdata/gedcom/torture551.ged");
    match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::InvalidData => {
            let bytes = std::fs::read(&path).expect("read torture551 bytes");
            bytes.iter().map(|&b| b as char).collect()
        }
        Err(e) => panic!("failed to read torture551.ged: {e}"),
    }
}

#[test]
fn torture551_has_zero_unhandled_standard_tags() {
    let db_path = temp_db_path("import");
    let mut conn = setup_db(&db_path);

    let input = read_torture_fixture();
    let report = import_gedcom_to_sqlite(&mut conn, "torture551-accounting", &input)
        .expect("import torture551.ged");

    let unhandled_standard_tags: BTreeMap<String, usize> = report
        .unhandled_tags
        .iter()
        .filter(|(tag, _)| !tag.starts_with('_'))
        .map(|(tag, count)| (tag.clone(), *count))
        .collect();

    assert!(
        unhandled_standard_tags.is_empty(),
        "found unhandled standard GEDCOM tags: {unhandled_standard_tags:?}"
    );

    assert!(
        !report.deferred_standard_tags.is_empty(),
        "expected deferred standard tag counters to be populated for torture551"
    );

    let _ = std::fs::remove_file(&db_path);
}
