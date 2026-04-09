use std::collections::{BTreeMap, BTreeSet};
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use clap::{Parser, Subcommand, ValueEnum};
use rusqlite::Connection;
use rusqlite::OptionalExtension;
use rusqlite::types::Value as SqlValue;
use rustygene_core::assertion::{AssertionStatus, Sandbox, SandboxStatus};
use rustygene_core::event::Event;
use rustygene_core::evidence::{Citation, Media, Note, Repository, Source};
use rustygene_core::family::Family;
use rustygene_core::person::Person;
use rustygene_core::research::{ResearchLogEntry, SearchResult};
use rustygene_core::types::EntityId;
use rustygene_gedcom::{
    ExportPrivacyPolicy, family_to_fam_node, gramps, import_gedcom_to_sqlite, media_to_obje_node,
    note_to_note_node, person_to_indi_node_with_policy, render_gedcom_file,
    repository_to_repo_node, source_to_sour_node,
};
use rustygene_storage::{
    EntityType, JsonExportMode, JsonImportMode, Pagination, ResearchLogFilter,
    StagingProposalFilter, Storage, StorageError, StorageErrorCode, run_migrations,
    sqlite_impl::SqliteBackend,
};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::EnvFilter;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, serde::Serialize)]
#[serde(rename_all = "lowercase")]
enum OutputFormat {
    Json,
    Text,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
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

#[derive(serde::Serialize)]
struct CliErrorInner {
    #[serde(rename = "type")]
    r#type: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<serde_json::Value>,
}

#[derive(serde::Serialize)]
struct CliError {
    error: CliErrorInner,
    #[serde(skip)]
    exit_code: i32,
}

impl CliError {
    fn internal(msg: impl Into<String>) -> Self {
        Self {
            error: CliErrorInner {
                r#type: "internal".to_string(),
                message: msg.into(),
                details: None,
            },
            exit_code: 1,
        }
    }

