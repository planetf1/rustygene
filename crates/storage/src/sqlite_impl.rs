use crate::{
    AuditLogEntry, EntityType, JsonAssertion, JsonExportManifest, JsonExportMode,
    JsonExportResult, Pagination, RelationshipEdge, ResearchLogFilter, Storage, StorageError,
    StorageErrorCode,
};
use rustygene_core::assertion::{compute_assertion_idempotency_key, AssertionStatus, EvidenceType};
use rustygene_core::evidence::{Citation, Media, Note, Repository, Source};
use rustygene_core::event::Event;
use rustygene_core::family::{Family, Relationship};
use rustygene_core::lds::LdsOrdinance;
use rustygene_core::place::Place;
use rustygene_core::person::Person;
use rustygene_core::research::{ResearchLogEntry, SearchResult};
use rustygene_core::types::EntityId;
use rusqlite::{Connection, OptionalExtension, Result as SqliteResult};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::str::FromStr;
use std::sync::{Arc, Mutex};

/// SQLite-backed implementation of the Storage trait.
pub struct SqliteBackend {
    connection: Arc<Mutex<Connection>>,
}

struct AssertionRowData {
    id_str: String,
    value_text: String,
    confidence: f64,
    status_text: String,
    evidence_type_text: String,
    source_citations_text: Option<String>,
    proposed_by_text: String,
    created_at_text: String,
    reviewed_at_text: Option<String>,
    reviewed_by_text: Option<String>,
}

struct ResearchRowData {
    id_text: String,
    date_text: String,
    objective: String,
    repository_id: Option<String>,
    repository_name: Option<String>,
    search_terms: String,
    source_id: Option<String>,
    result: String,
    findings: Option<String>,
    citations_created: Option<String>,
    next_steps: Option<String>,
    person_refs: Option<String>,
    tags: Option<String>,
}

impl SqliteBackend {
    pub fn new(connection: Connection) -> Self {
        Self {
            connection: Arc::new(Mutex::new(connection)),
        }
    }

    pub fn rebuild_all_snapshots(&self) -> Result<usize, StorageError> {
        let mut conn = self
            .connection
            .lock()
            .map_err(|e| StorageError {
                code: StorageErrorCode::Backend,
                message: format!("Mutex lock failed: {}", e),
            })?;

        let tx = conn.transaction().map_err(|e| StorageError {
            code: StorageErrorCode::Backend,
            message: format!("Transaction begin failed: {}", e),
        })?;

        let mut stmt = tx
            .prepare("SELECT DISTINCT entity_id, entity_type FROM assertions")
            .map_err(|e| StorageError {
                code: StorageErrorCode::Backend,
                message: format!("Prepare rebuild query failed: {}", e),
            })?;

        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| StorageError {
                code: StorageErrorCode::Backend,
                message: format!("Rebuild query failed: {}", e),
            })?
            .collect::<SqliteResult<Vec<_>>>()
            .map_err(|e| StorageError {
                code: StorageErrorCode::Backend,
                message: format!("Rebuild row collection failed: {}", e),
            })?;

        drop(stmt);

        let mut rebuilt = 0usize;
        for (entity_id_text, entity_type_text) in rows {
            let entity_id = Self::parse_entity_id_str(&entity_id_text)?;
            let entity_type = Self::entity_type_from_db(&entity_type_text)?;
            Self::recompute_entity_snapshot_tx(&tx, entity_id, entity_type)?;
            rebuilt += 1;
        }

        tx.commit().map_err(|e| StorageError {
            code: StorageErrorCode::Backend,
            message: format!("Transaction commit failed: {}", e),
        })?;

