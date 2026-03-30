use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};
use rusqlite::Connection;
use rustygene_core::event::Event;
use rustygene_core::family::Family;
use rustygene_core::person::Person;
use rustygene_core::research::{ResearchLogEntry, SearchResult};
use rustygene_core::types::EntityId;
use rustygene_storage::{
    Pagination, ResearchLogFilter, Storage, run_migrations, sqlite_impl::SqliteBackend,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum OutputFormat {
    Json,
    Text,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum CliSearchResult {
    Found,
    NotFound,
    PartiallyFound,
    Inconclusive,
}

impl From<CliSearchResult> for SearchResult {
    fn from(value: CliSearchResult) -> Self {
        match value {
            CliSearchResult::Found => SearchResult::Found,
            CliSearchResult::NotFound => SearchResult::NotFound,
            CliSearchResult::PartiallyFound => SearchResult::PartiallyFound,
            CliSearchResult::Inconclusive => SearchResult::Inconclusive,
        }
    }
}

#[derive(Debug, Parser)]
#[command(name = "rustygene", version, about = "RustyGene CLI")]
struct Cli {
    /// Database location
    #[arg(long, global = true, default_value = "~/.rustygene/rustygene.db")]
    db: PathBuf,

    /// Output format
    #[arg(long, global = true, value_enum, default_value_t = OutputFormat::Text)]
    format: OutputFormat,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Import,
    Export,
    Query {
        #[command(subcommand)]
        command: QueryCommands,
    },
    Show {
        #[command(subcommand)]
        command: ShowCommands,
    },
    ResearchLog {
        #[command(subcommand)]
        command: ResearchLogCommands,
    },
    RebuildSnapshots,
}

#[derive(Debug, Subcommand)]
enum QueryCommands {
    Person {
        #[arg(long)]
        name: String,
    },
}

#[derive(Debug, Subcommand)]
enum ShowCommands {
    Person { id: String },
    Family { id: String },
    Event { id: String },
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
struct QueryPersonRow {
    id: String,
    preferred_name: Option<String>,
    birth_date: Option<String>,
    death_date: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
struct AssertionView {
    value: String,
    status: String,
    preferred: bool,
    confidence: String,
}

#[derive(Debug, Clone, serde::Serialize)]
struct ShowPersonOutput {
    person: Person,
    assertions_by_field: BTreeMap<String, Vec<AssertionView>>,
    linked_event_ids: Vec<String>,
    linked_family_ids: Vec<String>,
    linked_source_ids: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
struct ShowFamilyOutput {
    family: Family,
    linked_event_ids: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
struct ShowEventOutput {
    event: Event,
}

#[derive(Debug, Subcommand)]
enum ResearchLogCommands {
    Add {
        #[arg(long)]
        objective: String,
        #[arg(long, value_enum)]
        result: CliSearchResult,
        #[arg(long)]
        person: Option<String>,
        #[arg(long)]
        repository: Option<String>,
    },
    List {
        #[arg(long)]
        person: Option<String>,
        #[arg(long, value_enum)]
        result: Option<CliSearchResult>,
    },
}

fn main() {
    let cli = Cli::parse();
    let db_path = resolve_db_path(&cli.db);
    if let Some(parent) = db_path.parent()
        && let Err(err) = std::fs::create_dir_all(parent)
    {
        eprintln!(
            "failed to create database directory '{}': {}",
            parent.display(),
            err
        );
        std::process::exit(1);
    }
    let mut connection = match Connection::open(&db_path) {
        Ok(conn) => conn,
        Err(err) => {
            eprintln!("failed to open database '{}': {}", db_path.display(), err);
            std::process::exit(1);
        }
    };

    if let Err(err) = run_migrations(&mut connection) {
        eprintln!("failed to run migrations: {}", err);
        std::process::exit(1);
    }

    let backend = SqliteBackend::new(connection);

    match cli.command {
        Commands::RebuildSnapshots => {
            match backend.rebuild_all_snapshots() {
                Ok(rebuilt_count) => match cli.format {
                    OutputFormat::Text => {
                        println!("rebuild-snapshots complete: {} entity snapshots rebuilt", rebuilt_count);
                    }
                    OutputFormat::Json => {
                        println!("{{\"rebuilt\":{}}}", rebuilt_count);
                    }
                },
                Err(err) => {
                    eprintln!("failed to rebuild snapshots: {}", err.message);
                    std::process::exit(1);
                }
            }
        }
        Commands::ResearchLog { command } => {
            run_research_log_command(command, &backend, cli.format);
        }
        Commands::Query { command } => {
            run_query_command(command, &db_path, cli.format);
        }
        Commands::Show { command } => {
            run_show_command(command, &db_path, cli.format);
        }
        Commands::Import
        | Commands::Export => {
            eprintln!("command not implemented yet");
            std::process::exit(2);
        }
    }
}

fn run_show_command(command: ShowCommands, db_path: &PathBuf, format: OutputFormat) {
    let conn = match Connection::open(db_path) {
        Ok(conn) => conn,
        Err(err) => {
            eprintln!("failed to open database '{}': {}", db_path.display(), err);
            std::process::exit(1);
        }
    };

    match command {
        ShowCommands::Person { id } => {
            let id = match parse_entity_id_arg(&id) {
                Ok(id) => id,
                Err(err) => {
                    eprintln!("invalid person id: {}", err);
                    std::process::exit(1);
                }
            };
            let output = match build_show_person_output(&conn, id) {
                Ok(v) => v,
                Err(err) => {
                    eprintln!("failed to show person: {}", err);
                    std::process::exit(1);
                }
            };

            match format {
                OutputFormat::Json => match serde_json::to_string(&output) {
                    Ok(json) => println!("{}", json),
                    Err(err) => {
                        eprintln!("failed to serialize show person output: {}", err);
                        std::process::exit(1);
                    }
                },
                OutputFormat::Text => {
                    println!("person: {}", output.person.id);
                    println!("assertions:");
                    for (field, assertions) in output.assertions_by_field {
                        println!("  {}:", field);
                        for a in assertions {
                            println!(
                                "    value={} status={} preferred={} confidence={}",
                                a.value, a.status, a.preferred, a.confidence
                            );
                        }
                    }
                    println!("linked events: {}", output.linked_event_ids.join(","));
                    println!("linked families: {}", output.linked_family_ids.join(","));
                    println!("linked sources: {}", output.linked_source_ids.join(","));
                }
            }
        }
        ShowCommands::Family { id } => {
            let id = match parse_entity_id_arg(&id) {
                Ok(id) => id,
                Err(err) => {
                    eprintln!("invalid family id: {}", err);
                    std::process::exit(1);
                }
            };
            let output = match build_show_family_output(&conn, id) {
                Ok(v) => v,
                Err(err) => {
                    eprintln!("failed to show family: {}", err);
                    std::process::exit(1);
                }
            };

            match format {
                OutputFormat::Json => match serde_json::to_string(&output) {
                    Ok(json) => println!("{}", json),
                    Err(err) => {
                        eprintln!("failed to serialize show family output: {}", err);
                        std::process::exit(1);
                    }
                },
                OutputFormat::Text => {
                    println!("family: {}", output.family.id);
                    println!("partner1: {:?}", output.family.partner1_id);
                    println!("partner2: {:?}", output.family.partner2_id);
                    println!("children: {}", output.family.child_links.len());
                    println!("linked events: {}", output.linked_event_ids.join(","));
                }
            }
        }
        ShowCommands::Event { id } => {
            let id = match parse_entity_id_arg(&id) {
                Ok(id) => id,
                Err(err) => {
                    eprintln!("invalid event id: {}", err);
                    std::process::exit(1);
                }
            };
            let output = match build_show_event_output(&conn, id) {
                Ok(v) => v,
                Err(err) => {
                    eprintln!("failed to show event: {}", err);
                    std::process::exit(1);
                }
            };

            match format {
                OutputFormat::Json => match serde_json::to_string(&output) {
                    Ok(json) => println!("{}", json),
                    Err(err) => {
                        eprintln!("failed to serialize show event output: {}", err);
                        std::process::exit(1);
                    }
                },
                OutputFormat::Text => {
                    println!("event: {}", output.event.id);
                    println!("type: {:?}", output.event.event_type);
                    println!("participants: {}", output.event.participants.len());
                }
            }
        }
    }
}

fn build_show_person_output(conn: &Connection, id: EntityId) -> Result<ShowPersonOutput, String> {
    let person_json: String = conn
        .query_row(
            "SELECT data FROM persons WHERE id = ?",
            rusqlite::params![id.to_string()],
            |row| row.get(0),
        )
        .map_err(|e| format!("person not found or unreadable: {}", e))?;
    let person: Person = serde_json::from_str(&person_json)
        .map_err(|e| format!("failed to parse person JSON: {}", e))?;

    let mut assertion_stmt = conn
        .prepare(
            "SELECT field, value, status, preferred, confidence
             FROM assertions
             WHERE entity_id = ?
             ORDER BY field ASC, preferred DESC, confidence DESC",
        )
        .map_err(|e| format!("failed to prepare assertion query: {}", e))?;

    let assertion_rows = assertion_stmt
        .query_map(rusqlite::params![id.to_string()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, f64>(4)?,
            ))
        })
        .map_err(|e| format!("failed to query assertions: {}", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("failed to collect assertions: {}", e))?;

    let mut assertions_by_field: BTreeMap<String, Vec<AssertionView>> = BTreeMap::new();
    for (field, value_raw, status, preferred, confidence) in assertion_rows {
        let value = serde_json::from_str::<serde_json::Value>(&value_raw)
            .ok()
            .and_then(|v| v.as_str().map(ToString::to_string).or(Some(v.to_string())))
            .unwrap_or(value_raw);
        assertions_by_field
            .entry(field)
            .or_default()
            .push(AssertionView {
                value,
                status,
                preferred: preferred != 0,
                confidence: format!("{:.3}", confidence),
            });
    }

    let linked_event_ids = query_linked_events_for_person(conn, id)?;
    let linked_family_ids = query_linked_families_for_person(conn, id)?;
    let linked_source_ids = query_linked_sources_for_person(conn, id)?;

    Ok(ShowPersonOutput {
        person,
        assertions_by_field,
        linked_event_ids,
        linked_family_ids,
        linked_source_ids,
    })
}

fn build_show_family_output(conn: &Connection, id: EntityId) -> Result<ShowFamilyOutput, String> {
    let family_json: String = conn
        .query_row(
            "SELECT data FROM families WHERE id = ?",
            rusqlite::params![id.to_string()],
            |row| row.get(0),
        )
        .map_err(|e| format!("family not found or unreadable: {}", e))?;
    let family: Family = serde_json::from_str(&family_json)
        .map_err(|e| format!("failed to parse family JSON: {}", e))?;

    let mut people = Vec::new();
    if let Some(p1) = family.partner1_id {
        people.push(p1.to_string());
    }
    if let Some(p2) = family.partner2_id {
        people.push(p2.to_string());
    }
    for child in &family.child_links {
        people.push(child.child_id.to_string());
    }

    let mut linked_event_ids = BTreeSet::new();
    for person_id in people {
        for event_id in query_linked_event_ids_by_person_id_text(conn, &person_id)? {
            linked_event_ids.insert(event_id);
        }
    }

    Ok(ShowFamilyOutput {
        family,
        linked_event_ids: linked_event_ids.into_iter().collect(),
    })
}

fn build_show_event_output(conn: &Connection, id: EntityId) -> Result<ShowEventOutput, String> {
    let event_json: String = conn
        .query_row(
            "SELECT data FROM events WHERE id = ?",
            rusqlite::params![id.to_string()],
            |row| row.get(0),
        )
        .map_err(|e| format!("event not found or unreadable: {}", e))?;
    let event: Event =
        serde_json::from_str(&event_json).map_err(|e| format!("failed to parse event JSON: {}", e))?;

    Ok(ShowEventOutput { event })
}

fn query_linked_event_ids_by_person_id_text(conn: &Connection, person_id: &str) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT DISTINCT e.id
             FROM events e
             WHERE EXISTS (
                SELECT 1
                FROM json_each(e.data, '$.participants') p
                WHERE json_extract(p.value, '$.person_id') = ?
             )",
        )
        .map_err(|e| format!("failed to prepare linked events query: {}", e))?;

    stmt.query_map(rusqlite::params![person_id], |row| row.get::<_, String>(0))
        .map_err(|e| format!("failed to query linked events: {}", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("failed to collect linked events: {}", e))
}

fn query_linked_events_for_person(conn: &Connection, id: EntityId) -> Result<Vec<String>, String> {
    query_linked_event_ids_by_person_id_text(conn, &id.to_string())
}

fn query_linked_families_for_person(conn: &Connection, id: EntityId) -> Result<Vec<String>, String> {
    let id_text = id.to_string();
    let mut stmt = conn
        .prepare(
            "SELECT DISTINCT f.id
             FROM families f
             WHERE json_extract(f.data, '$.partner1_id') = ?
                OR json_extract(f.data, '$.partner2_id') = ?
                OR EXISTS (
                    SELECT 1
                    FROM json_each(f.data, '$.child_links') c
                    WHERE json_extract(c.value, '$.child_id') = ?
                )",
        )
        .map_err(|e| format!("failed to prepare linked families query: {}", e))?;

    stmt.query_map(rusqlite::params![&id_text, &id_text, &id_text], |row| {
        row.get::<_, String>(0)
    })
    .map_err(|e| format!("failed to query linked families: {}", e))?
    .collect::<Result<Vec<_>, _>>()
    .map_err(|e| format!("failed to collect linked families: {}", e))
}

fn query_linked_sources_for_person(conn: &Connection, id: EntityId) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT DISTINCT json_extract(c.data, '$.source_id') AS source_id
             FROM assertions a
             JOIN json_each(a.source_citations) sc
             JOIN citations c ON c.id = json_extract(sc.value, '$.citation_id')
             WHERE a.entity_id = ?
               AND source_id IS NOT NULL",
        )
        .map_err(|e| format!("failed to prepare linked sources query: {}", e))?;

    stmt.query_map(rusqlite::params![id.to_string()], |row| row.get::<_, String>(0))
        .map_err(|e| format!("failed to query linked sources: {}", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("failed to collect linked sources: {}", e))
}

fn run_query_command(command: QueryCommands, db_path: &PathBuf, format: OutputFormat) {
    match command {
        QueryCommands::Person { name } => {
            let conn = match Connection::open(db_path) {
                Ok(conn) => conn,
                Err(err) => {
                    eprintln!("failed to open database '{}': {}", db_path.display(), err);
                    std::process::exit(1);
                }
            };

            let needle_json = match serde_json::to_string(&name) {
                Ok(v) => v,
                Err(err) => {
                    eprintln!("failed to serialize search name: {}", err);
                    std::process::exit(1);
                }
            };

            let mut stmt = match conn.prepare(
                "SELECT
                    p.id,
                    (
                        SELECT value FROM assertions a
                        WHERE a.entity_id = p.id
                          AND a.entity_type = 'person'
                          AND a.field = 'name'
                          AND a.status = 'confirmed'
                          AND a.preferred = 1
                        LIMIT 1
                    ) AS preferred_name,
                    (
                        SELECT value FROM assertions a
                        WHERE a.entity_id = p.id
                          AND a.entity_type = 'person'
                          AND a.field = 'birth_date'
                          AND a.status = 'confirmed'
                          AND a.preferred = 1
                        LIMIT 1
                    ) AS birth_date,
                    (
                        SELECT value FROM assertions a
                        WHERE a.entity_id = p.id
                          AND a.entity_type = 'person'
                          AND a.field = 'death_date'
                          AND a.status = 'confirmed'
                          AND a.preferred = 1
                        LIMIT 1
                    ) AS death_date
                 FROM persons p
                 WHERE EXISTS (
                    SELECT 1 FROM assertions a
                    WHERE a.entity_id = p.id
                      AND a.entity_type = 'person'
                      AND a.field = 'name'
                      AND a.status = 'confirmed'
                      AND a.preferred = 1
                      AND a.value = ?
                 )
                 ORDER BY p.id",
            ) {
                Ok(stmt) => stmt,
                Err(err) => {
                    eprintln!("failed to prepare query: {}", err);
                    std::process::exit(1);
                }
            };

            let rows = match stmt.query_map(rusqlite::params![needle_json], |row| {
                let parse_value = |raw: Option<String>| -> Result<Option<String>, rusqlite::Error> {
                    match raw {
                        Some(raw) => {
                            let value: Result<serde_json::Value, _> = serde_json::from_str(&raw);
                            match value {
                                Ok(v) => Ok(v.as_str().map(ToString::to_string).or(Some(v.to_string()))),
                                Err(_) => Ok(Some(raw)),
                            }
                        }
                        None => Ok(None),
                    }
                };

                Ok(QueryPersonRow {
                    id: row.get::<_, String>(0)?,
                    preferred_name: parse_value(row.get::<_, Option<String>>(1)?)?,
                    birth_date: parse_value(row.get::<_, Option<String>>(2)?)?,
                    death_date: parse_value(row.get::<_, Option<String>>(3)?)?,
                })
            }) {
                Ok(rows) => rows,
                Err(err) => {
                    eprintln!("failed to run query: {}", err);
                    std::process::exit(1);
                }
            };

            let rows: Vec<QueryPersonRow> = match rows.collect() {
                Ok(rows) => rows,
                Err(err) => {
                    eprintln!("failed to read query rows: {}", err);
                    std::process::exit(1);
                }
            };

            match format {
                OutputFormat::Json => match serde_json::to_string(&rows) {
                    Ok(json) => println!("{}", json),
                    Err(err) => {
                        eprintln!("failed to serialize query output: {}", err);
                        std::process::exit(1);
                    }
                },
                OutputFormat::Text => {
                    if rows.is_empty() {
                        println!("no matching persons found");
                    } else {
                        for row in rows {
                            println!(
                                "id={} name={} birth={} death={}",
                                row.id,
                                row.preferred_name.unwrap_or_else(|| "-".to_string()),
                                row.birth_date.unwrap_or_else(|| "-".to_string()),
                                row.death_date.unwrap_or_else(|| "-".to_string())
                            );
                        }
                    }
                }
            }
        }
    }
}

fn run_research_log_command(
    command: ResearchLogCommands,
    backend: &SqliteBackend,
    format: OutputFormat,
) {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
    {
        Ok(rt) => rt,
        Err(err) => {
            eprintln!("failed to initialize runtime: {}", err);
            std::process::exit(1);
        }
    };

    match command {
        ResearchLogCommands::Add {
            objective,
            result,
            person,
            repository,
        } => {
            let person_ref = person.as_deref().map(parse_entity_id_arg).transpose();
            let person_ref = match person_ref {
                Ok(v) => v,
                Err(err) => {
                    eprintln!("invalid --person id: {}", err);
                    std::process::exit(1);
                }
            };

            let repository_ref = repository.as_deref().map(parse_entity_id_arg).transpose();
            let repository_ref = match repository_ref {
                Ok(v) => v,
                Err(err) => {
                    eprintln!("invalid --repository id: {}", err);
                    std::process::exit(1);
                }
            };

            let entry = ResearchLogEntry {
                id: EntityId::new(),
                date: chrono::Utc::now(),
                objective,
                repository: repository_ref,
                repository_name: None,
                search_terms: Vec::new(),
                source_searched: None,
                result: result.into(),
                findings: None,
                citations_created: Vec::new(),
                next_steps: None,
                person_refs: person_ref.into_iter().collect(),
                tags: Vec::new(),
            };

            let result = runtime.block_on(backend.create_research_log_entry(&entry));
            match result {
                Ok(()) => match format {
                    OutputFormat::Text => println!("research-log add complete: id={}", entry.id),
                    OutputFormat::Json => {
                        println!("{{\"id\":\"{}\",\"status\":\"created\"}}", entry.id)
                    }
                },
                Err(err) => {
                    eprintln!("failed to add research log entry: {}", err.message);
                    std::process::exit(1);
                }
            }
        }
        ResearchLogCommands::List { person, result } => {
            let person_ref = person.as_deref().map(parse_entity_id_arg).transpose();
            let person_ref = match person_ref {
                Ok(v) => v,
                Err(err) => {
                    eprintln!("invalid --person id: {}", err);
                    std::process::exit(1);
                }
            };

            let filter = ResearchLogFilter {
                person_ref,
                result: result.map(Into::into),
                date_from_iso: None,
                date_to_iso: None,
            };

            let entries = runtime.block_on(backend.list_research_log_entries(
                &filter,
                Pagination {
                    limit: 100,
                    offset: 0,
                },
            ));

            match entries {
                Ok(entries) => match format {
                    OutputFormat::Text => {
                        if entries.is_empty() {
                            println!("no research-log entries found");
                        } else {
                            for e in entries {
                                println!("{} {} {:?} {}", e.id, e.date.to_rfc3339(), e.result, e.objective);
                            }
                        }
                    }
                    OutputFormat::Json => match serde_json::to_string(&entries) {
                        Ok(json) => println!("{}", json),
                        Err(err) => {
                            eprintln!("failed to serialize output: {}", err);
                            std::process::exit(1);
                        }
                    },
                },
                Err(err) => {
                    eprintln!("failed to list research log entries: {}", err.message);
                    std::process::exit(1);
                }
            }
        }
    }
}

fn parse_entity_id_arg(raw: &str) -> Result<EntityId, String> {
    serde_json::from_str::<EntityId>(&format!("\"{}\"", raw))
        .map_err(|e| format!("{} ({})", raw, e))
}

fn resolve_db_path(path: &PathBuf) -> PathBuf {
    let path_str = path.to_string_lossy();
    if path_str == "~" {
        return std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| path.clone());
    }
    if let Some(stripped) = path_str.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(stripped);
        }
    }
    path.clone()
}

