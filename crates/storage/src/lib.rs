pub mod sqlite_impl;

use refinery::embed_migrations;
use rusqlite::Connection;
use rustygene_core::assertion::{Assertion, AssertionStatus};
use rustygene_core::event::Event;
use rustygene_core::evidence::{Citation, Media, Note, Repository, Source};
use rustygene_core::family::{Family, Relationship};
use rustygene_core::lds::LdsOrdinance;
use rustygene_core::person::Person;
use rustygene_core::place::Place;
use rustygene_core::research::{ResearchLogEntry, SearchResult};
use rustygene_core::types::EntityId;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;

embed_migrations!("migrations");

pub const REQUIRED_SCHEMA_TABLES: &[&str] = &[
    "persons",
    "families",
    "family_relationships",
    "events",
    "places",
    "sources",
    "citations",
    "repositories",
    "media",
    "notes",
    "lds_ordinances",
    "assertions",
    "relationships",
    "audit_log",
    "event_log",
    "research_log",
    "sandboxes",
    "agents",
    "search_index",
];

pub const REQUIRED_SCHEMA_INDEXES: &[&str] = &[
    "idx_assertions_entity_field",
    "idx_assertions_date",
    "idx_assertions_status",
    "idx_assertions_confidence",
    "idx_assertions_sandbox",
    "idx_event_log_type_time",
    "idx_research_log_date",
    "idx_research_log_result",
    "idx_relationships_from_type",
    "idx_relationships_to_type",
];