        Ok(rebuilt)
    }

    pub fn export_json_dump(&self, mode: JsonExportMode) -> Result<JsonExportResult, StorageError> {
        let conn = self
            .connection
            .lock()
            .map_err(|e| StorageError {
                code: StorageErrorCode::Backend,
                message: format!("Mutex lock failed: {}", e),
            })?;

        let mut payload = serde_json::Map::new();
        let mut entity_counts: BTreeMap<String, usize> = BTreeMap::new();

        let entity_tables = [
            ("persons", true),
            ("families", true),
            ("events", true),
            ("places", true),
            ("sources", true),
            ("citations", true),
            ("repositories", true),
            ("media", true),
            ("notes", true),
            ("lds_ordinances", true),
            ("assertions", false),
            ("relationships", false),
            ("audit_log", false),
            ("research_log", false),
        ];

        for (table, is_entity_snapshot_table) in entity_tables {
            let rows = if is_entity_snapshot_table {
                Self::query_entity_snapshot_rows(&conn, table)?
            } else {
                Self::query_raw_rows(&conn, table)?
            };
            entity_counts.insert(table.to_string(), rows.len());
            payload.insert(table.to_string(), Value::Array(rows));
        }

        let schema_version: i64 = conn
            .query_row("SELECT COALESCE(MAX(version), 0) FROM refinery_schema_history", [], |row| {
                row.get(0)
            })
            .unwrap_or(0);

        let manifest = JsonExportManifest {
            exported_at: chrono::Utc::now().to_rfc3339(),
            schema_version,
            entity_counts,
        };

        payload.insert(
            "manifest".to_string(),
            serde_json::to_value(&manifest).map_err(|e| StorageError {
                code: StorageErrorCode::Serialization,
                message: format!("Manifest serialization failed: {}", e),
            })?,
        );

        drop(conn);

        let root = Value::Object(payload);

        match mode {
            JsonExportMode::Directory { output_dir } => {
                fs::create_dir_all(&output_dir).map_err(|e| StorageError {
                    code: StorageErrorCode::Backend,
                    message: format!("Failed to create export directory: {}", e),
                })?;

                write_json_file(output_dir.join("manifest.json"), &manifest)?;

                let root_obj = root.as_object().ok_or(StorageError {
                    code: StorageErrorCode::Serialization,
                    message: "Export root payload must be a JSON object".to_string(),
                })?;

                for (name, value) in root_obj {
                    if name == "manifest" {
                        continue;
                    }
                    write_json_file(output_dir.join(format!("{}.json", name)), value)?;
                }

                Ok(JsonExportResult {
                    manifest,
                    output_path: output_dir,
                })
            }
            JsonExportMode::SingleFile { output_file } => {
                if let Some(parent) = output_file.parent()
                    && !parent.as_os_str().is_empty()
                {
                    fs::create_dir_all(parent).map_err(|e| StorageError {
                        code: StorageErrorCode::Backend,
                        message: format!("Failed to create export parent directory: {}", e),
                    })?;
                }

                write_json_file(&output_file, &root)?;

                Ok(JsonExportResult {
                    manifest,
                    output_path: output_file,
                })
            }
        }
    }

    fn query_entity_snapshot_rows(conn: &Connection, table: &str) -> Result<Vec<Value>, StorageError> {
        let mut stmt = conn
            .prepare(&format!("SELECT data FROM {} ORDER BY created_at DESC", table))
            .map_err(|e| StorageError {
                code: StorageErrorCode::Backend,
                message: format!("Prepare {} query failed: {}", table, e),
            })?;

        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| StorageError {
                code: StorageErrorCode::Backend,
                message: format!("Query {} failed: {}", table, e),
            })?
            .collect::<SqliteResult<Vec<_>>>()
            .map_err(|e| StorageError {
                code: StorageErrorCode::Backend,
                message: format!("Collect {} failed: {}", table, e),
            })?;

        rows.into_iter()
            .map(|row| {
                serde_json::from_str::<Value>(&row).map_err(|e| StorageError {
                    code: StorageErrorCode::Serialization,
                    message: format!("{} JSON parse failed: {}", table, e),
                })
            })
            .collect()
    }

    fn query_raw_rows(conn: &Connection, table: &str) -> Result<Vec<Value>, StorageError> {
        let mut stmt = conn
            .prepare(&format!("SELECT * FROM {}", table))
            .map_err(|e| StorageError {
                code: StorageErrorCode::Backend,
                message: format!("Prepare {} query failed: {}", table, e),
            })?;

        let column_names: Vec<String> = stmt
            .column_names()
            .iter()
            .map(|name| (*name).to_string())
            .collect();

        let mut rows = stmt.query([]).map_err(|e| StorageError {
            code: StorageErrorCode::Backend,
            message: format!("Query {} failed: {}", table, e),
        })?;

        let mut output = Vec::new();
        while let Some(row) = rows.next().map_err(|e| StorageError {
            code: StorageErrorCode::Backend,
            message: format!("Row iteration for {} failed: {}", table, e),
        })? {
            let mut obj = serde_json::Map::new();
            for (idx, column_name) in column_names.iter().enumerate() {
                let as_string: Result<Option<String>, _> = row.get(idx);
                match as_string {
                    Ok(Some(v)) => {
                        let parsed = serde_json::from_str::<Value>(&v).unwrap_or(Value::String(v));
                        obj.insert(column_name.clone(), parsed);
                    }
                    Ok(None) => {
                        obj.insert(column_name.clone(), Value::Null);
                    }
                    Err(_) => {
                        let as_i64: Result<Option<i64>, _> = row.get(idx);
                        if let Ok(Some(v)) = as_i64 {
                            obj.insert(column_name.clone(), Value::Number(v.into()));
                            continue;
                        }

                        let as_f64: Result<Option<f64>, _> = row.get(idx);
                        if let Ok(Some(v)) = as_f64 {
                            if let Some(n) = serde_json::Number::from_f64(v) {
                                obj.insert(column_name.clone(), Value::Number(n));
                            } else {
                                obj.insert(column_name.clone(), Value::Null);
                            }
                            continue;
                        }

                        obj.insert(column_name.clone(), Value::Null);
                    }
                }
            }
            output.push(Value::Object(obj));
        }

        Ok(output)
    }

    fn serialize<T: serde::Serialize>(entity: &T) -> Result<Value, StorageError> {
        serde_json::to_value(entity).map_err(|e| StorageError {
            code: StorageErrorCode::Serialization,
            message: format!("Serialization failed: {}", e),
        })
    }

    fn deserialize<T: serde::de::DeserializeOwned>(value: &Value) -> Result<T, StorageError> {
        serde_json::from_value(value.clone()).map_err(|e| StorageError {
            code: StorageErrorCode::Serialization,
            message: format!("Deserialization failed: {}", e),
        })
    }

    fn insert_sync(&self, table: &str, id: EntityId, data: &Value) -> Result<(), StorageError> {
        let conn = self
            .connection
            .lock()
            .map_err(|e| StorageError {
                code: StorageErrorCode::Backend,
                message: format!("Mutex lock failed: {}", e),
            })?;

        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            &format!(
                "INSERT INTO {} (id, version, schema_version, data, created_at, updated_at) VALUES (?, 1, 1, ?, ?, ?)",
                table
            ),
            rusqlite::params![
                id.to_string(),
                data.to_string(),
                &now,
                &now
            ],
        )
        .map_err(|e| StorageError {
            code: StorageErrorCode::Backend,
            message: format!("Insert failed: {}", e),
        })?;

        Ok(())
    }

    fn get_sync<T: serde::de::DeserializeOwned>(
        &self,
        table: &str,
        id: EntityId,
    ) -> Result<T, StorageError> {
        let conn = self
            .connection
            .lock()
            .map_err(|e| StorageError {
                code: StorageErrorCode::Backend,
                message: format!("Mutex lock failed: {}", e),
            })?;

        let mut stmt = conn
            .prepare(&format!("SELECT data FROM {} WHERE id = ?", table))
            .map_err(|e| StorageError {
                code: StorageErrorCode::Backend,
                message: format!("Prepare failed: {}", e),
            })?;

        let data_str: String = stmt
            .query_row(rusqlite::params![id.to_string()], |row| row.get(0))
            .optional()
            .map_err(|e| StorageError {
                code: StorageErrorCode::Backend,
                message: format!("Query failed: {}", e),
            })?
            .ok_or(StorageError {
                code: StorageErrorCode::NotFound,
                message: format!("{} not found with id {}", table, id),
            })?;

        let value: Value = serde_json::from_str(&data_str).map_err(|e| StorageError {
            code: StorageErrorCode::Serialization,
            message: format!("JSON parse failed: {}", e),
        })?;

        Self::deserialize(&value)
    }

    fn update_sync(&self, table: &str, id: EntityId, data: &Value) -> Result<(), StorageError> {
        let conn = self
            .connection
            .lock()
            .map_err(|e| StorageError {
                code: StorageErrorCode::Backend,
                message: format!("Mutex lock failed: {}", e),
            })?;

        // Get version first
        let version: u32 = {
            let mut stmt = conn
                .prepare(&format!("SELECT version FROM {} WHERE id = ?", table))
                .map_err(|e| StorageError {
                    code: StorageErrorCode::Backend,
                    message: format!("Prepare failed: {}", e),
                })?;

            stmt.query_row(rusqlite::params![id.to_string()], |row| row.get(0))
                .optional()
                .map_err(|e| StorageError {
                    code: StorageErrorCode::Backend,
                    message: format!("Query failed: {}", e),
                })?
                .ok_or(StorageError {
                    code: StorageErrorCode::NotFound,
                    message: format!("{} not found with id {}", table, id),
                })?
        };

        let now = chrono::Utc::now().to_rfc3339();
        let rows = conn
            .execute(
                &format!(
                    "UPDATE {} SET data = ?, version = version + 1, updated_at = ? WHERE id = ? AND version = ?",
                    table
                ),
                rusqlite::params![
                    data.to_string(),
                    &now,
                    id.to_string(),
                    version
                ],
            )
            .map_err(|e| StorageError {
                code: StorageErrorCode::Backend,
                message: format!("Update failed: {}", e),
            })?;

        if rows == 0 {
            return Err(StorageError {
                code: StorageErrorCode::Conflict,
                message: "Version conflict".to_string(),
            });
        }

        Ok(())
    }

    fn delete_sync(&self, table: &str, id: EntityId) -> Result<(), StorageError> {
        let conn = self
            .connection
            .lock()
            .map_err(|e| StorageError {
                code: StorageErrorCode::Backend,
                message: format!("Mutex lock failed: {}", e),
            })?;

        conn.execute(
            &format!("DELETE FROM {} WHERE id = ?", table),
            rusqlite::params![id.to_string()],
        )
        .map_err(|e| StorageError {
            code: StorageErrorCode::Backend,
            message: format!("Delete failed: {}", e),
        })?;

        Ok(())
    }

    fn list_sync<T: serde::de::DeserializeOwned>(
        &self,
        table: &str,
        pagination: Pagination,
    ) -> Result<Vec<T>, StorageError> {
        let conn = self
            .connection
            .lock()
            .map_err(|e| StorageError {
                code: StorageErrorCode::Backend,
                message: format!("Mutex lock failed: {}", e),
            })?;

        let mut stmt = conn
            .prepare(&format!(
                "SELECT data FROM {} ORDER BY created_at DESC LIMIT ? OFFSET ?",
                table
            ))
            .map_err(|e| StorageError {
                code: StorageErrorCode::Backend,
                message: format!("Prepare failed: {}", e),
            })?;

        let rows: Vec<String> = stmt
            .query_map(
                rusqlite::params![pagination.limit as i32, pagination.offset as i32],
                |row| row.get(0),
            )
            .map_err(|e| StorageError {
                code: StorageErrorCode::Backend,
                message: format!("Query failed: {}", e),
            })?
            .collect::<SqliteResult<Vec<_>>>()
            .map_err(|e| StorageError {
                code: StorageErrorCode::Backend,
                message: format!("Collect failed: {}", e),
            })?;

        let mut result = Vec::new();
        for s in rows {
            let v: Value = serde_json::from_str(&s).map_err(|e| StorageError {
                code: StorageErrorCode::Serialization,
                message: format!("JSON parse failed: {}", e),
            })?;
            result.push(Self::deserialize(&v)?);
        }

        Ok(result)
    }

    fn entity_type_to_db(entity_type: EntityType) -> &'static str {
        match entity_type {
            EntityType::Person => "person",
            EntityType::Family => "family",
            EntityType::Relationship => "relationship",
            EntityType::Event => "event",
            EntityType::Place => "place",
            EntityType::Source => "source",
            EntityType::Citation => "citation",
            EntityType::Repository => "repository",
            EntityType::Media => "media",
            EntityType::Note => "note",
            EntityType::LdsOrdinance => "lds_ordinance",
        }
    }

    fn entity_table_for_type(entity_type: EntityType) -> &'static str {
        match entity_type {
            EntityType::Person => "persons",
            EntityType::Family => "families",
            EntityType::Relationship => "families",
            EntityType::Event => "events",
            EntityType::Place => "places",
            EntityType::Source => "sources",
            EntityType::Citation => "citations",
            EntityType::Repository => "repositories",
            EntityType::Media => "media",
            EntityType::Note => "notes",
            EntityType::LdsOrdinance => "lds_ordinances",
        }
    }

    fn entity_type_from_db(value: &str) -> Result<EntityType, StorageError> {
        match value {
            "person" => Ok(EntityType::Person),
            "family" => Ok(EntityType::Family),
            "relationship" => Ok(EntityType::Relationship),
            "event" => Ok(EntityType::Event),
            "place" => Ok(EntityType::Place),
            "source" => Ok(EntityType::Source),
            "citation" => Ok(EntityType::Citation),
            "repository" => Ok(EntityType::Repository),
            "media" => Ok(EntityType::Media),
            "note" => Ok(EntityType::Note),
            "lds_ordinance" => Ok(EntityType::LdsOrdinance),
            other => Err(StorageError {
                code: StorageErrorCode::Serialization,
                message: format!("Unknown entity type in assertions table: {}", other),
            }),
        }
    }

    fn assertion_status_to_db(status: &AssertionStatus) -> &'static str {
        match status {
            AssertionStatus::Confirmed => "confirmed",
            AssertionStatus::Proposed => "proposed",
            AssertionStatus::Disputed => "disputed",
            AssertionStatus::Rejected => "rejected",
        }
    }

    fn assertion_status_from_db(status: &str) -> Result<AssertionStatus, StorageError> {
        match status {
            "confirmed" => Ok(AssertionStatus::Confirmed),
            "proposed" => Ok(AssertionStatus::Proposed),
            "disputed" => Ok(AssertionStatus::Disputed),
            "rejected" => Ok(AssertionStatus::Rejected),
            other => Err(StorageError {
                code: StorageErrorCode::Serialization,
                message: format!("Unknown assertion status: {}", other),
            }),
        }
    }

    fn evidence_type_to_db(evidence_type: &EvidenceType) -> &'static str {
        match evidence_type {
            EvidenceType::Direct => "direct",
            EvidenceType::Indirect => "indirect",
            EvidenceType::Negative => "negative",
        }
    }

    fn evidence_type_from_db(evidence_type: &str) -> Result<EvidenceType, StorageError> {
        match evidence_type {
            "direct" => Ok(EvidenceType::Direct),
            "indirect" => Ok(EvidenceType::Indirect),
            "negative" => Ok(EvidenceType::Negative),
            other => Err(StorageError {
                code: StorageErrorCode::Serialization,
                message: format!("Unknown evidence type: {}", other),
            }),
        }
    }

    fn search_result_to_db(result: &SearchResult) -> &'static str {
        match result {
            SearchResult::Found => "found",
            SearchResult::NotFound => "not_found",
            SearchResult::PartiallyFound => "partially_found",
            SearchResult::Inconclusive => "inconclusive",
        }
    }

    fn search_result_from_db(result: &str) -> Result<SearchResult, StorageError> {
        match result {
            "found" => Ok(SearchResult::Found),
            "not_found" => Ok(SearchResult::NotFound),
            "partially_found" => Ok(SearchResult::PartiallyFound),
            "inconclusive" => Ok(SearchResult::Inconclusive),
            other => Err(StorageError {
                code: StorageErrorCode::Serialization,
                message: format!("Unknown search result: {}", other),
            }),
        }
    }

    fn parse_entity_id_str(value: &str) -> Result<EntityId, StorageError> {
        serde_json::from_str(&format!("\"{}\"", value)).map_err(|e| StorageError {
            code: StorageErrorCode::Serialization,
            message: format!("Invalid entity id '{}': {}", value, e),
        })
    }

    fn research_row_to_entry(data: ResearchRowData) -> Result<ResearchLogEntry, StorageError> {
        Ok(ResearchLogEntry {
            id: Self::parse_entity_id_str(&data.id_text)?,
            date: chrono::DateTime::parse_from_rfc3339(&data.date_text)
                .map_err(|e| StorageError {
                    code: StorageErrorCode::Serialization,
                    message: format!("Invalid research log date '{}': {}", data.date_text, e),
                })?
                .with_timezone(&chrono::Utc),
            objective: data.objective,
            repository: data
                .repository_id
                .as_deref()
                .map(Self::parse_entity_id_str)
                .transpose()?,
            repository_name: data.repository_name,
            search_terms: serde_json::from_str(&data.search_terms).map_err(|e| StorageError {
                code: StorageErrorCode::Serialization,
                message: format!("Invalid search_terms JSON: {}", e),
            })?,
            source_searched: data
                .source_id
                .as_deref()
                .map(Self::parse_entity_id_str)
                .transpose()?,
            result: Self::search_result_from_db(&data.result)?,
            findings: data.findings,
            citations_created: serde_json::from_str(
                data.citations_created.as_deref().unwrap_or("[]"),
            )
            .map_err(|e| StorageError {
                code: StorageErrorCode::Serialization,
                message: format!("Invalid citations_created JSON: {}", e),
            })?,
            next_steps: data.next_steps,
            person_refs: serde_json::from_str(data.person_refs.as_deref().unwrap_or("[]"))
                .map_err(|e| StorageError {
                    code: StorageErrorCode::Serialization,
                    message: format!("Invalid person_refs JSON: {}", e),
                })?,
            tags: serde_json::from_str(data.tags.as_deref().unwrap_or("[]")).map_err(|e| {
                StorageError {
                    code: StorageErrorCode::Serialization,
                    message: format!("Invalid tags JSON: {}", e),
                }
            })?,
        })
    }

    fn row_to_assertion(data: AssertionRowData) -> Result<JsonAssertion, StorageError> {
        let AssertionRowData {
            id_str,
            value_text,
            confidence,
            status_text,
            evidence_type_text,
            source_citations_text,
            proposed_by_text,
            created_at_text,
            reviewed_at_text,
            reviewed_by_text,
        } = data;

        let id: EntityId = serde_json::from_str(&format!("\"{}\"", id_str)).map_err(|e| StorageError {
            code: StorageErrorCode::Serialization,
            message: format!("Invalid assertion id '{}': {}", id_str, e),
        })?;
        let value: Value = serde_json::from_str(&value_text).map_err(|e| StorageError {
            code: StorageErrorCode::Serialization,
            message: format!("Invalid assertion value JSON: {}", e),
        })?;
        let status = Self::assertion_status_from_db(&status_text)?;
        let evidence_type = Self::evidence_type_from_db(&evidence_type_text)?;
        let source_citations = match source_citations_text {
            Some(raw) => serde_json::from_str(&raw).map_err(|e| StorageError {
                code: StorageErrorCode::Serialization,
                message: format!("Invalid source_citations JSON: {}", e),
            })?,
            None => Vec::new(),
        };
        let proposed_by = rustygene_core::types::ActorRef::from_str(&proposed_by_text).map_err(|e| {
            StorageError {
                code: StorageErrorCode::Serialization,
                message: format!("Invalid proposed_by '{}': {}", proposed_by_text, e),
            }
        })?;
        let created_at = chrono::DateTime::parse_from_rfc3339(&created_at_text)
            .map_err(|e| StorageError {
                code: StorageErrorCode::Serialization,
                message: format!("Invalid created_at '{}': {}", created_at_text, e),
            })?
            .with_timezone(&chrono::Utc);
        let reviewed_at = match reviewed_at_text {
            Some(ts) => Some(
                chrono::DateTime::parse_from_rfc3339(&ts)
                    .map_err(|e| StorageError {
                        code: StorageErrorCode::Serialization,
                        message: format!("Invalid reviewed_at '{}': {}", ts, e),
                    })?
                    .with_timezone(&chrono::Utc),
            ),
            None => None,
        };
        let reviewed_by = match reviewed_by_text {
            Some(actor) => Some(
                rustygene_core::types::ActorRef::from_str(&actor).map_err(|e| StorageError {
                    code: StorageErrorCode::Serialization,
                    message: format!("Invalid reviewed_by '{}': {}", actor, e),
                })?,
            ),
            None => None,
        };

        Ok(JsonAssertion {
            id,
            value,
            confidence,
            status,
            evidence_type,
            source_citations,
            proposed_by,
            created_at,
            reviewed_at,
            reviewed_by,
        })
    }

    fn recompute_entity_snapshot_tx(
        tx: &rusqlite::Transaction<'_>,
        entity_id: EntityId,
        entity_type: EntityType,
    ) -> Result<(), StorageError> {
        let table = Self::entity_table_for_type(entity_type);
        let current_data_str: Option<String> = tx
            .query_row(
                &format!("SELECT data FROM {} WHERE id = ?", table),
                rusqlite::params![entity_id.to_string()],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| StorageError {
                code: StorageErrorCode::Backend,
                message: format!("Snapshot read failed: {}", e),
            })?;

        let current_data_str = current_data_str.ok_or(StorageError {
            code: StorageErrorCode::NotFound,
            message: format!("Entity {} not found in table {}", entity_id, table),
        })?;

        let mut snapshot_json: Value = serde_json::from_str(&current_data_str).map_err(|e| StorageError {
            code: StorageErrorCode::Serialization,
            message: format!("Existing snapshot JSON parse failed: {}", e),
        })?;

        let obj = snapshot_json.as_object_mut().ok_or(StorageError {
            code: StorageErrorCode::Serialization,
            message: "Entity snapshot is not a JSON object".to_string(),
        })?;

        let mut stmt = tx
            .prepare(
                "SELECT field, value
                 FROM assertions
                 WHERE entity_id = ? AND status = 'confirmed' AND preferred = 1",
            )
            .map_err(|e| StorageError {
                code: StorageErrorCode::Backend,
                message: format!("Snapshot assertion query prepare failed: {}", e),
            })?;

        let rows = stmt
            .query_map(rusqlite::params![entity_id.to_string()], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| StorageError {
                code: StorageErrorCode::Backend,
                message: format!("Snapshot assertion query failed: {}", e),
            })?
            .collect::<SqliteResult<Vec<_>>>()
            .map_err(|e| StorageError {
                code: StorageErrorCode::Backend,
                message: format!("Snapshot assertion row collection failed: {}", e),
            })?;

        for (field, value_text) in rows {
            let value: Value = serde_json::from_str(&value_text).map_err(|e| StorageError {
                code: StorageErrorCode::Serialization,
                message: format!("Assertion value parse failed for field '{}': {}", field, e),
            })?;
            obj.insert(field, value);
        }

        let now = chrono::Utc::now().to_rfc3339();
        let updated = tx
            .execute(
                &format!(
                    "UPDATE {} SET data = ?, version = version + 1, updated_at = ? WHERE id = ?",
                    table
                ),
                rusqlite::params![snapshot_json.to_string(), now, entity_id.to_string()],
            )
            .map_err(|e| StorageError {
                code: StorageErrorCode::Backend,
                message: format!("Snapshot update failed: {}", e),
            })?;

        if updated == 0 {
            return Err(StorageError {
                code: StorageErrorCode::NotFound,
                message: format!("Entity {} not found in table {}", entity_id, table),
            });
        }

        Ok(())
    }
}