#[cfg(test)]
mod tests {
    use super::{
        Cli, CliSearchResult, Commands, QueryCommands, ResearchLogCommands, ShowCommands,
        parse_entity_id_arg, resolve_db_path,
    };
    use clap::Parser;
    use std::path::PathBuf;

    #[test]
    fn resolve_db_path_leaves_absolute_path_unchanged() {
        let input = PathBuf::from("/tmp/rustygene-test.db");
        assert_eq!(resolve_db_path(&input), input);
    }

    #[test]
    fn resolve_db_path_expands_home_prefix() {
        let home = std::env::var_os("HOME").expect("HOME must be set for test");
        let resolved = resolve_db_path(&PathBuf::from("~/.rustygene/test.db"));
        assert_eq!(resolved, PathBuf::from(home).join(".rustygene/test.db"));
    }

    #[test]
    fn parse_entity_id_arg_accepts_uuid() {
        let id = parse_entity_id_arg("550e8400-e29b-41d4-a716-446655440000").expect("parse id");
        assert_eq!(id.to_string(), "550e8400-e29b-41d4-a716-446655440000");
    }

    #[test]
    fn clap_parses_research_log_add() {
        let cli = Cli::parse_from([
            "rustygene",
            "research-log",
            "add",
            "--objective",
            "Find census",
            "--result",
            "partially-found",
            "--person",
            "550e8400-e29b-41d4-a716-446655440000",
        ]);

        match cli.command {
            Commands::ResearchLog {
                command:
                    ResearchLogCommands::Add {
                        objective,
                        result,
                        person,
                        repository,
                    },
            } => {
                assert_eq!(objective, "Find census");
                assert_eq!(result, CliSearchResult::PartiallyFound);
                assert!(person.is_some());
                assert!(repository.is_none());
            }
            _ => panic!("expected research-log add command"),
        }
    }