    fn user(msg: impl Into<String>) -> Self {
        Self {
            error: CliErrorInner {
                r#type: "usage".to_string(),
                message: msg.into(),
                details: None,
            },
            exit_code: 2,
        }
    }

}

enum CliResponse {
    Both { text: String, json: serde_json::Value },
}

#[derive(Debug, Parser)]
#[command(
    name = "rustygene",
    version,
    about = "RustyGene CLI - A semantic genealogy tool prioritizing evidence and assertions",
    long_about = "RustyGene CLI is a genealogical data management system that implements a 'be-it-so' assertion-based model. It tracks genealogy as a series of documented assertions with confidence scores rather than a collection of static facts.",
    after_help = "Examples:\n  rustygene import --format gedcom ./tree.ged\n  rustygene query person --name \"Mary Ann\" --fuzzy\n  rustygene show person 550e8400-e29b-41d4-a716-446655440000\n  rustygene export --format json --output ./dump"
)]
struct Cli {
    /// Local SQLite database path. Created if it doesn't exist.
    #[arg(
        long,
        visible_alias = "db-path",
        global = true,
        default_value = "~/.rustygene/rustygene.db",
        env = "RUSTYGENE_DB_PATH"
    )]
    db: PathBuf,

    /// Response format (text for humans, json for machines)
    #[arg(
        long = "output-format",
        global = true,
        value_enum,
        default_value_t = OutputFormat::Text,
        env = "RUSTYGENE_OUTPUT_FORMAT",
        value_name = "FORMAT"
    )]
    output_format: OutputFormat,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Import genealogical data from external formats (GEDCOM, etc.)
    #[command(
        after_help = "Examples:\n  rustygene import --format gedcom ./tree.ged\n  rustygene import --format json ./export.json --merge"
    )]
    Import {
        /// Format of the input file
        #[arg(short, long, value_enum, value_name = "IMPORT_FORMAT")]
        format: ImportFormat,

        /// Merge into existing database instead of creating a new one
        #[arg(short, long)]
        merge: bool,

        /// Path to the file to import
        file: PathBuf,

        /// Optional job ID to track the import
        #[arg(long)]
        job_id: Option<String>,
    },

    /// Export the assertion database to external formats
    #[command(
        after_help = "Examples:\n  rustygene export --format json --output ./dump\n  rustygene export --format gedcom --redact-living"
    )]
    Export {
        /// Format of the output file(s)
        #[arg(short, long, value_enum, value_name = "EXPORT_FORMAT")]
        format: ExportFormat,

        /// Output file path or directory
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Redact biographical data for potentially living persons
        #[arg(long, default_value_t = false)]
        redact_living: bool,
    },

    /// Search persons based on criteria (name, date)
    Query {
        #[command(subcommand)]
        command: QueryCommands,
    },

    /// Inspect specific genealogical entities by unique ID
    Show {
        #[command(subcommand)]
        command: ShowCommands,
    },

    /// Manage the research log (goals, results, reasoning)
    #[command(name = "research-log", alias = "rl")]
    ResearchLog {
        #[command(subcommand)]
        command: ResearchLogCommands,
    },

    /// Experimental 'what-if' scenarios without affecting main tree
    Sandbox {
        #[command(subcommand)]
        command: SandboxCommands,
    },

    /// Review and promote assertions from staging to record
    Staging {
        #[command(subcommand)]
        command: StagingCommands,
    },

    /// Compare two GEDCOM files or a GEDCOM against the DB
    Diff {
        /// File to compare against the database
        file: PathBuf,
    },

    /// Maintenance: Rebuild all person/family snapshots from assertions
    #[command(hide = true)]
    RebuildSnapshots,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum ImportFormat {
    Gedcom,
    Gramps,
    Json,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum ExportFormat {
    Gedcom,
    Json,
}

#[derive(Debug, Subcommand)]
enum QueryCommands {
    /// Search for persons by name and/or dates
    #[command(
        after_help = "Examples:\n  rustygene query person Jones\n  rustygene query person \"Mary Ann\"\n  rustygene query person --name Smyth --fuzzy\n  rustygene query person --birth-year-from 1800 --sort-by surname"
    )]
    Person {
        /// Quick positional name query (enables fuzzy matching automatically)
        #[arg(value_name = "QUERY", conflicts_with = "name")]
        query: Option<String>,

        /// Explicit name search
        #[arg(short, long)]
        name: Option<String>,

        /// Enable phonetic matching (nysiis/metaphone)
        #[arg(short, long)]
        fuzzy: bool,

        /// Lower bound for birth year (YYYY)
        #[arg(long)]
        birth_year_from: Option<i32>,

        /// Upper bound for birth year (YYYY)
        #[arg(long)]
        birth_year_to: Option<i32>,

        /// Sort criteria for results
        #[arg(short, long, value_enum, default_value_t = QueryPersonSort::Relevance)]
        sort_by: QueryPersonSort,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum QueryPersonSort {
    Relevance,
    Surname,
    Id,
}

#[derive(Debug, Subcommand)]
enum ShowCommands {
    Person { id: EntityId },
    Family { id: EntityId },
    Event { id: EntityId },
    Source { id: EntityId },
    Citation { id: EntityId },
    Repository { id: EntityId },
    Note { id: EntityId },
    Media { id: EntityId },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
struct QueryPersonRow {
    id: EntityId,
    preferred_name: Option<String>,
    birth_date: Option<String>,
    death_date: Option<String>,
    relevance_score: f64,
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
    linked_event_ids: Vec<EntityId>,
    linked_family_ids: Vec<EntityId>,
    linked_source_ids: Vec<EntityId>,
}

#[derive(Debug, Clone, serde::Serialize)]
struct ShowFamilyOutput {
    family: Family,
    linked_event_ids: Vec<EntityId>,
}

#[derive(Debug, Clone, serde::Serialize)]
struct ShowEventOutput {
    event: Event,
}

#[derive(Debug, Subcommand)]
enum ResearchLogCommands {
    /// Add a new research log entry
    Add {
        /// What were you trying to find?
        #[arg(short, long)]
        objective: String,

        /// Outcome of the search
        #[arg(short, long, value_enum)]
        result: CliSearchResult,

        /// ID of the person this search relates to
        #[arg(short, long)]
        person: Option<EntityId>,

        /// ID of the repository where the search was performed
        #[arg(long)]
        repository: Option<EntityId>,
    },

    /// List research log entries with optional filters
    List {
        /// Filter by specific person ID
        #[arg(short, long)]
        person: Option<EntityId>,

        /// Filter by specific outcome
        #[arg(short, long, value_enum)]
        result: Option<CliSearchResult>,
    },
}

#[derive(Debug, Subcommand)]
enum SandboxCommands {
    /// Create a new analysis sandbox
    Create {
        /// Name of the sandbox
        #[arg(short, long)]
        name: String,

        /// Rationale for this scenario
        #[arg(short, long)]
        description: Option<String>,

        /// Optional parent sandbox ID for hierarchical branches
        #[arg(short, long)]
        parent: Option<EntityId>,
    },

    /// List all defined sandboxes
    List,

    /// Compare assertions in a sandbox vs. the main graph
    Compare {
        /// Sandbox ID
        #[arg(short, long)]
        sandbox: EntityId,

        /// Entity ID to compare
        #[arg(short, long)]
        entity: EntityId,

        /// Entity type (person, family, etc.)
        #[arg(long, value_enum, default_value_t = SandboxEntityType::Person)]
        entity_type: SandboxEntityType,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum SandboxEntityType {
    Person,
    Family,
    Relationship,
    Event,
    Place,
    Source,
    Citation,
    Repository,
    Media,
    Note,
    LdsOrdinance,
}

impl From<SandboxEntityType> for EntityType {
    fn from(value: SandboxEntityType) -> Self {
        match value {
            SandboxEntityType::Person => EntityType::Person,
            SandboxEntityType::Family => EntityType::Family,
            SandboxEntityType::Relationship => EntityType::Relationship,
            SandboxEntityType::Event => EntityType::Event,
            SandboxEntityType::Place => EntityType::Place,
            SandboxEntityType::Source => EntityType::Source,
            SandboxEntityType::Citation => EntityType::Citation,
            SandboxEntityType::Repository => EntityType::Repository,
            SandboxEntityType::Media => EntityType::Media,
            SandboxEntityType::Note => EntityType::Note,
            SandboxEntityType::LdsOrdinance => EntityType::LdsOrdinance,
        }
    }
}

#[derive(Debug, Subcommand)]
enum StagingCommands {
    /// List assertions waiting in staging
    List {
        /// Filter by current status
        #[arg(short, long, value_enum)]
        status: Option<CliAssertionStatus>,
    },

    /// Confirm and move assertion to main graph
    Accept {
        /// Assertion ID
        #[arg(index = 1)]
        id: EntityId,

        /// Name of the person performing the review
        #[arg(short, long)]
        reviewer: String,
    },

    /// Discard assertion from staging
    Reject {
        /// Assertion ID
        #[arg(index = 1)]
        id: EntityId,

        /// Name of the person performing the review
        #[arg(short, long)]
        reviewer: String,

        /// Reason for rejection
        #[arg(long)]
        reason: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, serde::Serialize)]
#[serde(rename_all = "lowercase")]
enum CliAssertionStatus {
    Proposed,
    Confirmed,
    Rejected,
    Disputed,
}

impl From<CliAssertionStatus> for AssertionStatus {
    fn from(value: CliAssertionStatus) -> Self {
        match value {
            CliAssertionStatus::Proposed => AssertionStatus::Proposed,
            CliAssertionStatus::Confirmed => AssertionStatus::Confirmed,
            CliAssertionStatus::Rejected => AssertionStatus::Rejected,
            CliAssertionStatus::Disputed => AssertionStatus::Disputed,
        }
    }
}

fn main() {
    let _logging_guard = match init_tracing() {
        Ok(guard) => guard,
        Err(err) => {
            eprintln!("failed to initialize tracing: {}", err);
            std::process::exit(1);
        }
    };

    let cli = Cli::parse();
    let format = cli.output_format;
    let result = run(cli);

    render_result(result, format);
}

fn run(cli: Cli) -> Result<CliResponse, CliError> {
    let db_path = resolve_db_path(&cli.db);
    tracing::debug!(db_path = %db_path.display(), command = ?cli.command, "starting rustygene CLI");

    if let Some(parent) = db_path.parent()
        && let Err(err) = std::fs::create_dir_all(parent)
    {
        return Err(CliError::internal(format!(
            "failed to create database directory '{}': {}",
            parent.display(),
            err
        )));
    }

    let mut connection = Connection::open(&db_path).map_err(|err| {
        CliError::user(format!(
            "failed to open database '{}': {}",
            db_path.display(),
            err
        ))
    })?;

    tracing::debug!(db_path = %db_path.display(), "opened sqlite database");

    run_migrations(&mut connection).map_err(|err| {
        CliError::internal(format!("failed to run database migrations: {}", err))
    })?;

    let backend = SqliteBackend::new_with_path(connection, db_path.clone());

    match cli.command {
        Commands::RebuildSnapshots => {
            let rebuilt_count = backend
                .rebuild_all_snapshots()
                .map_err(|err| CliError::internal(format!("failed to rebuild snapshots: {}", err.message)))?;

            Ok(CliResponse::Both {
                text: format!("rebuild-snapshots complete: {} entity snapshots rebuilt", rebuilt_count),
                json: serde_json::json!({ "rebuilt": rebuilt_count }),
            })
        }
        Commands::ResearchLog { command } => run_research_log_command(command, &backend),
        Commands::Sandbox { command } => run_sandbox_command(command, &backend),
        Commands::Staging { command } => run_staging_command(command, &backend),
        Commands::Query { command } => run_query_command(command, &db_path),
        Commands::Diff { file } => run_diff_command(&db_path, &file),
        Commands::Show { command } => run_show_command(command, &db_path),
        Commands::Export {
            format,
            output,
            redact_living,
        } => run_export_command(&backend, &db_path, format, output, redact_living),
        Commands::Import {
            format,
            merge,
            file,
            job_id,
        } => run_import_command(
            &db_path,
            format,
            merge,
            &file,
            job_id.as_deref(),
            &backend,
        ),
    }
}

fn render_result(result: Result<CliResponse, CliError>, format: OutputFormat) {
    match result {
        Ok(resp) => {
            match (resp, format) {
                (CliResponse::Both { text, .. }, OutputFormat::Text) => {
                    println!("{}", text.trim_end());
                }
                (CliResponse::Both { json, .. }, OutputFormat::Json) => {
                    eprintln!("{}", serde_json::to_string_pretty(&json).unwrap());
                }
            }
            std::process::exit(0);
        }
        Err(err) => {
            match format {
                OutputFormat::Text => {
                    eprintln!("error: {}", err.error.message);
                }
                OutputFormat::Json => {
                    eprintln!("{}", serde_json::to_string_pretty(&err).unwrap());
                }
            }
            std::process::exit(err.exit_code);
        }
    }
}

fn init_tracing() -> Result<WorkerGuard, String> {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"));
    let log_format = std::env::var("RUSTYGENE_LOG_FORMAT").unwrap_or_default();
    let json_logs = log_format.eq_ignore_ascii_case("json");

    if json_logs {
        let (writer, guard) = tracing_appender::non_blocking(std::io::stdout());
        tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .with_writer(writer)
            .json()
            .with_current_span(true)
            .with_span_list(true)
            .try_init()
            .map_err(|err| err.to_string())?;
        return Ok(guard);
    }

    let (writer, guard) = tracing_appender::non_blocking(std::io::stderr());
    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_writer(writer);

    if std::io::stderr().is_terminal() {
        subscriber
            .pretty()
            .try_init()
            .map_err(|err| err.to_string())?;
    } else {
        subscriber
            .compact()
            .try_init()
            .map_err(|err| err.to_string())?;
    }

    Ok(guard)
}

/// Read a GEDCOM file, handling UTF-8 with BOM, and Latin-1 (ISO-8859-1) fallback.
/// GEDCOM 5.5.1 files are often encoded as ANSI/Latin-1; Rust's std::fs::read_to_string
/// is UTF-8 only, so we fall back to byte-by-byte Latin-1 → Unicode mapping.
fn read_gedcom_file(path: &Path) -> Result<String, String> {
    match std::fs::read_to_string(path) {
        Ok(s) => return Ok(s),
        Err(e) if e.kind() != std::io::ErrorKind::InvalidData => {
            return Err(e.to_string());
        }
        Err(_) => {}
    }
    // UTF-8 failed — try Latin-1 (ISO-8859-1): every byte value maps to the same Unicode scalar.
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    Ok(bytes.iter().map(|&b| b as char).collect())
}

#[derive(Debug, Clone)]
struct PersonMergeRecord {
    id: EntityId,
    person: Person,
    key: String,
    assertions: Vec<(String, serde_json::Value)>,
}

#[derive(Debug, Clone, serde::Serialize)]
struct MergeMatchRow {
    incoming_person_id: EntityId,
    matched_person_id: EntityId,
    key: String,
}

#[derive(Debug, Clone, serde::Serialize)]
struct MergeAmbiguousRow {
    incoming_person_id: EntityId,
    candidate_person_ids: Vec<EntityId>,
    key: String,
}

#[derive(Debug, Clone, serde::Serialize)]
struct MergePlan {
    matches: Vec<MergeMatchRow>,
    new_person_ids: Vec<EntityId>,
    ambiguous: Vec<MergeAmbiguousRow>,
}

#[derive(Debug, Clone, serde::Serialize)]
struct MergeExecutionReport {
    matches: usize,
    new_entities_created: usize,
    assertions_added_to_matches: usize,
    ambiguous: Vec<MergeAmbiguousRow>,
}

fn assertion_value_as_string(raw: Option<String>) -> Option<String> {
    raw.and_then(|text| {
        serde_json::from_str::<serde_json::Value>(&text)
            .ok()
            .and_then(|v| v.as_str().map(ToString::to_string))
            .or(Some(text))
    })
}

fn normalize_merge_name(name: Option<String>) -> String {
    name.unwrap_or_default()
        .to_ascii_lowercase()
        .split_whitespace()
        .collect::<String>()
}

fn load_person_merge_records(conn: &Connection) -> Result<Vec<PersonMergeRecord>, String> {
    let persons: Vec<Person> = load_snapshot_entities(conn, "persons")?;
    let mut out = Vec::with_capacity(persons.len());

    for person in persons {
        let preferred_name_raw: Option<String> = conn
            .query_row(
                "SELECT value FROM assertions
                 WHERE entity_id = ?
                   AND entity_type = 'person'
                   AND field = 'name'
                   AND status = 'confirmed'
                   AND preferred = 1
                   AND sandbox_id IS NULL
                 LIMIT 1",
                rusqlite::params![person.id.to_string()],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| format!("preferred name lookup failed: {}", e))?;

        let birth_raw: Option<String> = conn
            .query_row(
                "SELECT value FROM assertions
                 WHERE entity_id = ?
                   AND entity_type = 'person'
                   AND field = 'birth_date'
                   AND status = 'confirmed'
                   AND preferred = 1
                   AND sandbox_id IS NULL
                 LIMIT 1",
                rusqlite::params![person.id.to_string()],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| format!("birth date lookup failed: {}", e))?;

        let death_raw: Option<String> = conn
            .query_row(
                "SELECT value FROM assertions
                 WHERE entity_id = ?
                   AND entity_type = 'person'
                   AND field = 'death_date'
                   AND status = 'confirmed'
                   AND preferred = 1
                   AND sandbox_id IS NULL
                 LIMIT 1",
                rusqlite::params![person.id.to_string()],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| format!("death date lookup failed: {}", e))?;

        let merge_name = normalize_merge_name(assertion_value_as_string(preferred_name_raw));
        let birth_date = assertion_value_as_string(birth_raw).unwrap_or_default();
        let death_date = assertion_value_as_string(death_raw).unwrap_or_default();
        let key = format!("{}|{}|{}", merge_name, birth_date, death_date);

        let mut stmt = conn
            .prepare(
                "SELECT field, value
                 FROM assertions
                 WHERE entity_id = ?
                   AND entity_type = 'person'
                   AND sandbox_id IS NULL",
            )
            .map_err(|e| format!("prepare person assertion query failed: {}", e))?;

        let assertion_rows = stmt
            .query_map(rusqlite::params![person.id.to_string()], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| format!("query person assertions failed: {}", e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("collect person assertions failed: {}", e))?;

        let assertions = assertion_rows
            .into_iter()
            .filter_map(|(field, raw)| {
                serde_json::from_str::<serde_json::Value>(&raw)
                    .ok()
                    .map(|value| (field, value))
            })
            .collect::<Vec<_>>();

        out.push(PersonMergeRecord {
            id: person.id,
            person,
            key,
            assertions,
        });
    }

    Ok(out)
}

fn build_merge_plan(existing: &[PersonMergeRecord], incoming: &[PersonMergeRecord]) -> MergePlan {
    let mut existing_by_key: BTreeMap<String, Vec<&PersonMergeRecord>> = BTreeMap::new();
    for person in existing {
        if !person.key.trim_matches('|').is_empty() {
            existing_by_key
                .entry(person.key.clone())
                .or_default()
                .push(person);
        }
    }

    let mut plan = MergePlan {
        matches: Vec::new(),
        new_person_ids: Vec::new(),
        ambiguous: Vec::new(),
    };

    for incoming_person in incoming {
        if incoming_person.key.trim_matches('|').is_empty() {
            plan.new_person_ids.push(incoming_person.id);
            continue;
        }

        match existing_by_key.get(&incoming_person.key) {
            None => {
                plan.new_person_ids.push(incoming_person.id);
            }
            Some(matches) if matches.len() == 1 => {
                plan.matches.push(MergeMatchRow {
                    incoming_person_id: incoming_person.id,
                    matched_person_id: matches[0].id,
                    key: incoming_person.key.clone(),
                });
            }
            Some(matches) => {
                plan.ambiguous.push(MergeAmbiguousRow {
                    incoming_person_id: incoming_person.id,
                    candidate_person_ids: matches.iter().map(|p| p.id).collect(),
                    key: incoming_person.key.clone(),
                });
            }
        }
    }

    plan
}

fn load_incoming_person_records_from_gedcom(file: &Path) -> Result<Vec<PersonMergeRecord>, String> {
    let content = read_gedcom_file(file)?;
    let tmp_db = std::env::temp_dir().join(format!(
        "rustygene-merge-diff-{}-{}.sqlite",
        std::process::id(),
        uuid::Uuid::new_v4().simple()
    ));

    let mut conn = Connection::open(&tmp_db).map_err(|e| format!("open temp db failed: {}", e))?;
    run_migrations(&mut conn).map_err(|e| format!("migrate temp db failed: {}", e))?;
    import_gedcom_to_sqlite(&mut conn, "merge-diff-temp", &content)
        .map_err(|e| format!("import temp GEDCOM failed: {}", e))?;

    let records = load_person_merge_records(&conn)?;
    let _ = std::fs::remove_file(&tmp_db);
    Ok(records)
}

fn run_diff_command(db_path: &PathBuf, file: &Path) -> Result<CliResponse, CliError> {
    let conn = Connection::open(db_path).map_err(|err| {
        CliError::user(format!(
            "failed to open database '{}': {}",
            db_path.display(),
            err
        ))
    })?;

    let existing = load_person_merge_records(&conn)
        .map_err(|err| CliError::internal(format!("failed to load existing persons for diff: {}", err)))?;

    let incoming = load_incoming_person_records_from_gedcom(file)
        .map_err(|err| CliError::user(format!("failed to load incoming GEDCOM for diff: {}", err)))?;

    let plan = build_merge_plan(&existing, &incoming);

    let mut text = String::from("diff complete (no DB changes)\n");
    text.push_str(&format!("  matches: {}\n", plan.matches.len()));
    text.push_str(&format!("  new entities: {}\n", plan.new_person_ids.len()));
    text.push_str(&format!("  ambiguous: {}\n", plan.ambiguous.len()));
    for row in &plan.matches {
        text.push_str(&format!(
            "    match: {} <-> {} (identity: {})\n",
            row.incoming_person_id, row.matched_person_id, row.key
        ));
    }

    Ok(CliResponse::Both {
        text,
        json: serde_json::to_value(&plan).map_err(|err| {
            CliError::internal(format!("failed to serialize diff plan: {}", err))
        })?,
    })
}

fn run_import_command(
    db_path: &PathBuf,
    import_format: ImportFormat,
    merge: bool,
    file: &Path,
    job_id: Option<&str>,
    backend: &SqliteBackend,
) -> Result<CliResponse, CliError> {
    match import_format {
        ImportFormat::Gedcom => {
            if merge {
                let conn = Connection::open(db_path).map_err(|err| {
                    CliError::user(format!(
                        "failed to open database '{}': {}",
                        db_path.display(),
                        err
                    ))
                })?;

                let existing = load_person_merge_records(&conn).map_err(|err| {
                    CliError::internal(format!("failed to load existing persons for merge: {}", err))
                })?;

                let incoming = load_incoming_person_records_from_gedcom(file).map_err(|err| {
                    CliError::user(format!("failed to load incoming GEDCOM for merge: {}", err))
                })?;

                let plan = build_merge_plan(&existing, &incoming);
                let incoming_by_id: BTreeMap<EntityId, PersonMergeRecord> = incoming
                    .into_iter()
                    .map(|r| (r.id, r))
                    .collect();

                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_time()
                    .build()
                    .map_err(|err| {
                        CliError::internal(format!("failed to initialize runtime: {}", err))
                    })?;

                let mut new_entities_created = 0usize;
                let mut assertions_added_to_matches = 0usize;

                for new_id in &plan.new_person_ids {
                    let Some(incoming_person) = incoming_by_id.get(&new_id) else {
                        continue;
                    };
                    runtime
                        .block_on(backend.create_person(&incoming_person.person))
                        .map_err(|err| {
                            CliError::internal(format!(
                                "failed to create merged person {}: {}",
                                new_id, err.message
                            ))
                        })?;
                    for (field, value) in &incoming_person.assertions {
                        let assertion = rustygene_storage::JsonAssertion {
                            id: EntityId::new(),
                            value: value.clone(),
                            confidence: 0.9,
                            status: AssertionStatus::Confirmed,
                            evidence_type: rustygene_core::assertion::EvidenceType::Direct,
                            source_citations: Vec::new(),
                            proposed_by: rustygene_core::types::ActorRef::User("merge".to_string()),
                            created_at: chrono::Utc::now(),
                            reviewed_at: None,
                            reviewed_by: None,
                        };
                        runtime
                            .block_on(backend.create_assertion(
                                incoming_person.id,
                                EntityType::Person,
                                field,
                                &assertion,
                            ))
                            .map_err(|err| {
                                CliError::internal(format!(
                                    "failed to add assertion for new person {}: {}",
                                    new_id, err.message
                                ))
                            })?;
                    }
                    new_entities_created += 1;
                }

                for matched in &plan.matches {
                    let Some(incoming_person) = incoming_by_id.get(&matched.incoming_person_id)
                    else {
                        continue;
                    };
                    let target_id = matched.matched_person_id;

                    for (field, value) in &incoming_person.assertions {
                        let assertion = rustygene_storage::JsonAssertion {
                            id: EntityId::new(),
                            value: value.clone(),
                            confidence: 0.9,
                            status: AssertionStatus::Confirmed,
                            evidence_type: rustygene_core::assertion::EvidenceType::Direct,
                            source_citations: Vec::new(),
                            proposed_by: rustygene_core::types::ActorRef::User("merge".to_string()),
                            created_at: chrono::Utc::now(),
                            reviewed_at: None,
                            reviewed_by: None,
                        };
                        runtime
                            .block_on(backend.create_assertion(
                                target_id,
                                EntityType::Person,
                                field,
                                &assertion,
                            ))
                            .map_err(|err| {
                                CliError::internal(format!(
                                    "failed to add merged assertion on existing person {}: {}",
                                    matched.matched_person_id, err.message
                                ))
                            })?;
                        assertions_added_to_matches += 1;
                    }
                }

                backend.rebuild_all_snapshots().map_err(|err| {
                    CliError::internal(format!("snapshot rebuild failed after merge: {}", err.message))
                })?;

                let report = MergeExecutionReport {
                    matches: plan.matches.len(),
                    new_entities_created,
                    assertions_added_to_matches,
                    ambiguous: plan.ambiguous,
                };

                let mut text = String::from("merge import complete\n");
                text.push_str(&format!("  matched entities: {}\n", report.matches));
                text.push_str(&format!(
                    "  new entities created: {}\n",
                    report.new_entities_created
                ));
                text.push_str(&format!(
                    "  assertions added on matched entities: {}\n",
                    report.assertions_added_to_matches
                ));
                text.push_str(&format!("  ambiguous matches: {}\n", report.ambiguous.len()));
                for a in &report.ambiguous {
                    text.push_str(&format!(
                        "  ambiguous incoming={} candidates={} key={}\n",
                        a.incoming_person_id,
                        a.candidate_person_ids.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(","),
                        a.key
                    ));
                }

                Ok(CliResponse::Both {
                    text,
                    json: serde_json::to_value(&report).map_err(|err| {
                        CliError::internal(format!("failed to serialize merge output: {}", err))
                    })?,
                })
            } else {
                let content = read_gedcom_file(file).map_err(CliError::user)?;
                let effective_job_id = job_id
                    .map(ToString::to_string)
                    .unwrap_or_else(|| format!("import-{}", uuid::Uuid::new_v4().simple()));

                let report = backend
                    .with_connection(|conn| {
                        import_gedcom_to_sqlite(conn, &effective_job_id, &content).map_err(|e| {
                            StorageError {
                                code: StorageErrorCode::Backend,
                                message: e.to_string(),
                            }
                        })
                    })
                    .map_err(|err| {
                        CliError::internal(format!("gedcom import failed: {}", err.message))
                    })?;

                tracing::info!(
                    job_id = %effective_job_id,
                    assertions_created = report.assertions_created,
                    entity_types = report.entities_created_by_type.len(),
                    "gedcom import command completed"
                );

                backend.rebuild_all_snapshots().map_err(|err| {
                    CliError::internal(format!("snapshot rebuild failed: {}", err.message))
                })?;

                let total_entities: usize = report.entities_created_by_type.values().sum();
                let mut text = format!("gedcom import complete (job: {})\n", effective_job_id);
                text.push_str(&format!("  entities created: {}\n", total_entities));
                text.push_str(&format!("  assertions created: {}\n", report.assertions_created));
                text.push_str(&format!(
                    "  unknown tags preserved: {}\n",
                    report.unknown_tags_preserved
                ));

                Ok(CliResponse::Both {
                    text,
                    json: serde_json::json!({
                        "job_id": effective_job_id,
                        "entities_created": report.entities_created_by_type,
                        "assertions_created": report.assertions_created,
                        "unknown_tags_preserved": report.unknown_tags_preserved,
                    }),
                })
            }
        }
        ImportFormat::Gramps => {
            let content = std::fs::read_to_string(file).map_err(|err| {
                CliError::user(format!("failed to read '{}': {}", file.display(), err))
            })?;

            let effective_job_id = job_id
                .map(ToString::to_string)
                .unwrap_or_else(|| format!("gramps-import-{}", uuid::Uuid::new_v4().simple()));

            let report = gramps::import_gramps_xml_to_sqlite(backend, &effective_job_id, &content)
                .map_err(|err| CliError::internal(format!("gramps xml import failed: {}", err)))?;

            tracing::info!(
                job_id = %effective_job_id,
                assertions_created = report.assertions_created,
                entity_types = report.entities_created_by_type.len(),
                "gramps import command completed"
            );

            backend.rebuild_all_snapshots().map_err(|err| {
                CliError::internal(format!("snapshot rebuild failed: {}", err.message))
            })?;

            let mut text = format!("gramps xml import complete (job: {})\n", effective_job_id);
            for (entity_type, count) in &report.entities_created_by_type {
                text.push_str(&format!("  {}: {} entities created\n", entity_type, count));
            }
            text.push_str(&format!("  assertions created: {}\n", report.assertions_created));

            Ok(CliResponse::Both {
                text,
                json: serde_json::json!({
                    "job_id": effective_job_id,
                    "entities_created": report.entities_created_by_type,
                    "assertions_created": report.assertions_created,
                }),
            })
        }
        ImportFormat::Json => {
            let mode = if file.is_dir() {
                JsonImportMode::Directory {
                    input_dir: file.to_path_buf(),
                }
            } else {
                JsonImportMode::SingleFile {
                    input_file: file.to_path_buf(),
                }
            };
            let report = backend
                .import_json_dump(mode)
                .map_err(|err| CliError::internal(format!("json import failed: {}", err.message)))?;

            tracing::info!(
                assertions_imported = report.assertions_imported,
                entity_types = report.entities_imported_by_type.len(),
                "json import command completed"
            );

            let mut text = String::from("json import complete\n");
            for (entity_type, count) in &report.entities_imported_by_type {
                text.push_str(&format!("  {}: {} entities imported\n", entity_type, count));
            }
            text.push_str(&format!("  assertions imported: {}\n", report.assertions_imported));

            Ok(CliResponse::Both {
                text,
                json: serde_json::json!({
                    "entities_imported": report.entities_imported_by_type,
                    "assertions_imported": report.assertions_imported,
                }),
            })
        }
    }
}

fn preserved_or_generated_xref(original_xref: Option<&str>, prefix: char, index: usize) -> String {
    original_xref
        .map(std::borrow::ToOwned::to_owned)
        .unwrap_or_else(|| format!("@{}{}@", prefix, index + 1))
}

fn run_export_command(
    backend: &SqliteBackend,
    db_path: &PathBuf,
    export_format: ExportFormat,
    output: Option<PathBuf>,
    redact_living: bool,
) -> Result<CliResponse, CliError> {
    match export_format {
        ExportFormat::Json => {
            let mode = match output {
                Some(path) => {
                    let is_json_file = path
                        .extension()
                        .and_then(|ext| ext.to_str())
                        .map(|ext| ext.eq_ignore_ascii_case("json"))
                        .unwrap_or(false);
                    if is_json_file {
                        JsonExportMode::SingleFile { output_file: path }
                    } else {
                        JsonExportMode::Directory { output_dir: path }
                    }
                }
                None => JsonExportMode::Directory {
                    output_dir: PathBuf::from("."),
                },
            };

            let result = backend.export_json_dump(mode).map_err(|err| {
                CliError::internal(format!("failed to export JSON: {}", err.message))
            })?;

            Ok(CliResponse::Both {
                text: format!(
                    "json export complete: {} (schema v{})",
                    result.output_path.display(),
                    result.manifest.schema_version
                ),
                json: serde_json::to_value(&result).map_err(|err| {
                    CliError::internal(format!("failed to serialize export result: {}", err))
                })?,
            })
        }
        ExportFormat::Gedcom => {
            let conn = Connection::open(db_path).map_err(|err| {
                CliError::user(format!(
                    "failed to open database '{}': {}",
                    db_path.display(),
                    err
                ))
            })?;

            let persons: Vec<Person> = load_snapshot_entities(&conn, "persons")
                .map_err(|err| CliError::internal(format!("failed to load persons: {}", err)))?;
            let families: Vec<Family> = load_family_entities(&conn)
                .map_err(|err| CliError::internal(format!("failed to load families: {}", err)))?;
            let sources: Vec<Source> = load_snapshot_entities(&conn, "sources")
                .map_err(|err| CliError::internal(format!("failed to load sources: {}", err)))?;
            let repositories: Vec<Repository> = load_snapshot_entities(&conn, "repositories")
                .map_err(|err| CliError::internal(format!("failed to load repositories: {}", err)))?;
            let notes: Vec<Note> = load_snapshot_entities(&conn, "notes")
                .map_err(|err| CliError::internal(format!("failed to load notes: {}", err)))?;
            let media: Vec<Media> = load_snapshot_entities(&conn, "media")
                .map_err(|err| CliError::internal(format!("failed to load media: {}", err)))?;

            let privacy_policy = if redact_living {
                ExportPrivacyPolicy::RedactLiving
            } else {
                ExportPrivacyPolicy::None
            };

            let events: Vec<rustygene_core::event::Event> = load_snapshot_entities(&conn, "events")
                .map_err(|err| CliError::internal(format!("failed to load events: {}", err)))?;
            let places: Vec<rustygene_core::place::Place> = load_snapshot_entities(&conn, "places")
                .map_err(|err| CliError::internal(format!("failed to load places: {}", err)))?;

            let mut nodes = Vec::new();
            for (idx, person) in persons.iter().enumerate() {
                let xref = preserved_or_generated_xref(person.original_xref.as_deref(), 'I', idx);
                if let Some(node) =
                    person_to_indi_node_with_policy(person, &events, &places, &xref, privacy_policy)
                {
                    nodes.push(node);
                }
            }
            for (idx, family) in families.iter().enumerate() {
                let xref = preserved_or_generated_xref(family.original_xref.as_deref(), 'F', idx);
                nodes.push(family_to_fam_node(
                    family, &persons, &events, &places, &xref,
                ));
            }
            for (idx, source) in sources.iter().enumerate() {
                let xref = preserved_or_generated_xref(source.original_xref.as_deref(), 'S', idx);
                nodes.push(source_to_sour_node(source, &xref));
            }
            for (idx, repository) in repositories.iter().enumerate() {
                let xref =
                    preserved_or_generated_xref(repository.original_xref.as_deref(), 'R', idx);
                nodes.push(repository_to_repo_node(repository, &xref));
            }
            for (idx, note) in notes.iter().enumerate() {
                let xref = preserved_or_generated_xref(note.original_xref.as_deref(), 'N', idx);
                nodes.push(note_to_note_node(note, &xref));
            }
            for (idx, item) in media.iter().enumerate() {
                let xref = preserved_or_generated_xref(item.original_xref.as_deref(), 'O', idx);
                nodes.push(media_to_obje_node(item, &xref));
            }

            let rendered = render_gedcom_file(&nodes);
            if let Some(path) = output {
                if let Some(parent) = path.parent()
                    && !parent.as_os_str().is_empty()
                    && let Err(err) = std::fs::create_dir_all(parent)
                {
                    return Err(CliError::internal(format!(
                        "failed to create export directory '{}': {}",
                        parent.display(),
                        err
                    )));
                }

                std::fs::write(&path, &rendered).map_err(|err| {
                    CliError::internal(format!("failed to write GEDCOM file '{}': {}", path.display(), err))
                })?;

                Ok(CliResponse::Both {
                    text: format!("gedcom export complete: {}", path.display()),
                    json: serde_json::json!({ "path": path, "status": "complete" }),
                })
            } else {
                Ok(CliResponse::Both {
                    text: rendered.clone(),
                    json: serde_json::json!({ "gedcom": rendered }),
                })
            }
        }
    }
}

fn load_snapshot_entities<T: serde::de::DeserializeOwned>(
    conn: &Connection,
    table: &str,
) -> Result<Vec<T>, String> {
    let mut stmt = conn
        .prepare(&format!("SELECT data FROM {} ORDER BY created_at", table))
        .map_err(|e| format!("prepare {} query failed: {}", table, e))?;

    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|e| format!("query {} failed: {}", table, e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("collect {} failed: {}", table, e))?;

    rows.into_iter()
        .map(|raw| {
            serde_json::from_str::<T>(&raw)
                .map_err(|e| format!("parse {} row failed: {}", table, e))
        })
        .collect()
}

/// Load only `Family` rows from the shared `families` table.
/// `Relationship` rows are co-stored there and are excluded by filtering out
/// rows that carry a `relationship_type` JSON field.
fn load_family_entities(conn: &Connection) -> Result<Vec<Family>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT data FROM families \
             WHERE json_extract(data, '$.relationship_type') IS NULL \
             ORDER BY created_at",
        )
        .map_err(|e| format!("prepare families query failed: {}", e))?;

    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|e| format!("query families failed: {}", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("collect families failed: {}", e))?;

    rows.into_iter()
        .map(|raw| {
            serde_json::from_str::<Family>(&raw)
                .map_err(|e| format!("parse families row failed: {}", e))
        })
        .collect()
}

fn run_show_command(
    command: ShowCommands,
    db_path: &PathBuf,
) -> Result<CliResponse, CliError> {
    let conn = Connection::open(db_path).map_err(|err| {
        CliError::user(format!(
            "failed to open database '{}': {}",
            db_path.display(),
            err
        ))
    })?;

    match command {
        ShowCommands::Person { id } => {
            let id = id;
            let output = build_show_person_output(&conn, id).map_err(CliError::internal)?;

            let mut text = format!("person: {}\nassertions:\n", output.person.id);
            for (field, assertions) in output.assertions_by_field.clone() {
                text.push_str(&format!("  {}:\n", field));
                for a in assertions {
                    text.push_str(&format!(
                        "    value={} status={} preferred={} confidence={}\n",
                        a.value, a.status, a.preferred, a.confidence
                    ));
                }
            }
            text.push_str(&format!(
                "linked events: {}\n",
                output.linked_event_ids.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(",")
            ));
            text.push_str(&format!(
                "linked families: {}\n",
                output.linked_family_ids.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(",")
            ));
            text.push_str(&format!(
                "linked sources: {}\n",
                output.linked_source_ids.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(",")
            ));

            Ok(CliResponse::Both {
                text,
                json: serde_json::to_value(&output).map_err(|err| {
                    CliError::internal(format!("failed to serialize person: {}", err))
                })?,
            })
        }
        ShowCommands::Family { id } => {
            let id = id;
            let output = build_show_family_output(&conn, id).map_err(CliError::internal)?;

            let mut text = format!("family: {}\n", output.family.id);
            text.push_str(&format!("partner1: {:?}\n", output.family.partner1_id));
            text.push_str(&format!("partner2: {:?}\n", output.family.partner2_id));
            text.push_str(&format!("children: {}\n", output.family.child_links.len()));
            text.push_str(&format!(
                "linked events: {}\n",
                output.linked_event_ids.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(",")
            ));

            Ok(CliResponse::Both {
                text,
                json: serde_json::to_value(&output).map_err(|err| {
                    CliError::internal(format!("failed to serialize family: {}", err))
                })?,
            })
        }
        ShowCommands::Event { id } => {
            let id = id;
            let output = build_show_event_output(&conn, id).map_err(CliError::internal)?;

            let mut text = format!("event: {}\n", output.event.id);
            text.push_str(&format!("type: {:?}\n", output.event.event_type));
            text.push_str(&format!("participants: {}\n", output.event.participants.len()));

            Ok(CliResponse::Both {
                text,
                json: serde_json::to_value(&output).map_err(|err| {
                    CliError::internal(format!("failed to serialize event: {}", err))
                })?,
            })
        }
        ShowCommands::Source { id } => {
            let id = id;
            let source: Source = load_entity_from_table(&conn, "sources", id, "source")
                .map_err(CliError::internal)?;

            let mut text = format!("source: {}\n", source.id);
            text.push_str(&format!("title: {}\n", source.title));
            text.push_str(&format!(
                "author: {}\n",
                source.author.as_deref().unwrap_or("-")
            ));
            text.push_str(&format!(
                "publication: {}\n",
                source.publication_info.as_deref().unwrap_or("-")
            ));
            text.push_str(&format!("repository_refs: {}\n", source.repository_refs.len()));

            Ok(CliResponse::Both {
                text,
                json: serde_json::to_value(&source).map_err(|err| {
                    CliError::internal(format!("failed to serialize source: {}", err))
                })?,
            })
        }
        ShowCommands::Citation { id } => {
            let id = id;
            let citation: Citation = load_entity_from_table(&conn, "citations", id, "citation")
                .map_err(CliError::internal)?;

            let mut text = format!("citation: {}\n", citation.id);
            text.push_str(&format!("source_id: {}\n", citation.source_id));
            text.push_str(&format!("page: {}\n", citation.page.as_deref().unwrap_or("-")));
            text.push_str(&format!(
                "transcription: {}\n",
                citation.transcription.as_deref().unwrap_or("-")
            ));

            Ok(CliResponse::Both {
                text,
                json: serde_json::to_value(&citation).map_err(|err| {
                    CliError::internal(format!("failed to serialize citation: {}", err))
                })?,
            })
        }
        ShowCommands::Repository { id } => {
            let id = id;
            let repository: Repository =
                load_entity_from_table(&conn, "repositories", id, "repository")
                    .map_err(CliError::internal)?;

            let mut text = format!("repository: {}\n", repository.id);
            text.push_str(&format!("name: {}\n", repository.name));
            text.push_str(&format!("type: {:?}\n", repository.repository_type));
            text.push_str(&format!("urls: {}\n", repository.urls.len()));

            Ok(CliResponse::Both {
                text,
                json: serde_json::to_value(&repository).map_err(|err| {
                    CliError::internal(format!("failed to serialize repository: {}", err))
                })?,
            })
        }
        ShowCommands::Note { id } => {
            let id = id;
            let note: Note = load_entity_from_table(&conn, "notes", id, "note")
                .map_err(CliError::internal)?;

            let mut text = format!("note: {}\n", note.id);
            text.push_str(&format!("type: {:?}\n", note.note_type));
            text.push_str(&format!("text: {}\n", note.text));

            Ok(CliResponse::Both {
                text,
                json: serde_json::to_value(&note).map_err(|err| {
                    CliError::internal(format!("failed to serialize note: {}", err))
                })?,
            })
        }
        ShowCommands::Media { id } => {
            let id = id;
            let media: Media = load_entity_from_table(&conn, "media", id, "media")
                .map_err(CliError::internal)?;

            let mut text = format!("media: {}\n", media.id);
            text.push_str(&format!("file_path: {}\n", media.file_path));
            text.push_str(&format!("mime_type: {}\n", media.mime_type));
            text.push_str(&format!(
                "caption: {}\n",
                media.caption.as_deref().unwrap_or("-")
            ));

            Ok(CliResponse::Both {
                text,
                json: serde_json::to_value(&media).map_err(|err| {
                    CliError::internal(format!("failed to serialize media: {}", err))
                })?,
            })
        }
    }
}

fn load_entity_from_table<T: serde::de::DeserializeOwned>(
    conn: &Connection,
    table: &str,
    id: EntityId,
    label: &str,
) -> Result<T, String> {
    let sql = format!("SELECT data FROM {} WHERE id = ?", table);
    let raw: String = conn
        .query_row(&sql, rusqlite::params![id.to_string()], |row| row.get(0))
        .map_err(|e| format!("{} not found or unreadable: {}", label, e))?;
    serde_json::from_str::<T>(&raw).map_err(|e| format!("failed to parse {} JSON: {}", label, e))
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
        linked_event_ids: linked_event_ids.into_iter().filter_map(|s| EntityId::from_str(&s).ok()).collect(),
        linked_family_ids: linked_family_ids.into_iter().filter_map(|s| EntityId::from_str(&s).ok()).collect(),
        linked_source_ids: linked_source_ids.into_iter().filter_map(|s| EntityId::from_str(&s).ok()).collect(),
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
        linked_event_ids: linked_event_ids.into_iter().filter_map(|s| EntityId::from_str(&s).ok()).collect(),
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
    let event: Event = serde_json::from_str(&event_json)
        .map_err(|e| format!("failed to parse event JSON: {}", e))?;

    Ok(ShowEventOutput { event })
}

fn query_linked_event_ids_by_person_id_text(
    conn: &Connection,
    person_id: &str,
) -> Result<Vec<String>, String> {
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

fn query_linked_families_for_person(
    conn: &Connection,
    id: EntityId,
) -> Result<Vec<String>, String> {
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

    stmt.query_map(rusqlite::params![id.to_string()], |row| {
        row.get::<_, String>(0)
    })
    .map_err(|e| format!("failed to query linked sources: {}", e))?
    .collect::<Result<Vec<_>, _>>()
    .map_err(|e| format!("failed to collect linked sources: {}", e))
}

fn run_query_command(command: QueryCommands, db_path: &PathBuf) -> Result<CliResponse, CliError> {
    match command {
        QueryCommands::Person {
            query,
            name,
            fuzzy,
            birth_year_from,
            birth_year_to,
            sort_by,
        } => {
            let conn = Connection::open(db_path).map_err(|err| {
                CliError::user(format!(
                    "failed to open database '{}': {}",
                    db_path.display(),
                    err
                ))
            })?;

            let (effective_name, effective_fuzzy) =
                resolve_person_query(query.as_deref(), name.as_deref(), fuzzy);

            let rows = query_person_rows(
                &conn,
                effective_name,
                effective_fuzzy,
                birth_year_from,
                birth_year_to,
                sort_by,
            )
            .map_err(CliError::internal)?;

            let mut text = String::new();
            if rows.is_empty() {
                text.push_str("no matching persons found\n");
            } else {
                for row in &rows {
                    text.push_str(&format!(
                        "id={} name={} birth={} death={} score={:.4}\n",
                        row.id,
                        row.preferred_name.as_deref().unwrap_or("-"),
                        row.birth_date.as_deref().unwrap_or("-"),
                        row.death_date.as_deref().unwrap_or("-"),
                        row.relevance_score
                    ));
                }
            }

            Ok(CliResponse::Both {
                text,
                json: serde_json::to_value(&rows).map_err(|err| {
                    CliError::internal(format!("failed to serialize query output: {}", err))
                })?,
            })
        }
    }
}

fn resolve_person_query<'a>(
    positional_query: Option<&'a str>,
    named_query: Option<&'a str>,
    fuzzy_flag: bool,
) -> (Option<&'a str>, bool) {
    let effective_name = named_query.or(positional_query);
    let effective_fuzzy = positional_query.is_some() || fuzzy_flag;
    (effective_name, effective_fuzzy)
}

fn search_terms(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(|token| token.to_ascii_lowercase())
        .collect()
}

fn soundex(token: &str) -> Option<String> {
    let mut chars = token.chars().filter(|c| c.is_ascii_alphabetic());
    let first = chars.next()?.to_ascii_uppercase();

    let mut result = String::with_capacity(4);
    result.push(first);

    let mut previous = match first {
        'B' | 'F' | 'P' | 'V' => '1',
        'C' | 'G' | 'J' | 'K' | 'Q' | 'S' | 'X' | 'Z' => '2',
        'D' | 'T' => '3',
        'L' => '4',
        'M' | 'N' => '5',
        'R' => '6',
        _ => '0',
    };

    for c in chars {
        let code = match c.to_ascii_uppercase() {
            'B' | 'F' | 'P' | 'V' => '1',
            'C' | 'G' | 'J' | 'K' | 'Q' | 'S' | 'X' | 'Z' => '2',
            'D' | 'T' => '3',
            'L' => '4',
            'M' | 'N' => '5',
            'R' => '6',
            _ => '0',
        };

        if code == '0' {
            previous = code;
            continue;
        }

        if code != previous {
            result.push(code);
        }
        previous = code;
        if result.len() == 4 {
            break;
        }
    }

    while result.len() < 4 {
        result.push('0');
    }

    Some(result)
}

fn simple_metaphone(token: &str) -> Option<String> {
    let mut chars = token
        .chars()
        .filter(|c| c.is_ascii_alphabetic())
        .map(|c| c.to_ascii_uppercase())
        .peekable();

    let mut out = String::new();
    while let Some(ch) = chars.next() {
        let code = match ch {
            'A' | 'E' | 'I' | 'O' | 'U' => {
                if out.is_empty() {
                    Some(ch)
                } else {
                    None
                }
            }
            'B' => Some('B'),
            'C' => {
                if matches!(chars.peek(), Some('H')) {
                    chars.next();
                    Some('X')
                } else {
                    Some('K')
                }
            }
            'D' => Some('T'),
            'F' => Some('F'),
            'G' => {
                if matches!(chars.peek(), Some('H')) {
                    chars.next();
                    Some('F')
                } else {
                    Some('K')
                }
            }
            'H' => None,
            'J' => Some('J'),
            'K' | 'Q' => Some('K'),
            'L' => Some('L'),
            'M' | 'N' => Some('N'),
            'P' => {
                if matches!(chars.peek(), Some('H')) {
                    chars.next();
                    Some('F')
                } else {
                    Some('P')
                }
            }
            'R' => Some('R'),
            'S' | 'X' | 'Z' => Some('S'),
            'T' => Some('T'),
            'V' | 'W' => Some('F'),
            'Y' => None,
            _ => None,
        };

        if let Some(code) = code
            && !out.ends_with(code)
        {
            out.push(code);
        }
    }

    if out.is_empty() { None } else { Some(out) }
}

fn build_person_match_query(name: &str, fuzzy: bool) -> String {
    let terms = search_terms(name);
    if terms.is_empty() {
        return String::from("*");
    }

    let groups = terms
        .into_iter()
        .map(|term| {
            let mut variants = Vec::new();
            variants.push(format!("{}*", term));
            if term.len() >= 3 {
                variants.push(format!("{}*", &term[0..2]));
            }

            if fuzzy {
                if let Some(sx) = soundex(&term) {
                    variants.push(format!("sx{}", sx.to_ascii_lowercase()));
                }
                if let Some(mp) = simple_metaphone(&term) {
                    variants.push(format!("mp{}", mp.to_ascii_lowercase()));
                }
            }

            format!("({})", variants.join(" OR "))
        })
        .collect::<Vec<_>>();

    groups.join(" AND ")
}

fn query_person_rows(
    conn: &Connection,
    name: Option<&str>,
    fuzzy: bool,
    birth_year_from: Option<i32>,
    birth_year_to: Option<i32>,
    sort_by: QueryPersonSort,
) -> Result<Vec<QueryPersonRow>, String> {
    let mut sql = String::from(
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
                ) AS death_date,
                {rank_expr}
             {from_clause}
                 WHERE {where_clause}",
    );

    let mut args: Vec<SqlValue> = Vec::new();

    let (rank_expr, from_clause, mut where_clause) = if let Some(search_name) = name {
        let match_query = build_person_match_query(search_name, fuzzy);
        args.push(SqlValue::Text(match_query));
        (
            "bm25(search_index) AS rank",
            "FROM search_index si JOIN persons p ON p.id = si.entity_id",
            String::from("si.entity_type = 'person' AND si.content MATCH ?"),
        )
    } else {
        ("0.0 AS rank", "FROM persons p", String::from("1 = 1"))
    };

    if let Some(from) = birth_year_from {
        where_clause.push_str(" AND p.birth_year >= ?");
        args.push(SqlValue::Integer(i64::from(from)));
    }
    if let Some(to) = birth_year_to {
        where_clause.push_str(" AND p.birth_year <= ?");
        args.push(SqlValue::Integer(i64::from(to)));
    }

    sql = sql
        .replace("{rank_expr}", rank_expr)
        .replace("{from_clause}", from_clause)
        .replace("{where_clause}", &where_clause);

    match sort_by {
        QueryPersonSort::Relevance => sql.push_str(" ORDER BY rank ASC, p.id ASC"),
        QueryPersonSort::Surname => {
            sql.push_str(
                " ORDER BY p.primary_surname COLLATE NOCASE ASC, p.primary_given_name COLLATE NOCASE ASC, p.id ASC",
            );
        }
        QueryPersonSort::Id => sql.push_str(" ORDER BY p.id ASC"),
    }

    let mut stmt = conn
        .prepare(&sql)
        .map_err(|err| format!("failed to prepare query: {}", err))?;

    let rows = stmt
        .query_map(rusqlite::params_from_iter(args.iter()), |row| {
            let parse_value = |raw: Option<String>| -> Result<Option<String>, rusqlite::Error> {
                match raw {
                    Some(raw) => {
                        let value: Result<serde_json::Value, _> = serde_json::from_str(&raw);
                        match value {
                            Ok(v) => {
                                Ok(v.as_str().map(ToString::to_string).or(Some(v.to_string())))
                            }
                            Err(_) => Ok(Some(raw)),
                        }
                    }
                    None => Ok(None),
                }
            };

            let rank: f64 = row.get(4)?;
            Ok(QueryPersonRow {
                id: EntityId::from_str(&row.get::<_, String>(0)?).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,
                preferred_name: parse_value(row.get::<_, Option<String>>(1)?)?,
                birth_date: parse_value(row.get::<_, Option<String>>(2)?)?,
                death_date: parse_value(row.get::<_, Option<String>>(3)?)?,
                relevance_score: -rank,
            })
        })
        .map_err(|err| format!("failed to run query: {}", err))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("failed to read query rows: {}", err))
}

fn run_research_log_command(
    command: ResearchLogCommands,
    backend: &SqliteBackend,
) -> Result<CliResponse, CliError> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .map_err(|err| CliError::internal(format!("failed to initialize runtime: {}", err)))?;

    match command {
        ResearchLogCommands::Add {
            objective,
            result,
            person,
            repository,
        } => {
            let person_ref = person;

            let repository_ref = repository;

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

            runtime
                .block_on(backend.create_research_log_entry(&entry))
                .map_err(|err| {
                    CliError::internal(format!("failed to add research log entry: {}", err.message))
                })?;

            Ok(CliResponse::Both {
                text: format!("research-log add complete: id={}", entry.id),
                json: serde_json::json!({ "id": entry.id, "status": "created" }),
            })
        }
        ResearchLogCommands::List { person, result } => {
            let person_ref = person;

            let filter = ResearchLogFilter {
                person_ref,
                result: result.map(Into::into),
                date_from_iso: None,
                date_to_iso: None,
            };

            let entries = runtime
                .block_on(backend.list_research_log_entries(
                    &filter,
                    Pagination {
                        limit: 100,
                        offset: 0,
                    },
                ))
                .map_err(|err| {
                    CliError::internal(format!("failed to list research log entries: {}", err.message))
                })?;

            let mut text = String::new();
            if entries.is_empty() {
                text.push_str("no research-log entries found\n");
            } else {
                for e in &entries {
                    text.push_str(&format!(
                        "{} {} {:?} {}\n",
                        e.id,
                        e.date.to_rfc3339(),
                        e.result,
                        e.objective
                    ));
                }
            }

            Ok(CliResponse::Both {
                text,
                json: serde_json::to_value(&entries).map_err(|err| {
                    CliError::internal(format!("failed to serialize output: {}", err))
                })?,
            })
        }
    }
}