fn write_json_file(path: impl AsRef<Path>, value: &impl serde::Serialize) -> Result<(), StorageError> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| StorageError {
        code: StorageErrorCode::Serialization,
        message: format!("JSON serialization failed: {}", e),
    })?;

    fs::write(path.as_ref(), bytes).map_err(|e| StorageError {
        code: StorageErrorCode::Backend,
        message: format!("Failed to write JSON file '{}': {}", path.as_ref().display(), e),
    })
}

#[async_trait::async_trait]
impl Storage for SqliteBackend {
    // Person
    async fn create_person(&self, person: &Person) -> Result<(), StorageError> {
        let data = Self::serialize(person)?;
        self.insert_sync("persons", person.id, &data)
    }

    async fn get_person(&self, id: EntityId) -> Result<Person, StorageError> {
        self.get_sync::<Person>("persons", id)
    }

    async fn update_person(&self, person: &Person) -> Result<(), StorageError> {
        let data = Self::serialize(person)?;
        self.update_sync("persons", person.id, &data)
    }

    async fn delete_person(&self, id: EntityId) -> Result<(), StorageError> {
        self.delete_sync("persons", id)
    }

    async fn list_persons(&self, pagination: Pagination) -> Result<Vec<Person>, StorageError> {
        self.list_sync::<Person>("persons", pagination)
    }

    // Family
    async fn create_family(&self, family: &Family) -> Result<(), StorageError> {
        let data = Self::serialize(family)?;
        self.insert_sync("families", family.id, &data)
    }

    async fn get_family(&self, id: EntityId) -> Result<Family, StorageError> {
        self.get_sync::<Family>("families", id)
    }

    async fn update_family(&self, family: &Family) -> Result<(), StorageError> {
        let data = Self::serialize(family)?;
        self.update_sync("families", family.id, &data)
    }

    async fn delete_family(&self, id: EntityId) -> Result<(), StorageError> {
        self.delete_sync("families", id)
    }

    async fn list_families(&self, pagination: Pagination) -> Result<Vec<Family>, StorageError> {
        self.list_sync::<Family>("families", pagination)
    }

    // Relationship
    async fn create_relationship(&self, rel: &Relationship) -> Result<(), StorageError> {
        let data = Self::serialize(rel)?;
        self.insert_sync("families", rel.id, &data)
    }

    async fn get_relationship(&self, id: EntityId) -> Result<Relationship, StorageError> {
        self.get_sync::<Relationship>("families", id)
    }

    async fn update_relationship(&self, rel: &Relationship) -> Result<(), StorageError> {
        let data = Self::serialize(rel)?;
        self.update_sync("families", rel.id, &data)
    }

    async fn delete_relationship(&self, id: EntityId) -> Result<(), StorageError> {
        self.delete_sync("families", id)
    }

    async fn list_relationships(
        &self,
        pagination: Pagination,
    ) -> Result<Vec<Relationship>, StorageError> {
        self.list_sync::<Relationship>("families", pagination)
    }

    // Event
    async fn create_event(&self, event: &Event) -> Result<(), StorageError> {
        let data = Self::serialize(event)?;
        self.insert_sync("events", event.id, &data)
    }

    async fn get_event(&self, id: EntityId) -> Result<Event, StorageError> {
        self.get_sync::<Event>("events", id)
    }

    async fn update_event(&self, event: &Event) -> Result<(), StorageError> {
        let data = Self::serialize(event)?;
        self.update_sync("events", event.id, &data)
    }

    async fn delete_event(&self, id: EntityId) -> Result<(), StorageError> {
        self.delete_sync("events", id)
    }

    async fn list_events(&self, pagination: Pagination) -> Result<Vec<Event>, StorageError> {
        self.list_sync::<Event>("events", pagination)
    }

