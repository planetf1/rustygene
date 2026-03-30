use crate::{
    AuditLogEntry, EntityType, JsonAssertion, Pagination, RelationshipEdge, ResearchLogFilter,
    Storage, StorageError, StorageErrorCode,
};
use rustygene_core::assertion::AssertionStatus;
use rustygene_core::evidence::{Citation, Media, Note, Repository, Source};
use rustygene_core::event::Event;
use rustygene_core::family::{Family, Relationship};
use rustygene_core::lds::LdsOrdinance;
use rustygene_core::place::Place;
use rustygene_core::person::Person;
use rustygene_core::research::ResearchLogEntry;
use rustygene_core::types::EntityId;
use rusqlite::{Connection, OptionalExtension, Result as SqliteResult};
use serde_json::Value;
use std::sync::{Arc, Mutex};

/// SQLite-backed implementation of the Storage trait.
pub struct SqliteBackend {
    connection: Arc<Mutex<Connection>>,
}

impl SqliteBackend {
    pub fn new(connection: Connection) -> Self {
        Self {
            connection: Arc::new(Mutex::new(connection)),
        }
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

    // Stubs
    async fn create_assertion(
        &self,
        _entity_id: EntityId,
        _entity_type: EntityType,
        _field: &str,
        _assertion: &JsonAssertion,
    ) -> Result<(), StorageError> {
        Err(StorageError {
            code: StorageErrorCode::Backend,
            message: "Not implemented".to_string(),
        })
    }

    async fn list_assertions_for_entity(
        &self,
        _entity_id: EntityId,
    ) -> Result<Vec<JsonAssertion>, StorageError> {
        Ok(Vec::new())
    }

    async fn list_assertions_for_field(
        &self,
        _entity_id: EntityId,
        _field: &str,
    ) -> Result<Vec<JsonAssertion>, StorageError> {
        Ok(Vec::new())
    }

    async fn update_assertion_status(
        &self,
        _assertion_id: EntityId,
        _status: AssertionStatus,
    ) -> Result<(), StorageError> {
        Err(StorageError {
            code: StorageErrorCode::Backend,
            message: "Not implemented".to_string(),
        })
    }

    async fn create_research_log_entry(
        &self,
        _entry: &ResearchLogEntry,
    ) -> Result<(), StorageError> {
        Err(StorageError {
            code: StorageErrorCode::Backend,
            message: "Not implemented".to_string(),
        })
    }

    async fn get_research_log_entry(&self, _id: EntityId) -> Result<ResearchLogEntry, StorageError> {
        Err(StorageError {
            code: StorageErrorCode::Backend,
            message: "Not implemented".to_string(),
        })
    }

    async fn delete_research_log_entry(&self, _id: EntityId) -> Result<(), StorageError> {
        Err(StorageError {
            code: StorageErrorCode::Backend,
            message: "Not implemented".to_string(),
        })
    }

    async fn list_research_log_entries(
        &self,
        _filter: &ResearchLogFilter,
        _pagination: Pagination,
    ) -> Result<Vec<ResearchLogEntry>, StorageError> {
        Ok(Vec::new())
    }

    async fn append_audit_log_entry(&self, _entry: &AuditLogEntry) -> Result<(), StorageError> {
        Err(StorageError {
            code: StorageErrorCode::Backend,
            message: "Not implemented".to_string(),
        })
    }

    async fn upsert_relationship_edge(&self, _edge: &RelationshipEdge) -> Result<(), StorageError> {
        Err(StorageError {
            code: StorageErrorCode::Backend,
            message: "Not implemented".to_string(),
        })
    }

    async fn list_relationship_edges_for_entity(
        &self,
        _entity_id: EntityId,
    ) -> Result<Vec<RelationshipEdge>, StorageError> {
        Ok(Vec::new())
    }

    async fn ancestors(
        &self,
        _person_id: EntityId,
        _max_depth: u32,
    ) -> Result<Vec<EntityId>, StorageError> {
        Ok(Vec::new())
    }

    async fn descendants(
        &self,
        _person_id: EntityId,
        _max_depth: u32,
    ) -> Result<Vec<EntityId>, StorageError> {
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;


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
}