fn run_sandbox_command(
    command: SandboxCommands,
    backend: &SqliteBackend,
) -> Result<CliResponse, CliError> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .map_err(|err| CliError::internal(format!("failed to initialize runtime: {}", err)))?;

    match command {
        SandboxCommands::Create {
            name,
            description,
            parent,
        } => {
            let parent_sandbox = parent;

            let sandbox = Sandbox {
                id: EntityId::new(),
                name,
                description,
                created_at: chrono::Utc::now(),
                parent_sandbox,
                status: SandboxStatus::Active,
            };

            runtime
                .block_on(backend.create_sandbox(&sandbox))
                .map_err(|err| {
                    CliError::internal(format!("failed to create sandbox: {}", err.message))
                })?;

            Ok(CliResponse::Both {
                text: format!("sandbox created: id={} name={}", sandbox.id, sandbox.name),
                json: serde_json::json!({ "id": sandbox.id, "name": sandbox.name, "status": "active" }),
            })
        }
        SandboxCommands::List => {
            let items = runtime
                .block_on(backend.list_sandboxes(Pagination {
                    limit: 500,
                    offset: 0,
                }))
                .map_err(|err| {
                    CliError::internal(format!("failed to list sandboxes: {}", err.message))
                })?;

            let mut text = String::new();
            if items.is_empty() {
                text.push_str("no sandboxes found\n");
            } else {
                for s in &items {
                    text.push_str(&format!("{} {:?} {}\n", s.id, s.status, s.name));
                }
            }

            Ok(CliResponse::Both {
                text,
                json: serde_json::to_value(&items).map_err(|err| {
                    CliError::internal(format!("failed to serialize output: {}", err))
                })?,
            })
        }
        SandboxCommands::Compare {
            sandbox,
            entity,
            entity_type,
        } => {
            let sandbox_id = sandbox;
            let entity_id = entity;

            let diffs = runtime
                .block_on(backend.compare_sandbox_vs_trunk(
                    entity_id,
                    entity_type.into(),
                    sandbox_id,
                ))
                .map_err(|err| {
                    CliError::internal(format!("failed to compare sandbox: {}", err.message))
                })?;

            let mut text = String::new();
            if diffs.is_empty() {
                text.push_str("no diffs between trunk and sandbox\n");
            } else {
                for d in &diffs {
                    text.push_str(&format!(
                        "field={} trunk={} sandbox={}\n",
                        d.field,
                        d.trunk_value
                            .as_ref()
                            .map(|v| v.to_string())
                            .unwrap_or_else(|| "<none>".to_string()),
                        d.sandbox_value
                            .as_ref()
                            .map(|v| v.to_string())
                            .unwrap_or_else(|| "<none>".to_string())
                    ));
                }
            }

            Ok(CliResponse::Both {
                text,
                json: serde_json::to_value(&diffs).map_err(|err| {
                    CliError::internal(format!("failed to serialize output: {}", err))
                })?,
            })
        }
    }
}