    // Place
    async fn create_place(&self, place: &Place) -> Result<(), StorageError> {
        let data = Self::serialize(place)?;
        self.insert_sync("places", place.id, &data)
    }

    async fn get_place(&self, id: EntityId) -> Result<Place, StorageError> {
        self.get_sync::<Place>("places", id)
    }

    async fn update_place(&self, place: &Place) -> Result<(), StorageError> {
        let data = Self::serialize(place)?;
        self.update_sync("places", place.id, &data)
    }

    async fn delete_place(&self, id: EntityId) -> Result<(), StorageError> {
        self.delete_sync("places", id)
    }

    async fn list_places(&self, pagination: Pagination) -> Result<Vec<Place>, StorageError> {
        self.list_sync::<Place>("places", pagination)
    }

    // Repository
    async fn create_repository(&self, repo: &Repository) -> Result<(), StorageError> {
        let data = Self::serialize(repo)?;
        self.insert_sync("repositories", repo.id, &data)
    }

    async fn get_repository(&self, id: EntityId) -> Result<Repository, StorageError> {
        self.get_sync::<Repository>("repositories", id)
    }

    async fn update_repository(&self, repo: &Repository) -> Result<(), StorageError> {
        let data = Self::serialize(repo)?;
        self.update_sync("repositories", repo.id, &data)
    }

    async fn delete_repository(&self, id: EntityId) -> Result<(), StorageError> {
        self.delete_sync("repositories", id)
    }

    async fn list_repositories(
        &self,
        pagination: Pagination,
    ) -> Result<Vec<Repository>, StorageError> {
        self.list_sync::<Repository>("repositories", pagination)
    }

    // Source
    async fn create_source(&self, source: &Source) -> Result<(), StorageError> {
        let data = Self::serialize(source)?;
        self.insert_sync("sources", source.id, &data)
    }

    async fn get_source(&self, id: EntityId) -> Result<Source, StorageError> {
        self.get_sync::<Source>("sources", id)
    }

    async fn update_source(&self, source: &Source) -> Result<(), StorageError> {
        let data = Self::serialize(source)?;
        self.update_sync("sources", source.id, &data)
    }

    async fn delete_source(&self, id: EntityId) -> Result<(), StorageError> {
        self.delete_sync("sources", id)
    }

    async fn list_sources(&self, pagination: Pagination) -> Result<Vec<Source>, StorageError> {
        self.list_sync::<Source>("sources", pagination)
    }

    // Citation
    async fn create_citation(&self, citation: &Citation) -> Result<(), StorageError> {
        let data = Self::serialize(citation)?;
        self.insert_sync("citations", citation.id, &data)
    }

    async fn get_citation(&self, id: EntityId) -> Result<Citation, StorageError> {
        self.get_sync::<Citation>("citations", id)
    }

    async fn update_citation(&self, citation: &Citation) -> Result<(), StorageError> {
        let data = Self::serialize(citation)?;
        self.update_sync("citations", citation.id, &data)
    }

    async fn delete_citation(&self, id: EntityId) -> Result<(), StorageError> {
        self.delete_sync("citations", id)
    }

    async fn list_citations(&self, pagination: Pagination) -> Result<Vec<Citation>, StorageError> {
        self.list_sync::<Citation>("citations", pagination)
    }

    // Media
    async fn create_media(&self, media: &Media) -> Result<(), StorageError> {
        let data = Self::serialize(media)?;
        self.insert_sync("media", media.id, &data)
    }

    async fn get_media(&self, id: EntityId) -> Result<Media, StorageError> {
        self.get_sync::<Media>("media", id)
    }

    async fn update_media(&self, media: &Media) -> Result<(), StorageError> {
        let data = Self::serialize(media)?;
        self.update_sync("media", media.id, &data)
    }

    async fn delete_media(&self, id: EntityId) -> Result<(), StorageError> {
        self.delete_sync("media", id)
    }

    async fn list_media(&self, pagination: Pagination) -> Result<Vec<Media>, StorageError> {
        self.list_sync::<Media>("media", pagination)
    }

    // Note
    async fn create_note(&self, note: &Note) -> Result<(), StorageError> {
        let data = Self::serialize(note)?;
        self.insert_sync("notes", note.id, &data)
    }

    async fn get_note(&self, id: EntityId) -> Result<Note, StorageError> {
        self.get_sync::<Note>("notes", id)
    }

    async fn update_note(&self, note: &Note) -> Result<(), StorageError> {
        let data = Self::serialize(note)?;
        self.update_sync("notes", note.id, &data)
    }

    async fn delete_note(&self, id: EntityId) -> Result<(), StorageError> {
        self.delete_sync("notes", id)
    }

    async fn list_notes(&self, pagination: Pagination) -> Result<Vec<Note>, StorageError> {
        self.list_sync::<Note>("notes", pagination)
    }

    // LDS Ordinance
    async fn create_lds_ordinance(&self, ord: &LdsOrdinance) -> Result<(), StorageError> {
        let data = Self::serialize(ord)?;
        self.insert_sync("lds_ordinances", ord.id, &data)
    }

    async fn get_lds_ordinance(&self, id: EntityId) -> Result<LdsOrdinance, StorageError> {
        self.get_sync::<LdsOrdinance>("lds_ordinances", id)
    }

    async fn update_lds_ordinance(&self, lds: &LdsOrdinance) -> Result<(), StorageError> {
        let data = Self::serialize(lds)?;
        self.update_sync("lds_ordinances", lds.id, &data)
    }

    async fn delete_lds_ordinance(&self, id: EntityId) -> Result<(), StorageError> {
        self.delete_sync("lds_ordinances", id)
    }

    async fn list_lds_ordinances(
        &self,
        pagination: Pagination,
    ) -> Result<Vec<LdsOrdinance>, StorageError> {
        self.list_sync::<LdsOrdinance>("lds_ordinances", pagination)
    }

    async fn create_assertion(
        &self,
        entity_id: EntityId,
        entity_type: EntityType,
        field: &str,
        assertion: &JsonAssertion,
    ) -> Result<(), StorageError> {
        let idempotency_key = compute_assertion_idempotency_key(
            entity_id,
            field,
            &assertion.value,
            &assertion.source_citations,
        )
        .map_err(|e| StorageError {
            code: StorageErrorCode::Serialization,
            message: format!("Idempotency key computation failed: {}", e),
        })?;

        let mut conn = self
            .connection
            .lock()
            .map_err(|e| StorageError {
                code: StorageErrorCode::Backend,
                message: format!("Mutex lock failed: {}", e),
            })?;

        let tx = conn.transaction().map_err(|e| StorageError {
            code: StorageErrorCode::Backend,
            message: format!("Transaction begin failed: {}", e),
        })?;

        let existing_id: Option<String> = tx
            .query_row(
                "SELECT id FROM assertions WHERE idempotency_key = ?",
                rusqlite::params![&idempotency_key],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| StorageError {
                code: StorageErrorCode::Backend,
                message: format!("Idempotency lookup failed: {}", e),
            })?;

        if existing_id.is_some() {
            tx.commit().map_err(|e| StorageError {
                code: StorageErrorCode::Backend,
                message: format!("Transaction commit failed: {}", e),
            })?;
            return Ok(());
        }

        let preferred = if assertion.status == AssertionStatus::Confirmed {
            let existing_preferred: i64 = tx
                .query_row(
                    "SELECT COUNT(*) FROM assertions WHERE entity_id = ? AND field = ? AND preferred = 1 AND status = 'confirmed'",
                    rusqlite::params![entity_id.to_string(), field],
                    |row| row.get(0),
                )
                .map_err(|e| StorageError {
                    code: StorageErrorCode::Backend,
                    message: format!("Preferred lookup failed: {}", e),
                })?;
            if existing_preferred == 0 {
                1
            } else {
                0
            }
        } else {
            0
        };

        let source_citations_json = serde_json::to_string(&assertion.source_citations).map_err(|e| {
            StorageError {
                code: StorageErrorCode::Serialization,
                message: format!("Source citations serialization failed: {}", e),
            }
        })?;

        tx.execute(
            "INSERT INTO assertions (
                id, entity_id, entity_type, field, value, value_date, value_text,
                confidence, status, preferred, source_citations, proposed_by,
                reviewed_by, created_at, reviewed_at, evidence_type, idempotency_key
             ) VALUES (?, ?, ?, ?, ?, NULL, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            rusqlite::params![
                assertion.id.to_string(),
                entity_id.to_string(),
                Self::entity_type_to_db(entity_type),
                field,
                assertion.value.to_string(),
                assertion.value.as_str(),
                assertion.confidence,
                Self::assertion_status_to_db(&assertion.status),
                preferred,
                source_citations_json,
                assertion.proposed_by.to_string(),
                assertion.reviewed_by.as_ref().map(ToString::to_string),
                assertion.created_at.to_rfc3339(),
                assertion.reviewed_at.as_ref().map(chrono::DateTime::to_rfc3339),
                Self::evidence_type_to_db(&assertion.evidence_type),
                idempotency_key,
            ],
        )
        .map_err(|e| StorageError {
            code: StorageErrorCode::Backend,
            message: format!("Assertion insert failed: {}", e),
        })?;

        Self::recompute_entity_snapshot_tx(&tx, entity_id, entity_type)?;

        tx.commit().map_err(|e| StorageError {
            code: StorageErrorCode::Backend,
            message: format!("Transaction commit failed: {}", e),
        })?;

        Ok(())
    }