pub fn run_migrations(connection: &mut Connection) -> Result<refinery::Report, refinery::Error> {
    migrations::runner().run(connection)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageError {
    pub code: StorageErrorCode,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageErrorCode {
    NotFound,
    Conflict,
    Validation,
    Serialization,
    Backend,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pagination {
    pub limit: u32,
    pub offset: u32,
}

impl Default for Pagination {
    fn default() -> Self {
        Self {
            limit: 100,
            offset: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResearchLogFilter {
    pub person_ref: Option<EntityId>,
    pub result: Option<SearchResult>,
    pub date_from_iso: Option<String>,
    pub date_to_iso: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityType {
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditLogEntry {
    pub actor: String,
    pub entity_id: EntityId,
    pub entity_type: EntityType,
    pub action: String,
    pub old_value_json: Option<Value>,
    pub new_value_json: Option<Value>,
    pub timestamp_iso: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationshipEdge {
    pub from_entity: EntityId,
    pub to_entity: EntityId,
    pub rel_type: String,
    pub directed: bool,
    pub assertion_id: Option<EntityId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JsonExportMode {
    Directory { output_dir: PathBuf },
    SingleFile { output_file: PathBuf },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JsonImportMode {
    Directory { input_dir: PathBuf },
    SingleFile { input_file: PathBuf },
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct JsonExportManifest {
    pub exported_at: String,
    pub schema_version: i64,
    pub entity_counts: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonExportResult {
    pub manifest: JsonExportManifest,
    pub output_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonImportReport {
    pub manifest: JsonExportManifest,
    pub entities_imported_by_type: BTreeMap<String, usize>,
    pub assertions_imported: usize,
    pub audit_log_entries_imported: usize,
    pub research_log_entries_imported: usize,
}

pub type JsonAssertion = Assertion<Value>;

/// Storage abstraction used by the core and API layers.
///
/// Concrete backends (SQLite now, PostgreSQL later) implement this trait.
#[async_trait::async_trait]
pub trait Storage {
    async fn create_person(&self, person: &Person) -> Result<(), StorageError>;
    async fn get_person(&self, id: EntityId) -> Result<Person, StorageError>;
    async fn update_person(&self, person: &Person) -> Result<(), StorageError>;
    async fn delete_person(&self, id: EntityId) -> Result<(), StorageError>;
    async fn list_persons(&self, pagination: Pagination) -> Result<Vec<Person>, StorageError>;

    async fn create_family(&self, family: &Family) -> Result<(), StorageError>;
    async fn get_family(&self, id: EntityId) -> Result<Family, StorageError>;
    async fn update_family(&self, family: &Family) -> Result<(), StorageError>;
    async fn delete_family(&self, id: EntityId) -> Result<(), StorageError>;
    async fn list_families(&self, pagination: Pagination) -> Result<Vec<Family>, StorageError>;

    async fn create_relationship(&self, relationship: &Relationship) -> Result<(), StorageError>;
    async fn get_relationship(&self, id: EntityId) -> Result<Relationship, StorageError>;
    async fn update_relationship(&self, relationship: &Relationship) -> Result<(), StorageError>;
    async fn delete_relationship(&self, id: EntityId) -> Result<(), StorageError>;
    async fn list_relationships(
        &self,
        pagination: Pagination,
    ) -> Result<Vec<Relationship>, StorageError>;

    async fn create_event(&self, event: &Event) -> Result<(), StorageError>;
    async fn get_event(&self, id: EntityId) -> Result<Event, StorageError>;
    async fn update_event(&self, event: &Event) -> Result<(), StorageError>;
    async fn delete_event(&self, id: EntityId) -> Result<(), StorageError>;
    async fn list_events(&self, pagination: Pagination) -> Result<Vec<Event>, StorageError>;

    async fn create_place(&self, place: &Place) -> Result<(), StorageError>;
    async fn get_place(&self, id: EntityId) -> Result<Place, StorageError>;
    async fn update_place(&self, place: &Place) -> Result<(), StorageError>;
    async fn delete_place(&self, id: EntityId) -> Result<(), StorageError>;
    async fn list_places(&self, pagination: Pagination) -> Result<Vec<Place>, StorageError>;

    async fn create_repository(&self, repository: &Repository) -> Result<(), StorageError>;
    async fn get_repository(&self, id: EntityId) -> Result<Repository, StorageError>;
    async fn update_repository(&self, repository: &Repository) -> Result<(), StorageError>;
    async fn delete_repository(&self, id: EntityId) -> Result<(), StorageError>;
    async fn list_repositories(
        &self,
        pagination: Pagination,
    ) -> Result<Vec<Repository>, StorageError>;

    async fn create_source(&self, source: &Source) -> Result<(), StorageError>;
    async fn get_source(&self, id: EntityId) -> Result<Source, StorageError>;
    async fn update_source(&self, source: &Source) -> Result<(), StorageError>;
    async fn delete_source(&self, id: EntityId) -> Result<(), StorageError>;
    async fn list_sources(&self, pagination: Pagination) -> Result<Vec<Source>, StorageError>;

    async fn create_citation(&self, citation: &Citation) -> Result<(), StorageError>;
    async fn get_citation(&self, id: EntityId) -> Result<Citation, StorageError>;
    async fn update_citation(&self, citation: &Citation) -> Result<(), StorageError>;
    async fn delete_citation(&self, id: EntityId) -> Result<(), StorageError>;
    async fn list_citations(&self, pagination: Pagination) -> Result<Vec<Citation>, StorageError>;

    async fn create_media(&self, media: &Media) -> Result<(), StorageError>;
    async fn get_media(&self, id: EntityId) -> Result<Media, StorageError>;
    async fn update_media(&self, media: &Media) -> Result<(), StorageError>;
    async fn delete_media(&self, id: EntityId) -> Result<(), StorageError>;
    async fn list_media(&self, pagination: Pagination) -> Result<Vec<Media>, StorageError>;

    async fn create_note(&self, note: &Note) -> Result<(), StorageError>;
    async fn get_note(&self, id: EntityId) -> Result<Note, StorageError>;
    async fn update_note(&self, note: &Note) -> Result<(), StorageError>;
    async fn delete_note(&self, id: EntityId) -> Result<(), StorageError>;
    async fn list_notes(&self, pagination: Pagination) -> Result<Vec<Note>, StorageError>;

    async fn create_lds_ordinance(&self, ordinance: &LdsOrdinance) -> Result<(), StorageError>;
    async fn get_lds_ordinance(&self, id: EntityId) -> Result<LdsOrdinance, StorageError>;
    async fn update_lds_ordinance(&self, ordinance: &LdsOrdinance) -> Result<(), StorageError>;
    async fn delete_lds_ordinance(&self, id: EntityId) -> Result<(), StorageError>;
    async fn list_lds_ordinances(
        &self,
        pagination: Pagination,
    ) -> Result<Vec<LdsOrdinance>, StorageError>;

    async fn create_assertion(
        &self,
        entity_id: EntityId,
        entity_type: EntityType,
        field: &str,
        assertion: &JsonAssertion,
    ) -> Result<(), StorageError>;
    async fn list_assertions_for_entity(
        &self,
        entity_id: EntityId,
    ) -> Result<Vec<JsonAssertion>, StorageError>;
    async fn list_assertions_for_field(
        &self,
        entity_id: EntityId,
        field: &str,
    ) -> Result<Vec<JsonAssertion>, StorageError>;
    async fn update_assertion_status(
        &self,
        assertion_id: EntityId,
        status: AssertionStatus,
    ) -> Result<(), StorageError>;

    async fn create_research_log_entry(&self, entry: &ResearchLogEntry)
    -> Result<(), StorageError>;
    async fn get_research_log_entry(&self, id: EntityId) -> Result<ResearchLogEntry, StorageError>;
    async fn delete_research_log_entry(&self, id: EntityId) -> Result<(), StorageError>;
    async fn list_research_log_entries(
        &self,
        filter: &ResearchLogFilter,
        pagination: Pagination,
    ) -> Result<Vec<ResearchLogEntry>, StorageError>;

    async fn append_audit_log_entry(&self, entry: &AuditLogEntry) -> Result<(), StorageError>;

    async fn upsert_relationship_edge(&self, edge: &RelationshipEdge) -> Result<(), StorageError>;
    async fn list_relationship_edges_for_entity(
        &self,
        entity_id: EntityId,
    ) -> Result<Vec<RelationshipEdge>, StorageError>;
    async fn ancestors(
        &self,
        person_id: EntityId,
        max_depth: u32,
    ) -> Result<Vec<EntityId>, StorageError>;
    async fn descendants(
        &self,
        person_id: EntityId,
        max_depth: u32,
    ) -> Result<Vec<EntityId>, StorageError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn pagination_defaults_are_stable() {
        let p = Pagination::default();
        assert_eq!(p.limit, 100);
        assert_eq!(p.offset, 0);
    }

    #[test]
    fn runs_initial_migration_on_fresh_database() {
        let mut connection = Connection::open_in_memory().expect("open in-memory sqlite database");
        let report = run_migrations(&mut connection).expect("run embedded refinery migrations");
        assert!(!report.applied_migrations().is_empty());

        let tables: HashSet<String> = {
            let mut statement = connection
                .prepare("SELECT name FROM sqlite_master WHERE type = 'table'")
                .expect("prepare sqlite_master table query");
            statement
                .query_map([], |row| row.get::<_, String>(0))
                .expect("query sqlite_master tables")
                .collect::<Result<HashSet<_>, _>>()
                .expect("collect table names")
        };

        for table in REQUIRED_SCHEMA_TABLES {
            assert!(
                tables.contains(*table),
                "expected table '{table}' to exist after migration"
            );
        }

        let indexes: HashSet<String> = {
            let mut statement = connection
                .prepare("SELECT name FROM sqlite_master WHERE type = 'index'")
                .expect("prepare sqlite_master index query");
            statement
                .query_map([], |row| row.get::<_, String>(0))
                .expect("query sqlite_master indexes")
                .collect::<Result<HashSet<_>, _>>()
                .expect("collect index names")
        };

        for index in REQUIRED_SCHEMA_INDEXES {
            assert!(
                indexes.contains(*index),
                "expected index '{index}' to exist after migration"
            );
        }
    }
}