fn run_staging_command(
    command: StagingCommands,
    backend: &SqliteBackend,
) -> Result<CliResponse, CliError> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .map_err(|err| CliError::internal(format!("failed to initialize runtime: {}", err)))?;

    match command {
        StagingCommands::List { status } => {
            let filter = StagingProposalFilter {
                entity_id: None,
                entity_type: None,
                status: status.map(Into::into),
            };
            let items = runtime
                .block_on(backend.list_staging_proposals(
                    &filter,
                    Pagination {
                        limit: 500,
                        offset: 0,
                    },
                ))
                .map_err(|err| {
                    CliError::internal(format!("failed to list staging proposals: {}", err.message))
                })?;

            let mut text = String::new();
            if items.is_empty() {
                text.push_str("no staging proposals found\n");
            } else {
                for p in &items {
                    text.push_str(&format!(
                        "{} {:?} entity={} field={} submitted_by={}\n",
                        p.id, p.status, p.entity_id, p.field, p.submitted_by
                    ));
                }
            }

            Ok(CliResponse::Both {
                text,
                json: serde_json::to_value(&items).map_err(|err| {
                    CliError::internal(format!("failed to serialize output: {}", err))
                })?,
            })
        }
        StagingCommands::Accept { id, reviewer } => {
            let proposal_id = id;

            runtime
                .block_on(backend.accept_staging_proposal(proposal_id, &reviewer))
                .map_err(|err| {
                    CliError::internal(format!("failed to accept proposal: {}", err.message))
                })?;

            Ok(CliResponse::Both {
                text: format!("proposal accepted: {}", id),
                json: serde_json::json!({ "id": id, "status": "confirmed" }),
            })
        }
        StagingCommands::Reject {
            id,
            reviewer,
            reason,
        } => {
            let proposal_id = id;

            runtime
                .block_on(backend.reject_staging_proposal(
                    proposal_id,
                    &reviewer,
                    reason.as_deref(),
                ))
                .map_err(|err| {
                    CliError::internal(format!("failed to reject proposal: {}", err.message))
                })?;

            Ok(CliResponse::Both {
                text: format!("proposal rejected: {}", id),
                json: serde_json::json!({ "id": id, "status": "rejected" }),
            })
        }
    }
}