    async fn list_assertions_for_entity(
        &self,
        entity_id: EntityId,
    ) -> Result<Vec<JsonAssertion>, StorageError> {
        let conn = self
            .connection
            .lock()
            .map_err(|e| StorageError {
                code: StorageErrorCode::Backend,
                message: format!("Mutex lock failed: {}", e),
            })?;

        let mut stmt = conn
            .prepare(
                "SELECT id, value, confidence, status, evidence_type, source_citations,
                        proposed_by, created_at, reviewed_at, reviewed_by
                 FROM assertions
                 WHERE entity_id = ?
                 ORDER BY created_at DESC",
            )
            .map_err(|e| StorageError {
                code: StorageErrorCode::Backend,
                message: format!("Prepare failed: {}", e),
            })?;

        let mapped = stmt
            .query_map(rusqlite::params![entity_id.to_string()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, f64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, Option<String>>(8)?,
                    row.get::<_, Option<String>>(9)?,
                ))
            })
            .map_err(|e| StorageError {
                code: StorageErrorCode::Backend,
                message: format!("Query failed: {}", e),
            })?;

        let rows = mapped.collect::<SqliteResult<Vec<_>>>().map_err(|e| StorageError {
            code: StorageErrorCode::Backend,
            message: format!("Row collection failed: {}", e),
        })?;

        rows.into_iter()
            .map(
                |(
                    id,
                    value,
                    confidence,
                    status,
                    evidence_type,
                    source_citations,
                    proposed_by,
                    created_at,
                    reviewed_at,
                    reviewed_by,
                )| {
                    Self::row_to_assertion(AssertionRowData {
                        id_str: id,
                        value_text: value,
                        confidence,
                        status_text: status,
                        evidence_type_text: evidence_type,
                        source_citations_text: source_citations,
                        proposed_by_text: proposed_by,
                        created_at_text: created_at,
                        reviewed_at_text: reviewed_at,
                        reviewed_by_text: reviewed_by,
                    })
                },
            )
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| StorageError {
                code: StorageErrorCode::Backend,
                message: format!("Row mapping failed: {:?}", e),
            })
    }

    async fn list_assertions_for_field(
        &self,
        entity_id: EntityId,
        field: &str,
    ) -> Result<Vec<JsonAssertion>, StorageError> {
        let conn = self
            .connection
            .lock()
            .map_err(|e| StorageError {
                code: StorageErrorCode::Backend,
                message: format!("Mutex lock failed: {}", e),
            })?;

        let mut stmt = conn
            .prepare(
                "SELECT id, value, confidence, status, evidence_type, source_citations,
                        proposed_by, created_at, reviewed_at, reviewed_by
                 FROM assertions
                 WHERE entity_id = ? AND field = ?
                 ORDER BY preferred DESC, confidence DESC, created_at DESC",
            )
            .map_err(|e| StorageError {
                code: StorageErrorCode::Backend,
                message: format!("Prepare failed: {}", e),
            })?;

        let mapped = stmt
            .query_map(rusqlite::params![entity_id.to_string(), field], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, f64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, Option<String>>(8)?,
                    row.get::<_, Option<String>>(9)?,
                ))
            })
            .map_err(|e| StorageError {
                code: StorageErrorCode::Backend,
                message: format!("Query failed: {}", e),
            })?;

        let rows = mapped.collect::<SqliteResult<Vec<_>>>().map_err(|e| StorageError {
            code: StorageErrorCode::Backend,
            message: format!("Row collection failed: {}", e),
        })?;

        rows.into_iter()
            .map(
                |(
                    id,
                    value,
                    confidence,
                    status,
                    evidence_type,
                    source_citations,
                    proposed_by,
                    created_at,
                    reviewed_at,
                    reviewed_by,
                )| {
                    Self::row_to_assertion(AssertionRowData {
                        id_str: id,
                        value_text: value,
                        confidence,
                        status_text: status,
                        evidence_type_text: evidence_type,
                        source_citations_text: source_citations,
                        proposed_by_text: proposed_by,
                        created_at_text: created_at,
                        reviewed_at_text: reviewed_at,
                        reviewed_by_text: reviewed_by,
                    })
                },
            )
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| StorageError {
                code: StorageErrorCode::Backend,
                message: format!("Row mapping failed: {:?}", e),
            })
    }

    async fn update_assertion_status(
        &self,
        assertion_id: EntityId,
        status: AssertionStatus,
    ) -> Result<(), StorageError> {
        let mut conn = self
            .connection
            .lock()
            .map_err(|e| StorageError {
                code: StorageErrorCode::Backend,
                message: format!("Mutex lock failed: {}", e),
            })?;

        let tx = conn.transaction().map_err(|e| StorageError {
            code: StorageErrorCode::Backend,
            message: format!("Transaction begin failed: {}", e),
        })?;

        let found: Option<(String, String, String)> = tx
            .query_row(
                "SELECT entity_id, entity_type, field FROM assertions WHERE id = ?",
                rusqlite::params![assertion_id.to_string()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(|e| StorageError {
                code: StorageErrorCode::Backend,
                message: format!("Assertion lookup failed: {}", e),
            })?;

        let (entity_id, entity_type_text, field) = found.ok_or(StorageError {
            code: StorageErrorCode::NotFound,
            message: format!("Assertion not found with id {}", assertion_id),
        })?;
        let entity_type = Self::entity_type_from_db(&entity_type_text)?;

        let now = chrono::Utc::now().to_rfc3339();
        let new_status = Self::assertion_status_to_db(&status);

        if status == AssertionStatus::Confirmed {
            tx.execute(
                "UPDATE assertions
                 SET preferred = 0
                 WHERE entity_id = ? AND field = ? AND id != ?",
                rusqlite::params![entity_id, field, assertion_id.to_string()],
            )
            .map_err(|e| StorageError {
                code: StorageErrorCode::Backend,
                message: format!("Clearing existing preferred assertion failed: {}", e),
            })?;
        }

        let preferred = if status == AssertionStatus::Confirmed {
            1
        } else {
            0
        };

        let rows = tx
            .execute(
                "UPDATE assertions
                 SET status = ?, preferred = ?, reviewed_at = ?
                 WHERE id = ?",
                rusqlite::params![new_status, preferred, now, assertion_id.to_string()],
            )
            .map_err(|e| StorageError {
                code: StorageErrorCode::Backend,
                message: format!("Assertion status update failed: {}", e),
            })?;

        if rows == 0 {
            return Err(StorageError {
                code: StorageErrorCode::NotFound,
                message: format!("Assertion not found with id {}", assertion_id),
            });
        }

        let parsed_entity_id: EntityId = serde_json::from_str(&format!("\"{}\"", entity_id)).map_err(|e| {
            StorageError {
                code: StorageErrorCode::Serialization,
                message: format!("Invalid entity id '{}': {}", entity_id, e),
            }
        })?;

        Self::recompute_entity_snapshot_tx(&tx, parsed_entity_id, entity_type)?;

        tx.commit().map_err(|e| StorageError {
            code: StorageErrorCode::Backend,
            message: format!("Transaction commit failed: {}", e),
        })?;

        Ok(())
    }

    async fn create_research_log_entry(
        &self,
        entry: &ResearchLogEntry,
    ) -> Result<(), StorageError> {
        let conn = self
            .connection
            .lock()
            .map_err(|e| StorageError {
                code: StorageErrorCode::Backend,
                message: format!("Mutex lock failed: {}", e),
            })?;

        let search_terms = serde_json::to_string(&entry.search_terms).map_err(|e| StorageError {
            code: StorageErrorCode::Serialization,
            message: format!("Failed to serialize search_terms: {}", e),
        })?;
        let citations_created = serde_json::to_string(&entry.citations_created).map_err(|e| {
            StorageError {
                code: StorageErrorCode::Serialization,
                message: format!("Failed to serialize citations_created: {}", e),
            }
        })?;
        let person_refs = serde_json::to_string(&entry.person_refs).map_err(|e| StorageError {
            code: StorageErrorCode::Serialization,
            message: format!("Failed to serialize person_refs: {}", e),
        })?;
        let tags = serde_json::to_string(&entry.tags).map_err(|e| StorageError {
            code: StorageErrorCode::Serialization,
            message: format!("Failed to serialize tags: {}", e),
        })?;

        conn.execute(
            "INSERT INTO research_log (
                id, date, objective, repository_id, repository_name, search_terms,
                source_id, result, findings, citations_created, next_steps, person_refs, tags
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            rusqlite::params![
                entry.id.to_string(),
                entry.date.to_rfc3339(),
                &entry.objective,
                entry.repository.map(|v| v.to_string()),
                &entry.repository_name,
                search_terms,
                entry.source_searched.map(|v| v.to_string()),
                Self::search_result_to_db(&entry.result),
                &entry.findings,
                citations_created,
                &entry.next_steps,
                person_refs,
                tags,
            ],
        )
        .map_err(|e| StorageError {
            code: StorageErrorCode::Backend,
            message: format!("Research log insert failed: {}", e),
        })?;

        Ok(())
    }

    async fn get_research_log_entry(&self, id: EntityId) -> Result<ResearchLogEntry, StorageError> {
        let conn = self
            .connection
            .lock()
            .map_err(|e| StorageError {
                code: StorageErrorCode::Backend,
                message: format!("Mutex lock failed: {}", e),
            })?;

        let row = conn
            .query_row(
                "SELECT id, date, objective, repository_id, repository_name, search_terms,
                        source_id, result, findings, citations_created, next_steps, person_refs, tags
                 FROM research_log WHERE id = ?",
                rusqlite::params![id.to_string()],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, Option<String>>(3)?,
                        r.get::<_, Option<String>>(4)?,
                        r.get::<_, String>(5)?,
                        r.get::<_, Option<String>>(6)?,
                        r.get::<_, String>(7)?,
                        r.get::<_, Option<String>>(8)?,
                        r.get::<_, Option<String>>(9)?,
                        r.get::<_, Option<String>>(10)?,
                        r.get::<_, Option<String>>(11)?,
                        r.get::<_, Option<String>>(12)?,
                    ))
                },
            )
            .optional()
            .map_err(|e| StorageError {
                code: StorageErrorCode::Backend,
                message: format!("Research log query failed: {}", e),
            })?
            .ok_or(StorageError {
                code: StorageErrorCode::NotFound,
                message: format!("Research log entry not found: {}", id),
            })?;

        let (
            id_text,
            date_text,
            objective,
            repository_id,
            repository_name,
            search_terms,
            source_id,
            result,
            findings,
            citations_created,
            next_steps,
            person_refs,
            tags,
        ) = row;

        Self::research_row_to_entry(ResearchRowData {
            id_text,
            date_text,
            objective,
            repository_id,
            repository_name,
            search_terms,
            source_id,
            result,
            findings,
            citations_created,
            next_steps,
            person_refs,
            tags,
        })
    }

    async fn delete_research_log_entry(&self, id: EntityId) -> Result<(), StorageError> {
        let conn = self
            .connection
            .lock()
            .map_err(|e| StorageError {
                code: StorageErrorCode::Backend,
                message: format!("Mutex lock failed: {}", e),
            })?;

        conn.execute(
            "DELETE FROM research_log WHERE id = ?",
            rusqlite::params![id.to_string()],
        )
        .map_err(|e| StorageError {
            code: StorageErrorCode::Backend,
            message: format!("Research log delete failed: {}", e),
        })?;

        Ok(())
    }

    async fn list_research_log_entries(
        &self,
        filter: &ResearchLogFilter,
        pagination: Pagination,
    ) -> Result<Vec<ResearchLogEntry>, StorageError> {
        let conn = self
            .connection
            .lock()
            .map_err(|e| StorageError {
                code: StorageErrorCode::Backend,
                message: format!("Mutex lock failed: {}", e),
            })?;

        use rusqlite::types::Value as SqlValue;
        let mut query = String::from(
            "SELECT id, date, objective, repository_id, repository_name, search_terms,
                    source_id, result, findings, citations_created, next_steps, person_refs, tags
             FROM research_log WHERE 1=1",
        );
        let mut args: Vec<SqlValue> = Vec::new();

        if let Some(person_ref) = filter.person_ref {
            query.push_str(" AND EXISTS (SELECT 1 FROM json_each(research_log.person_refs) WHERE json_each.value = ?)");
            args.push(SqlValue::Text(person_ref.to_string()));
        }

        if let Some(result) = &filter.result {
            query.push_str(" AND result = ?");
            args.push(SqlValue::Text(Self::search_result_to_db(result).to_string()));
        }

        if let Some(date_from) = &filter.date_from_iso {
            query.push_str(" AND date >= ?");
            args.push(SqlValue::Text(date_from.clone()));
        }

        if let Some(date_to) = &filter.date_to_iso {
            query.push_str(" AND date <= ?");
            args.push(SqlValue::Text(date_to.clone()));
        }

        query.push_str(" ORDER BY date DESC LIMIT ? OFFSET ?");
        args.push(SqlValue::Integer(i64::from(pagination.limit)));
        args.push(SqlValue::Integer(i64::from(pagination.offset)));

        let mut stmt = conn.prepare(&query).map_err(|e| StorageError {
            code: StorageErrorCode::Backend,
            message: format!("Research list prepare failed: {}", e),
        })?;

        let rows = stmt
            .query_map(rusqlite::params_from_iter(args.iter()), |r| {
                Ok(ResearchRowData {
                    id_text: r.get(0)?,
                    date_text: r.get(1)?,
                    objective: r.get(2)?,
                    repository_id: r.get(3)?,
                    repository_name: r.get(4)?,
                    search_terms: r.get(5)?,
                    source_id: r.get(6)?,
                    result: r.get(7)?,
                    findings: r.get(8)?,
                    citations_created: r.get(9)?,
                    next_steps: r.get(10)?,
                    person_refs: r.get(11)?,
                    tags: r.get(12)?,
                })
            })
            .map_err(|e| StorageError {
                code: StorageErrorCode::Backend,
                message: format!("Research list query failed: {}", e),
            })?
            .collect::<SqliteResult<Vec<_>>>()
            .map_err(|e| StorageError {
                code: StorageErrorCode::Backend,
                message: format!("Research list row collection failed: {}", e),
            })?;

        rows.into_iter()
            .map(Self::research_row_to_entry)
            .collect::<Result<Vec<_>, _>>()
    }

    async fn append_audit_log_entry(&self, entry: &AuditLogEntry) -> Result<(), StorageError> {
        let conn = self
            .connection
            .lock()
            .map_err(|e| StorageError {
                code: StorageErrorCode::Backend,
                message: format!("Mutex lock failed: {}", e),
            })?;

        conn.execute(
            "INSERT INTO audit_log (timestamp, actor, entity_id, entity_type, action, old_value, new_value)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
            rusqlite::params![
                &entry.timestamp_iso,
                &entry.actor,
                entry.entity_id.to_string(),
                Self::entity_type_to_db(entry.entity_type),
                &entry.action,
                entry.old_value_json.as_ref().map(serde_json::Value::to_string),
                entry.new_value_json.as_ref().map(serde_json::Value::to_string),
            ],
        )
        .map_err(|e| StorageError {
            code: StorageErrorCode::Backend,
            message: format!("Audit log insert failed: {}", e),
        })?;

        Ok(())
    }

    async fn upsert_relationship_edge(&self, _edge: &RelationshipEdge) -> Result<(), StorageError> {
        let conn = self
            .connection
            .lock()
            .map_err(|e| StorageError {
                code: StorageErrorCode::Backend,
                message: format!("Mutex lock failed: {}", e),
            })?;

        let edge = if _edge.directed {
            _edge.clone()
        } else if _edge.from_entity <= _edge.to_entity {
            _edge.clone()
        } else {
            RelationshipEdge {
                from_entity: _edge.to_entity,
                to_entity: _edge.from_entity,
                rel_type: _edge.rel_type.clone(),
                directed: false,
                assertion_id: _edge.assertion_id,
            }
        };

        let edge_id = format!(
            "{}:{}:{}:{}",
            edge.from_entity,
            edge.to_entity,
            edge.rel_type,
            if edge.directed { 1 } else { 0 }
        );

        conn.execute(
            "INSERT INTO relationships (
                id, from_entity, from_type, to_entity, to_type, rel_type, assertion_id, directed
             ) VALUES (?, ?, 'person', ?, 'person', ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
                assertion_id = excluded.assertion_id,
                rel_type = excluded.rel_type,
                directed = excluded.directed",
            rusqlite::params![
                edge_id,
                edge.from_entity.to_string(),
                edge.to_entity.to_string(),
                &edge.rel_type,
                edge.assertion_id.map(|v| v.to_string()),
                if edge.directed { 1 } else { 0 },
            ],
        )
        .map_err(|e| StorageError {
            code: StorageErrorCode::Backend,
            message: format!("Relationship upsert failed: {}", e),
        })?;

        Ok(())
    }

    async fn list_relationship_edges_for_entity(
        &self,
        entity_id: EntityId,
    ) -> Result<Vec<RelationshipEdge>, StorageError> {
        let conn = self
            .connection
            .lock()
            .map_err(|e| StorageError {
                code: StorageErrorCode::Backend,
                message: format!("Mutex lock failed: {}", e),
            })?;

        let mut stmt = conn
            .prepare(
                "SELECT from_entity, to_entity, rel_type, directed, assertion_id
                 FROM relationships
                 WHERE from_entity = ? OR to_entity = ?",
            )
            .map_err(|e| StorageError {
                code: StorageErrorCode::Backend,
                message: format!("Relationship list prepare failed: {}", e),
            })?;

        let rows = stmt
            .query_map(
                rusqlite::params![entity_id.to_string(), entity_id.to_string()],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, i64>(3)?,
                        r.get::<_, Option<String>>(4)?,
                    ))
                },
            )
            .map_err(|e| StorageError {
                code: StorageErrorCode::Backend,
                message: format!("Relationship list query failed: {}", e),
            })?
            .collect::<SqliteResult<Vec<_>>>()
            .map_err(|e| StorageError {
                code: StorageErrorCode::Backend,
                message: format!("Relationship list row collection failed: {}", e),
            })?;

        rows.into_iter()
            .map(|(from, to, rel_type, directed, assertion_id)| {
                Ok(RelationshipEdge {
                    from_entity: Self::parse_entity_id_str(&from)?,
                    to_entity: Self::parse_entity_id_str(&to)?,
                    rel_type,
                    directed: directed != 0,
                    assertion_id: assertion_id
                        .as_deref()
                        .map(Self::parse_entity_id_str)
                        .transpose()?,
                })
            })
            .collect::<Result<Vec<_>, _>>()
    }

    async fn ancestors(
        &self,
        person_id: EntityId,
        max_depth: u32,
    ) -> Result<Vec<EntityId>, StorageError> {
        let conn = self
            .connection
            .lock()
            .map_err(|e| StorageError {
                code: StorageErrorCode::Backend,
                message: format!("Mutex lock failed: {}", e),
            })?;

        let mut stmt = conn
            .prepare(
                "WITH RECURSIVE anc(id, depth) AS (
                    SELECT from_entity, 1
                    FROM relationships
                    WHERE directed = 1 AND rel_type = 'parent_of' AND to_entity = ?
                    UNION ALL
                    SELECT r.from_entity, anc.depth + 1
                    FROM relationships r
                    JOIN anc ON r.to_entity = anc.id
                    WHERE r.directed = 1
                      AND r.rel_type = 'parent_of'
                      AND anc.depth < ?
                )
                SELECT DISTINCT id FROM anc",
            )
            .map_err(|e| StorageError {
                code: StorageErrorCode::Backend,
                message: format!("Ancestors prepare failed: {}", e),
            })?;

        let rows = stmt
            .query_map(
                rusqlite::params![person_id.to_string(), i64::from(max_depth)],
                |r| r.get::<_, String>(0),
            )
            .map_err(|e| StorageError {
                code: StorageErrorCode::Backend,
                message: format!("Ancestors query failed: {}", e),
            })?
            .collect::<SqliteResult<Vec<_>>>()
            .map_err(|e| StorageError {
                code: StorageErrorCode::Backend,
                message: format!("Ancestors row collection failed: {}", e),
            })?;

        rows.into_iter()
            .map(|id| Self::parse_entity_id_str(&id))
            .collect::<Result<Vec<_>, _>>()
    }

    async fn descendants(
        &self,
        person_id: EntityId,
        max_depth: u32,
    ) -> Result<Vec<EntityId>, StorageError> {
        let conn = self
            .connection
            .lock()
            .map_err(|e| StorageError {
                code: StorageErrorCode::Backend,
                message: format!("Mutex lock failed: {}", e),
            })?;

        let mut stmt = conn
            .prepare(
                "WITH RECURSIVE des(id, depth) AS (
                    SELECT to_entity, 1
                    FROM relationships
                    WHERE directed = 1 AND rel_type = 'parent_of' AND from_entity = ?
                    UNION ALL
                    SELECT r.to_entity, des.depth + 1
                    FROM relationships r
                    JOIN des ON r.from_entity = des.id
                    WHERE r.directed = 1
                      AND r.rel_type = 'parent_of'
                      AND des.depth < ?
                )
                SELECT DISTINCT id FROM des",
            )
            .map_err(|e| StorageError {
                code: StorageErrorCode::Backend,
                message: format!("Descendants prepare failed: {}", e),
            })?;

        let rows = stmt
            .query_map(
                rusqlite::params![person_id.to_string(), i64::from(max_depth)],
                |r| r.get::<_, String>(0),
            )
            .map_err(|e| StorageError {
                code: StorageErrorCode::Backend,
                message: format!("Descendants query failed: {}", e),
            })?
            .collect::<SqliteResult<Vec<_>>>()
            .map_err(|e| StorageError {
                code: StorageErrorCode::Backend,
                message: format!("Descendants row collection failed: {}", e),
            })?;

        rows.into_iter()
            .map(|id| Self::parse_entity_id_str(&id))
            .collect::<Result<Vec<_>, _>>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustygene_core::assertion::EvidenceType;
    use rustygene_core::types::ActorRef;
    use serde_json::json;
    use std::time::{SystemTime, UNIX_EPOCH};


    fn create_test_backend() -> SqliteBackend {
        let mut conn = Connection::open_in_memory().expect("open db");
        crate::run_migrations(&mut conn).expect("migrate");
        SqliteBackend::new(conn)
    }

    #[tokio::test]
    async fn insert_and_get_round_trip() {
        let backend = create_test_backend();
        let person = Person {
            id: EntityId::new(),
            names: vec![],
            gender: rustygene_core::types::Gender::Unknown,
            living: true,
            private: false,
            _raw_gedcom: Default::default(),
        };

        backend.create_person(&person).await.expect("create");
        let retrieved = backend.get_person(person.id).await.expect("get");
        assert_eq!(person.id, retrieved.id);
    }


    #[tokio::test]
    async fn delete_removes_entity() {
        let backend = create_test_backend();
        let person = Person {
            id: EntityId::new(),
            names: vec![],
            gender: rustygene_core::types::Gender::Unknown,
            living: true,
            private: false,
            _raw_gedcom: Default::default(),
        };

        backend.create_person(&person).await.expect("create");
        backend.delete_person(person.id).await.expect("delete");
        let result = backend.get_person(person.id).await;
        assert!(matches!(
            result,
            Err(StorageError {
                code: StorageErrorCode::NotFound,
                ..
            })
        ));
    }

    #[tokio::test]
    async fn list_pagination() {
        let backend = create_test_backend();
        for _ in 0..5 {
            let person = Person {
                id: EntityId::new(),
                names: vec![],
                gender: rustygene_core::types::Gender::Unknown,
                living: true,
                private: false,
                _raw_gedcom: Default::default(),
            };
            backend.create_person(&person).await.expect("create");
        }

        let p1 = backend
            .list_persons(Pagination {
                limit: 2,
                offset: 0,
            })
            .await
            .expect("list");
        assert_eq!(p1.len(), 2);
    }

    fn sample_assertion(value: Value, status: AssertionStatus) -> JsonAssertion {
        JsonAssertion {
            id: EntityId::new(),
            value,
            confidence: 0.9,
            status,
            evidence_type: EvidenceType::Direct,
            source_citations: Vec::new(),
            proposed_by: ActorRef::User("tester".to_string()),
            created_at: chrono::Utc::now(),
            reviewed_at: None,
            reviewed_by: None,
        }
    }

    fn sample_research_entry(
        id: EntityId,
        date: chrono::DateTime<chrono::Utc>,
        result: SearchResult,
        person_refs: Vec<EntityId>,
    ) -> ResearchLogEntry {
        ResearchLogEntry {
            id,
            date,
            objective: "Find census hit".to_string(),
            repository: None,
            repository_name: Some("Archive".to_string()),
            search_terms: vec!["john".to_string(), "census".to_string()],
            source_searched: None,
            result,
            findings: Some("Some findings".to_string()),
            citations_created: vec![],
            next_steps: Some("More work".to_string()),
            person_refs,
            tags: vec!["tag1".to_string()],
        }
    }

    #[tokio::test]
    async fn assertion_create_and_query_by_entity_and_field() {
        let backend = create_test_backend();
        let entity_id = EntityId::new();
        let person = Person {
            id: entity_id,
            names: vec![],
            gender: rustygene_core::types::Gender::Unknown,
            living: true,
            private: false,
            _raw_gedcom: Default::default(),
        };
        backend.create_person(&person).await.expect("create person");

        let name_assertion = sample_assertion(json!("John Doe"), AssertionStatus::Confirmed);
        let birth_assertion = sample_assertion(json!("1850-05-01"), AssertionStatus::Proposed);

        backend
            .create_assertion(entity_id, EntityType::Person, "name", &name_assertion)
            .await
            .expect("create name assertion");
        backend
            .create_assertion(entity_id, EntityType::Person, "birth_date", &birth_assertion)
            .await
            .expect("create birth assertion");

        let all = backend
            .list_assertions_for_entity(entity_id)
            .await
            .expect("list by entity");
        assert_eq!(all.len(), 2);

        let names = backend
            .list_assertions_for_field(entity_id, "name")
            .await
            .expect("list by field");
        assert_eq!(names.len(), 1);
        assert_eq!(names[0].id, name_assertion.id);
        assert_eq!(names[0].value, json!("John Doe"));
    }

    #[tokio::test]
    async fn assertion_idempotency_duplicate_is_noop() {
        let backend = create_test_backend();
        let entity_id = EntityId::new();
        let person = Person {
            id: entity_id,
            names: vec![],
            gender: rustygene_core::types::Gender::Unknown,
            living: true,
            private: false,
            _raw_gedcom: Default::default(),
        };
        backend.create_person(&person).await.expect("create person");

        let assertion = sample_assertion(json!("John Doe"), AssertionStatus::Proposed);

        backend
            .create_assertion(entity_id, EntityType::Person, "name", &assertion)
            .await
            .expect("first create");

        let duplicate = JsonAssertion {
            id: EntityId::new(),
            ..assertion.clone()
        };

        backend
            .create_assertion(entity_id, EntityType::Person, "name", &duplicate)
            .await
            .expect("duplicate should be no-op");

        let names = backend
            .list_assertions_for_field(entity_id, "name")
            .await
            .expect("list by field");
        assert_eq!(names.len(), 1);
        assert_eq!(names[0].id, assertion.id);
    }

    #[tokio::test]
    async fn assertion_status_update_sets_preferred() {
        let backend = create_test_backend();
        let entity_id = EntityId::new();
        let person = Person {
            id: entity_id,
            names: vec![],
            gender: rustygene_core::types::Gender::Unknown,
            living: true,
            private: false,
            _raw_gedcom: Default::default(),
        };
        backend.create_person(&person).await.expect("create person");

        let a1 = sample_assertion(json!("John A Doe"), AssertionStatus::Confirmed);
        let a2 = sample_assertion(json!("John B Doe"), AssertionStatus::Proposed);

        backend
            .create_assertion(entity_id, EntityType::Person, "name", &a1)
            .await
            .expect("create first assertion");
        backend
            .create_assertion(entity_id, EntityType::Person, "name", &a2)
            .await
            .expect("create second assertion");

        backend
            .update_assertion_status(a2.id, AssertionStatus::Confirmed)
            .await
            .expect("promote second assertion");

        let names = backend
            .list_assertions_for_field(entity_id, "name")
            .await
            .expect("list by field");
        assert_eq!(names.len(), 2);

        let first = &names[0];
        assert_eq!(first.id, a2.id);
        assert_eq!(first.status, AssertionStatus::Confirmed);

        let conn = backend.connection.lock().expect("lock");
        let a1_preferred: i64 = conn
            .query_row(
                "SELECT preferred FROM assertions WHERE id = ?",
                rusqlite::params![a1.id.to_string()],
                |row| row.get(0),
            )
            .expect("a1 preferred");
        let a2_preferred: i64 = conn
            .query_row(
                "SELECT preferred FROM assertions WHERE id = ?",
                rusqlite::params![a2.id.to_string()],
                |row| row.get(0),
            )
            .expect("a2 preferred");

        assert_eq!(a1_preferred, 0);
        assert_eq!(a2_preferred, 1);
    }

    #[tokio::test]
    async fn snapshot_recomputed_on_assertion_create() {
        let backend = create_test_backend();
        let person_id = EntityId::new();
        let person = Person {
            id: person_id,
            names: vec![],
            gender: rustygene_core::types::Gender::Unknown,
            living: true,
            private: false,
            _raw_gedcom: Default::default(),
        };
        backend.create_person(&person).await.expect("create person");

        let assertion = sample_assertion(json!("John Snapshot"), AssertionStatus::Confirmed);
        backend
            .create_assertion(person_id, EntityType::Person, "name", &assertion)
            .await
            .expect("create assertion");

        let conn = backend.connection.lock().expect("lock");
        let snapshot_data: String = conn
            .query_row(
                "SELECT data FROM persons WHERE id = ?",
                rusqlite::params![person_id.to_string()],
                |row| row.get(0),
            )
            .expect("read person snapshot");
        let snapshot_json: Value = serde_json::from_str(&snapshot_data).expect("parse snapshot");
        assert_eq!(snapshot_json["name"], json!("John Snapshot"));
    }

    #[tokio::test]
    async fn snapshot_recomputed_on_assertion_status_change() {
        let backend = create_test_backend();
        let person_id = EntityId::new();
        let person = Person {
            id: person_id,
            names: vec![],
            gender: rustygene_core::types::Gender::Unknown,
            living: true,
            private: false,
            _raw_gedcom: Default::default(),
        };
        backend.create_person(&person).await.expect("create person");

        let first = sample_assertion(json!("John Old"), AssertionStatus::Confirmed);
        let second = sample_assertion(json!("John New"), AssertionStatus::Proposed);

        backend
            .create_assertion(person_id, EntityType::Person, "name", &first)
            .await
            .expect("create first assertion");
        backend
            .create_assertion(person_id, EntityType::Person, "name", &second)
            .await
            .expect("create second assertion");

        backend
            .update_assertion_status(second.id, AssertionStatus::Confirmed)
            .await
            .expect("confirm second assertion");

        let conn = backend.connection.lock().expect("lock");
        let snapshot_data: String = conn
            .query_row(
                "SELECT data FROM persons WHERE id = ?",
                rusqlite::params![person_id.to_string()],
                |row| row.get(0),
            )
            .expect("read person snapshot");
        let snapshot_json: Value = serde_json::from_str(&snapshot_data).expect("parse snapshot");
        assert_eq!(snapshot_json["name"], json!("John New"));
    }

    #[tokio::test]
    async fn append_audit_log_entry_persists_row() {
        let backend = create_test_backend();
        let entity_id = EntityId::new();

        let entry = AuditLogEntry {
            actor: "user:tester".to_string(),
            entity_id,
            entity_type: EntityType::Person,
            action: "update_person".to_string(),
            old_value_json: Some(json!({ "living": true })),
            new_value_json: Some(json!({ "living": false })),
            timestamp_iso: chrono::Utc::now().to_rfc3339(),
        };

        backend
            .append_audit_log_entry(&entry)
            .await
            .expect("append audit log entry");

        let conn = backend.connection.lock().expect("lock");
        let row: (String, String, String, String, String, Option<String>, Option<String>) = conn
            .query_row(
                "SELECT timestamp, actor, entity_id, entity_type, action, old_value, new_value
                 FROM audit_log ORDER BY id DESC LIMIT 1",
                [],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                        r.get(6)?,
                    ))
                },
            )
            .expect("read audit row");

        assert_eq!(row.1, entry.actor);
        assert_eq!(row.2, entity_id.to_string());
        assert_eq!(row.3, "person");
        assert_eq!(row.4, entry.action);
        assert_eq!(
            row.5.as_deref(),
            Some(json!({ "living": true }).to_string().as_str())
        );
        assert_eq!(
            row.6.as_deref(),
            Some(json!({ "living": false }).to_string().as_str())
        );
    }

    #[tokio::test]
    async fn research_log_create_get_delete_round_trip() {
        let backend = create_test_backend();
        let entry_id = EntityId::new();
        let entry = sample_research_entry(
            entry_id,
            chrono::Utc::now(),
            SearchResult::PartiallyFound,
            vec![EntityId::new()],
        );

        backend
            .create_research_log_entry(&entry)
            .await
            .expect("create research entry");

        let fetched = backend
            .get_research_log_entry(entry_id)
            .await
            .expect("get research entry");
        assert_eq!(fetched.id, entry.id);
        assert_eq!(fetched.objective, entry.objective);
        assert_eq!(fetched.result, SearchResult::PartiallyFound);

        backend
            .delete_research_log_entry(entry_id)
            .await
            .expect("delete research entry");

        let after_delete = backend.get_research_log_entry(entry_id).await;
        assert!(matches!(
            after_delete,
            Err(StorageError {
                code: StorageErrorCode::NotFound,
                ..
            })
        ));
    }

    #[tokio::test]
    async fn research_log_list_filters_work() {
        let backend = create_test_backend();
        let p1 = EntityId::new();
        let p2 = EntityId::new();

        let older = sample_research_entry(
            EntityId::new(),
            chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
                .expect("parse older")
                .with_timezone(&chrono::Utc),
            SearchResult::Found,
            vec![p1],
        );
        let newer = sample_research_entry(
            EntityId::new(),
            chrono::DateTime::parse_from_rfc3339("2025-01-01T00:00:00Z")
                .expect("parse newer")
                .with_timezone(&chrono::Utc),
            SearchResult::NotFound,
            vec![p2],
        );

        backend
            .create_research_log_entry(&older)
            .await
            .expect("create older");
        backend
            .create_research_log_entry(&newer)
            .await
            .expect("create newer");

        let by_person = backend
            .list_research_log_entries(
                &ResearchLogFilter {
                    person_ref: Some(p1),
                    result: None,
                    date_from_iso: None,
                    date_to_iso: None,
                },
                Pagination::default(),
            )
            .await
            .expect("list by person");
        assert_eq!(by_person.len(), 1);
        assert_eq!(by_person[0].id, older.id);

        let by_result = backend
            .list_research_log_entries(
                &ResearchLogFilter {
                    person_ref: None,
                    result: Some(SearchResult::NotFound),
                    date_from_iso: None,
                    date_to_iso: None,
                },
                Pagination::default(),
            )
            .await
            .expect("list by result");
        assert_eq!(by_result.len(), 1);
        assert_eq!(by_result[0].id, newer.id);

        let by_date = backend
            .list_research_log_entries(
                &ResearchLogFilter {
                    person_ref: None,
                    result: None,
                    date_from_iso: Some("2024-06-01T00:00:00Z".to_string()),
                    date_to_iso: Some("2025-12-31T23:59:59Z".to_string()),
                },
                Pagination::default(),
            )
            .await
            .expect("list by date range");
        assert_eq!(by_date.len(), 1);
        assert_eq!(by_date[0].id, newer.id);
    }

    #[tokio::test]
    async fn relationship_upsert_and_list_supports_undirected_normalization() {
        let backend = create_test_backend();
        let a = EntityId::new();
        let b = EntityId::new();

        let edge = RelationshipEdge {
            from_entity: b,
            to_entity: a,
            rel_type: "partner_in".to_string(),
            directed: false,
            assertion_id: None,
        };

        backend
            .upsert_relationship_edge(&edge)
            .await
            .expect("upsert undirected edge");

        let a_edges = backend
            .list_relationship_edges_for_entity(a)
            .await
            .expect("list edges for a");
        let b_edges = backend
            .list_relationship_edges_for_entity(b)
            .await
            .expect("list edges for b");

        assert_eq!(a_edges.len(), 1);
        assert_eq!(b_edges.len(), 1);
        assert!(!a_edges[0].directed);
        assert_eq!(a_edges[0].rel_type, "partner_in");
    }

    #[tokio::test]
    async fn relationship_ancestors_and_descendants_follow_parent_of_edges() {
        let backend = create_test_backend();

        let grandparent = EntityId::new();
        let parent = EntityId::new();
        let child = EntityId::new();

        backend
            .upsert_relationship_edge(&RelationshipEdge {
                from_entity: grandparent,
                to_entity: parent,
                rel_type: "parent_of".to_string(),
                directed: true,
                assertion_id: None,
            })
            .await
            .expect("upsert grandparent->parent");

        backend
            .upsert_relationship_edge(&RelationshipEdge {
                from_entity: parent,
                to_entity: child,
                rel_type: "parent_of".to_string(),
                directed: true,
                assertion_id: None,
            })
            .await
            .expect("upsert parent->child");

        let ancestors = backend.ancestors(child, 4).await.expect("ancestors");
        assert!(ancestors.contains(&parent));
        assert!(ancestors.contains(&grandparent));

        let descendants = backend
            .descendants(grandparent, 4)
            .await
            .expect("descendants");
        assert!(descendants.contains(&parent));
        assert!(descendants.contains(&child));
    }

    #[tokio::test]
    async fn rebuild_all_snapshots_recomputes_confirmed_preferred_fields() {
        let backend = create_test_backend();
        let person_id = EntityId::new();
        backend
            .create_person(&Person {
                id: person_id,
                names: vec![],
                gender: rustygene_core::types::Gender::Unknown,
                living: true,
                private: false,
                _raw_gedcom: Default::default(),
            })
            .await
            .expect("create person");

        let asserted_name = sample_assertion(json!("Rebuilt Name"), AssertionStatus::Confirmed);
        backend
            .create_assertion(person_id, EntityType::Person, "name", &asserted_name)
            .await
            .expect("create assertion");

        {
            let conn = backend.connection.lock().expect("lock");
            conn.execute(
                "UPDATE persons SET data = json_set(data, '$.name', 'stale') WHERE id = ?",
                rusqlite::params![person_id.to_string()],
            )
            .expect("set stale value");
        }

        let rebuilt = backend.rebuild_all_snapshots().expect("rebuild snapshots");
        assert!(rebuilt >= 1);

        let conn = backend.connection.lock().expect("lock");
        let data: String = conn
            .query_row(
                "SELECT data FROM persons WHERE id = ?",
                rusqlite::params![person_id.to_string()],
                |row| row.get(0),
            )
            .expect("read person snapshot");
        let value: serde_json::Value = serde_json::from_str(&data).expect("parse person json");
        assert_eq!(value["name"], json!("Rebuilt Name"));
    }

    #[tokio::test]
    async fn json_export_directory_writes_manifest_and_tables() {
        let backend = create_test_backend();
        let person_id = EntityId::new();
        backend
            .create_person(&Person {
                id: person_id,
                names: vec![],
                gender: rustygene_core::types::Gender::Unknown,
                living: true,
                private: false,
                _raw_gedcom: Default::default(),
            })
            .await
            .expect("create person");

        let assertion = sample_assertion(json!("Exported Name"), AssertionStatus::Confirmed);
        backend
            .create_assertion(person_id, EntityType::Person, "name", &assertion)
            .await
            .expect("create assertion");

        let research = sample_research_entry(
            EntityId::new(),
            chrono::Utc::now(),
            SearchResult::Found,
            vec![person_id],
        );
        backend
            .create_research_log_entry(&research)
            .await
            .expect("create research entry");

        backend
            .append_audit_log_entry(&AuditLogEntry {
                actor: "user:export-test".to_string(),
                entity_id: person_id,
                entity_type: EntityType::Person,
                action: "update".to_string(),
                old_value_json: None,
                new_value_json: Some(json!({"name": "Exported Name"})),
                timestamp_iso: chrono::Utc::now().to_rfc3339(),
            })
            .await
            .expect("append audit");

        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let out_dir = std::env::temp_dir().join(format!("rustygene-json-export-{suffix}"));

        let result = backend
            .export_json_dump(JsonExportMode::Directory {
                output_dir: out_dir.clone(),
            })
            .expect("export directory");

        assert_eq!(result.output_path, out_dir);
        assert!(out_dir.join("manifest.json").exists());
        assert!(out_dir.join("persons.json").exists());
        assert!(out_dir.join("assertions.json").exists());
        assert!(out_dir.join("audit_log.json").exists());
        assert!(out_dir.join("research_log.json").exists());
        assert!(result.manifest.entity_counts.get("persons").copied().unwrap_or(0) >= 1);
        assert!(result
            .manifest
            .entity_counts
            .get("assertions")
            .copied()
            .unwrap_or(0)
            >= 1);

        let _ = std::fs::remove_dir_all(&out_dir);
    }

    #[tokio::test]
    async fn json_export_single_file_writes_combined_payload() {
        let backend = create_test_backend();
        let person_id = EntityId::new();
        backend
            .create_person(&Person {
                id: person_id,
                names: vec![],
                gender: rustygene_core::types::Gender::Unknown,
                living: true,
                private: false,
                _raw_gedcom: Default::default(),
            })
            .await
            .expect("create person");

        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let out_file = std::env::temp_dir().join(format!("rustygene-export-{suffix}.json"));

        let result = backend
            .export_json_dump(JsonExportMode::SingleFile {
                output_file: out_file.clone(),
            })
            .expect("export single file");

        assert_eq!(result.output_path, out_file);
        let payload = std::fs::read_to_string(&out_file).expect("read exported file");
        let parsed: serde_json::Value = serde_json::from_str(&payload).expect("parse exported json");

        assert!(parsed.get("manifest").is_some());
        assert!(parsed.get("persons").is_some());
        assert!(parsed["persons"].as_array().is_some());

        let _ = std::fs::remove_file(&out_file);
    }
}