    #[test]
    fn clap_parses_research_log_list() {
        let cli = Cli::parse_from([
            "rustygene",
            "research-log",
            "list",
            "--result",
            "not-found",
        ]);

        match cli.command {
            Commands::ResearchLog {
                command: ResearchLogCommands::List { person, result },
            } => {
                assert!(person.is_none());
                assert_eq!(result, Some(CliSearchResult::NotFound));
            }
            _ => panic!("expected research-log list command"),
        }
    }

    #[test]
    fn clap_parses_query_person() {
        let cli = Cli::parse_from(["rustygene", "query", "person", "--name", "Jones"]);

        match cli.command {
            Commands::Query {
                command: QueryCommands::Person { name },
            } => {
                assert_eq!(name, "Jones");
            }
            _ => panic!("expected query person command"),
        }
    }

    #[test]
    fn clap_parses_show_person() {
        let cli = Cli::parse_from([
            "rustygene",
            "show",
            "person",
            "550e8400-e29b-41d4-a716-446655440000",
        ]);

        match cli.command {
            Commands::Show {
                command: ShowCommands::Person { id },
            } => assert_eq!(id, "550e8400-e29b-41d4-a716-446655440000"),
            _ => panic!("expected show person command"),
        }
    }

    #[test]
    fn clap_parses_show_family() {
        let cli = Cli::parse_from([
            "rustygene",
            "show",
            "family",
            "550e8400-e29b-41d4-a716-446655440000",
        ]);

        match cli.command {
            Commands::Show {
                command: ShowCommands::Family { id },
            } => assert_eq!(id, "550e8400-e29b-41d4-a716-446655440000"),
            _ => panic!("expected show family command"),
        }
    }

    #[test]
    fn clap_parses_show_event() {
        let cli = Cli::parse_from([
            "rustygene",
            "show",
            "event",
            "550e8400-e29b-41d4-a716-446655440000",
        ]);

        match cli.command {
            Commands::Show {
                command: ShowCommands::Event { id },
            } => assert_eq!(id, "550e8400-e29b-41d4-a716-446655440000"),
            _ => panic!("expected show event command"),
        }
    }
}