fn resolve_db_path(path: &Path) -> PathBuf {
    let path_str = path.to_string_lossy();
    if path_str == "~" {
        return std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| path.to_path_buf());
    }
    if let Some(stripped) = path_str.strip_prefix("~/")
        && let Some(home) = std::env::var_os("HOME")
    {
        return PathBuf::from(home).join(stripped);
    }
    path.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::{
        Cli, CliSearchResult, Commands, ExportFormat, ImportFormat, QueryCommands,
        QueryPersonSort, ResearchLogCommands, SandboxCommands, ShowCommands, StagingCommands,
    };
    use super::{
        build_person_match_query, preserved_or_generated_xref,
        resolve_db_path, resolve_person_query,
    };
    use clap::Parser;
    use rusqlite::Connection;
    use rustygene_gedcom::import_gedcom_to_sqlite;
    use rustygene_storage::{run_migrations, sqlite_impl::SqliteBackend};
    use std::path::{Path, PathBuf};

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
        let cli = Cli::parse_from(["rustygene", "research-log", "list", "--result", "not-found"]);

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
                command:
                    QueryCommands::Person {
                        query,
                        name,
                        fuzzy,
                        birth_year_from,
                        birth_year_to,
                        sort_by,
                    },
            } => {
                assert_eq!(query, None);
                assert_eq!(name, Some("Jones".to_string()));
                assert!(!fuzzy);
                assert_eq!(birth_year_from, None);
                assert_eq!(birth_year_to, None);
                assert_eq!(sort_by, QueryPersonSort::Relevance);
            }
            _ => panic!("expected query person command"),
        }
    }

    #[test]
    fn clap_parses_query_person_fuzzy() {
        let cli = Cli::parse_from(["rustygene", "query", "person", "--name", "Jon", "--fuzzy"]);

        match cli.command {
            Commands::Query {
                command:
                    QueryCommands::Person {
                        query,
                        name,
                        fuzzy,
                        birth_year_from,
                        birth_year_to,
                        sort_by,
                    },
            } => {
                assert_eq!(query, None);
                assert_eq!(name, Some("Jon".to_string()));
                assert!(fuzzy);
                assert_eq!(birth_year_from, None);
                assert_eq!(birth_year_to, None);
                assert_eq!(sort_by, QueryPersonSort::Relevance);
            }
            _ => panic!("expected query person command"),
        }
    }

    #[test]
    fn clap_parses_query_person_birth_year_and_sort() {
        let cli = Cli::parse_from([
            "rustygene",
            "query",
            "person",
            "--birth-year-from",
            "1800",
            "--birth-year-to",
            "1900",
            "--sort-by",
            "surname",
        ]);

        match cli.command {
            Commands::Query {
                command:
                    QueryCommands::Person {
                        query,
                        name,
                        fuzzy,
                        birth_year_from,
                        birth_year_to,
                        sort_by,
                    },
            } => {
                assert_eq!(query, None);
                assert_eq!(name, None);
                assert!(!fuzzy);
                assert_eq!(birth_year_from, Some(1800));
                assert_eq!(birth_year_to, Some(1900));
                assert_eq!(sort_by, QueryPersonSort::Surname);
            }
            _ => panic!("expected query person command"),
        }
    }

    #[test]
    fn clap_parses_query_person_positional_quick_search() {
        let cli = Cli::parse_from(["rustygene", "query", "person", "Jones"]);

        match cli.command {
            Commands::Query {
                command:
                    QueryCommands::Person {
                        query,
                        name,
                        fuzzy,
                        birth_year_from,
                        birth_year_to,
                        sort_by,
                    },
            } => {
                assert_eq!(query, Some("Jones".to_string()));
                assert_eq!(name, None);
                assert!(!fuzzy);
                assert_eq!(birth_year_from, None);
                assert_eq!(birth_year_to, None);
                assert_eq!(sort_by, QueryPersonSort::Relevance);
            }
            _ => panic!("expected query person command"),
        }
    }

    #[test]
    fn clap_parses_global_db_path_alias() {
        let cli = Cli::parse_from([
            "rustygene",
            "--db-path",
            "./tmp/family.db",
            "query",
            "person",
            "Jones",
        ]);
        assert_eq!(cli.db, PathBuf::from("./tmp/family.db"));
    }

    #[test]
    fn resolve_person_query_enables_fuzzy_for_positional_search() {
        let (name, fuzzy) = resolve_person_query(Some("Jones"), None, false);
        assert_eq!(name, Some("Jones"));
        assert!(fuzzy);

        let (name, fuzzy) = resolve_person_query(None, Some("Jones"), false);
        assert_eq!(name, Some("Jones"));
        assert!(!fuzzy);
    }

    #[test]
    fn fuzzy_match_builder_includes_phonetic_tokens() {
        let q = build_person_match_query("Smyth", true);
        assert!(q.contains("smyth*"));
        assert!(q.contains("sxs530"));
        assert!(q.contains("mpsn"));
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

    #[test]
    fn clap_parses_show_source() {
        let cli = Cli::parse_from([
            "rustygene",
            "show",
            "source",
            "550e8400-e29b-41d4-a716-446655440000",
        ]);

        match cli.command {
            Commands::Show {
                command: ShowCommands::Source { id },
            } => assert_eq!(id, "550e8400-e29b-41d4-a716-446655440000"),
            _ => panic!("expected show source command"),
        }
    }

    #[test]
    fn clap_parses_show_citation() {
        let cli = Cli::parse_from([
            "rustygene",
            "show",
            "citation",
            "550e8400-e29b-41d4-a716-446655440000",
        ]);

        match cli.command {
            Commands::Show {
                command: ShowCommands::Citation { id },
            } => assert_eq!(id, "550e8400-e29b-41d4-a716-446655440000"),
            _ => panic!("expected show citation command"),
        }
    }

    #[test]
    fn clap_parses_show_repository() {
        let cli = Cli::parse_from([
            "rustygene",
            "show",
            "repository",
            "550e8400-e29b-41d4-a716-446655440000",
        ]);

        match cli.command {
            Commands::Show {
                command: ShowCommands::Repository { id },
            } => assert_eq!(id, "550e8400-e29b-41d4-a716-446655440000"),
            _ => panic!("expected show repository command"),
        }
    }

    #[test]
    fn clap_parses_show_note() {
        let cli = Cli::parse_from([
            "rustygene",
            "show",
            "note",
            "550e8400-e29b-41d4-a716-446655440000",
        ]);

        match cli.command {
            Commands::Show {
                command: ShowCommands::Note { id },
            } => assert_eq!(id, "550e8400-e29b-41d4-a716-446655440000"),
            _ => panic!("expected show note command"),
        }
    }

    #[test]
    fn clap_parses_show_media() {
        let cli = Cli::parse_from([
            "rustygene",
            "show",
            "media",
            "550e8400-e29b-41d4-a716-446655440000",
        ]);

        match cli.command {
            Commands::Show {
                command: ShowCommands::Media { id },
            } => assert_eq!(id, "550e8400-e29b-41d4-a716-446655440000"),
            _ => panic!("expected show media command"),
        }
    }

    #[test]
    fn clap_parses_sandbox_create() {
        let cli = Cli::parse_from([
            "rustygene",
            "sandbox",
            "create",
            "--name",
            "hypothesis-a",
            "--description",
            "test branch",
        ]);

        match cli.command {
            Commands::Sandbox {
                command:
                    SandboxCommands::Create {
                        name,
                        description,
                        parent,
                    },
            } => {
                assert_eq!(name, "hypothesis-a");
                assert_eq!(description.as_deref(), Some("test branch"));
                assert!(parent.is_none());
            }
            _ => panic!("expected sandbox create command"),
        }
    }

    #[test]
    fn clap_parses_sandbox_compare() {
        let cli = Cli::parse_from([
            "rustygene",
            "sandbox",
            "compare",
            "--sandbox",
            "550e8400-e29b-41d4-a716-446655440000",
            "--entity",
            "550e8400-e29b-41d4-a716-446655440001",
            "--entity-type",
            "person",
        ]);

        match cli.command {
            Commands::Sandbox {
                command:
                    SandboxCommands::Compare {
                        sandbox, entity, ..
                    },
            } => {
                assert_eq!(sandbox, "550e8400-e29b-41d4-a716-446655440000");
                assert_eq!(entity, "550e8400-e29b-41d4-a716-446655440001");
            }
            _ => panic!("expected sandbox compare command"),
        }
    }

    #[test]
    fn clap_parses_staging_list() {
        let cli = Cli::parse_from(["rustygene", "staging", "list", "--status", "proposed"]);

        match cli.command {
            Commands::Staging {
                command: StagingCommands::List { status },
            } => {
                assert!(status.is_some());
            }
            _ => panic!("expected staging list command"),
        }
    }

    #[test]
    fn clap_parses_staging_accept_reject() {
        let accept = Cli::parse_from([
            "rustygene",
            "staging",
            "accept",
            "550e8400-e29b-41d4-a716-446655440000",
            "--reviewer",
            "cli-user",
        ]);

        match accept.command {
            Commands::Staging {
                command: StagingCommands::Accept { id, reviewer },
            } => {
                assert_eq!(id, "550e8400-e29b-41d4-a716-446655440000");
                assert_eq!(reviewer, "cli-user");
            }
            _ => panic!("expected staging accept command"),
        }

        let reject = Cli::parse_from([
            "rustygene",
            "staging",
            "reject",
            "550e8400-e29b-41d4-a716-446655440000",
            "--reviewer",
            "cli-user",
            "--reason",
            "insufficient evidence",
        ]);

        match reject.command {
            Commands::Staging {
                command:
                    StagingCommands::Reject {
                        id,
                        reviewer,
                        reason,
                    },
            } => {
                assert_eq!(id, "550e8400-e29b-41d4-a716-446655440000");
                assert_eq!(reviewer, "cli-user");
                assert_eq!(reason.as_deref(), Some("insufficient evidence"));
            }
            _ => panic!("expected staging reject command"),
        }
    }

    #[test]
    fn clap_parses_export_json_with_directory_output() {
        let cli = Cli::parse_from([
            "rustygene",
            "export",
            "--format",
            "json",
            "--output",
            "./dump",
        ]);

        match cli.command {
            Commands::Export {
                format,
                output,
                redact_living,
            } => {
                assert_eq!(format, ExportFormat::Json);
                assert_eq!(output, Some(PathBuf::from("./dump")));
                assert!(!redact_living);
            }
            _ => panic!("expected export command"),
        }
    }

    #[test]
    fn clap_parses_import_gramps() {
        let cli = Cli::parse_from(["rustygene", "import", "--format", "gramps", "sample.gramps"]);

        match cli.command {
            Commands::Import {
                format,
                merge,
                file,
                job_id,
            } => {
                assert_eq!(format, ImportFormat::Gramps);
                assert!(!merge);
                assert_eq!(file, PathBuf::from("sample.gramps"));
                assert!(job_id.is_none());
            }
            _ => panic!("expected import command"),
        }
    }

    #[test]
    fn clap_parses_import_gedcom_merge_mode() {
        let cli = Cli::parse_from([
            "rustygene",
            "import",
            "--format",
            "gedcom",
            "--merge",
            "incoming.ged",
        ]);

        match cli.command {
            Commands::Import {
                format,
                merge,
                file,
                ..
            } => {
                assert_eq!(format, ImportFormat::Gedcom);
                assert!(merge);
                assert_eq!(file, PathBuf::from("incoming.ged"));
            }
            _ => panic!("expected import command"),
        }
    }

    #[test]
    fn clap_parses_diff_command() {
        let cli = Cli::parse_from(["rustygene", "diff", "incoming.ged"]);
        match cli.command {
            Commands::Diff { file } => assert_eq!(file, PathBuf::from("incoming.ged")),
            _ => panic!("expected diff command"),
        }
    }

    #[test]
    fn merge_import_overlapping_subset_does_not_duplicate_assertions() {
        let db_path = std::env::temp_dir().join(format!(
            "rustygene-merge-cli-test-{}-{}.sqlite",
            std::process::id(),
            uuid::Uuid::new_v4().simple()
        ));

        let mut conn = Connection::open(&db_path).expect("open temp db");
        run_migrations(&mut conn).expect("run migrations");

        let gedcom = super::read_gedcom_file(Path::new("../../testdata/gedcom/simpsons.ged"))
            .expect("read simpsons fixture");
        import_gedcom_to_sqlite(&mut conn, "merge-test-initial", &gedcom)
            .expect("import initial GEDCOM");

        let persons_before: usize = conn
            .query_row("SELECT COUNT(*) FROM persons", [], |row| row.get(0))
            .expect("count persons before");
        let assertions_before: usize = conn
            .query_row("SELECT COUNT(*) FROM assertions", [], |row| row.get(0))
            .expect("count assertions before");
        drop(conn);

        let mut conn2 = Connection::open(&db_path).expect("re-open db");
        run_migrations(&mut conn2).expect("run migrations on reopen");
        let backend = SqliteBackend::new(conn2);

        let _ = super::run_import_command(
            &db_path,
            ImportFormat::Gedcom,
            true,
            Path::new("../../testdata/gedcom/simpsons.ged"),
            None,
            &backend,
        );

        let conn3 = Connection::open(&db_path).expect("open db for verification");
        let persons_after: usize = conn3
            .query_row("SELECT COUNT(*) FROM persons", [], |row| row.get(0))
            .expect("count persons after");
        let assertions_after: usize = conn3
            .query_row("SELECT COUNT(*) FROM assertions", [], |row| row.get(0))
            .expect("count assertions after");

        assert_eq!(persons_after, persons_before);
        assert_eq!(assertions_after, assertions_before);

        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn clap_parses_export_gedcom_with_redaction() {
        let cli = Cli::parse_from([
            "rustygene",
            "export",
            "--format",
            "gedcom",
            "--redact-living",
        ]);

        match cli.command {
            Commands::Export {
                format,
                output,
                redact_living,
            } => {
                assert_eq!(format, ExportFormat::Gedcom);
                assert_eq!(output, None);
                assert!(redact_living);
            }
            _ => panic!("expected export command"),
        }
    }

    #[test]
    fn preserved_or_generated_xref_prefers_original_id() {
        assert_eq!(preserved_or_generated_xref(Some("@I23@"), 'I', 0), "@I23@");
    }

    #[test]
    fn preserved_or_generated_xref_falls_back_to_sequential_id() {
        assert_eq!(preserved_or_generated_xref(None, 'F', 2), "@F3@");
    }
}
