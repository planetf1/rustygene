use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn top_level_help_lists_navigation_commands() {
    let mut cmd = Command::cargo_bin("rustygene-cli").expect("binary exists");
    cmd.arg("--help");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("RustyGene CLI"))
        .stdout(predicate::str::contains("import"))
        .stdout(predicate::str::contains("export"))
        .stdout(predicate::str::contains("query"))
        .stdout(predicate::str::contains("show"))
        .stdout(predicate::str::contains("--db <DB>"))
        .stdout(predicate::str::contains("--output-format <FORMAT>"));
}

#[test]
fn query_person_help_contains_examples_for_humans_and_agents() {
    let mut cmd = Command::cargo_bin("rustygene-cli").expect("binary exists");
    cmd.args(["query", "person", "--help"]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Examples:"))
        .stdout(predicate::str::contains("query person Jones"))
        .stdout(predicate::str::contains("--fuzzy"))
        .stdout(predicate::str::contains("--birth-year-from"));
}

#[test]
fn invalid_subcommand_returns_hintful_error() {
    let mut cmd = Command::cargo_bin("rustygene-cli").expect("binary exists");
    cmd.arg("qurey");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("unrecognized subcommand"))
        .stderr(predicate::str::contains("query"));
}

#[test]
fn show_person_invalid_uuid_is_human_readable() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let db_path = temp_dir.path().join("cli-show-person-invalid-id.sqlite");

    let mut cmd = Command::cargo_bin("rustygene-cli").expect("binary exists");
    cmd.args([
        "--db",
        db_path.to_string_lossy().as_ref(),
        "show",
        "person",
        "not-a-uuid",
    ]);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("invalid person id"));
}

#[test]
fn import_without_required_format_shows_actionable_usage() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let db_path = temp_dir.path().join("cli-import-missing-format.sqlite");

    let mut cmd = Command::cargo_bin("rustygene-cli").expect("binary exists");
    cmd.args([
        "--db",
        db_path.to_string_lossy().as_ref(),
        "import",
        "testdata/gedcom/kennedy.ged",
    ]);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("--format <IMPORT_FORMAT>"))
        .stderr(predicate::str::contains("Usage:"));
}
