use std::collections::BTreeMap;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use chrono::Utc;
use rustygene_core::assertion::{AssertionStatus, EvidenceType};
use rustygene_core::evidence::{Note, NoteRef, NoteType};
use rustygene_core::types::{ActorRef, EntityId};
use rustygene_storage::{EntityType, JsonAssertion, Pagination};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::errors::{ApiError, parse_entity_id};
use crate::AppState;

#[derive(Debug, Deserialize)]
struct NotesQuery {
    #[serde(default)]
    limit: Option<u32>,
    #[serde(default)]
    offset: Option<u32>,
    #[serde(default)]
    entity_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UpsertNoteRequest {
    text: String,
    #[serde(default)]
    note_type: Option<NoteType>,
    #[serde(default)]
    linked_entity_id: Option<EntityId>,
    #[serde(default)]
    linked_entity_type: Option<String>,
    #[serde(default)]
    position_x_pct: Option<u8>,
    #[serde(default)]
    position_y_pct: Option<u8>,
}

#[derive(Debug, Serialize)]
struct NoteDetailResponse {
    id: EntityId,
    text: String,
    note_type: NoteType,
    linked_entity_id: Option<EntityId>,
    linked_entity_type: Option<String>,
    position_x_pct: Option<u8>,
    position_y_pct: Option<u8>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_notes).post(create_note))
        .route("/:id", get(get_note).put(update_note).delete(delete_note))
}

async fn list_notes(
    State(state): State<AppState>,
    Query(query): Query<NotesQuery>,
) -> Result<Json<Vec<NoteDetailResponse>>, ApiError> {
    if let Some(entity_id_raw) = query.entity_id {
        let entity_id = parse_entity_id(&entity_id_raw)?;
        let assertion_records = state
            .storage
            .list_assertion_records_for_entity(entity_id)
            .await?;

        let mut note_ids = Vec::new();
        for record in assertion_records {
            if record.field != "note_ref" {
                continue;
            }

            if let Ok(note_ref) = serde_json::from_value::<NoteRef>(record.assertion.value.clone())
            {
                note_ids.push(note_ref.note_id);
                continue;
            }

            if let Ok(note_id_str) = serde_json::from_value::<String>(record.assertion.value) {
                if let Ok(parsed) = Uuid::parse_str(&note_id_str) {
                    note_ids.push(EntityId(parsed));
                }
            }
        }

        note_ids.sort_by_key(ToString::to_string);
        note_ids.dedup();

        let mut notes = Vec::new();
        for note_id in note_ids {
            if let Ok(note) = state.storage.get_note(note_id).await {
                notes.push(note_to_detail_response(note));
            }
        }

        return Ok(Json(notes));
    }

    let notes = state
        .storage
        .list_notes(Pagination {
            limit: query.limit.unwrap_or(100),
            offset: query.offset.unwrap_or(0),
        })
        .await?;

    Ok(Json(
        notes
            .into_iter()
            .map(note_to_detail_response)
            .collect::<Vec<_>>(),
    ))
}

async fn create_note(
    State(state): State<AppState>,
    Json(request): Json<UpsertNoteRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let sanitized_text = sanitize_note_text(&request.text)?;

    let mut note = Note {
        id: EntityId::new(),
        text: sanitized_text,
        note_type: request.note_type.unwrap_or(NoteType::General),
        original_xref: None,
        _raw_gedcom: BTreeMap::new(),
    };

    if let Some((entity_id, entity_type)) = parse_optional_link(
        request.linked_entity_id,
        request.linked_entity_type.as_deref(),
    )? {
        ensure_entity_exists(&state, entity_id, entity_type).await?;
        note._raw_gedcom
            .insert("linked_entity_id".to_string(), entity_id.to_string());
        note._raw_gedcom.insert(
            "linked_entity_type".to_string(),
            entity_type_to_str(entity_type).to_string(),
        );
    }

    apply_optional_annotation_position(&mut note, request.position_x_pct, request.position_y_pct)?;

    state.storage.create_note(&note).await?;

    if let Some((entity_id, entity_type)) = parse_optional_link(
        request.linked_entity_id,
        request.linked_entity_type.as_deref(),
    )? {
        state
            .storage
            .create_assertion(
                entity_id,
                entity_type,
                "note_ref",
                &JsonAssertion {
                    id: EntityId::new(),
                    value: serde_json::to_value(NoteRef { note_id: note.id }).map_err(|err| {
                        ApiError::internal(format!(
                            "serialize note_ref assertion failed: {err}"
                        ))
                    })?,
                    confidence: 1.0,
                    status: AssertionStatus::Confirmed,
                    evidence_type: EvidenceType::Direct,
                    source_citations: Vec::new(),
                    proposed_by: ActorRef::User("api".to_string()),
                    created_at: Utc::now(),
                    reviewed_at: None,
                    reviewed_by: None,
                },
            )
            .await?;
    }

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "id": note.id })),
    ))
}

async fn get_note(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<NoteDetailResponse>, ApiError> {
    let note_id = parse_entity_id(&id)?;
    let note = state.storage.get_note(note_id).await?;
    Ok(Json(note_to_detail_response(note)))
}

async fn update_note(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<UpsertNoteRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let note_id = parse_entity_id(&id)?;
    let mut note = state.storage.get_note(note_id).await?;

    note.text = sanitize_note_text(&request.text)?;
    note.note_type = request.note_type.unwrap_or(note.note_type);

    if let Some((entity_id, entity_type)) = parse_optional_link(
        request.linked_entity_id,
        request.linked_entity_type.as_deref(),
    )? {
        ensure_entity_exists(&state, entity_id, entity_type).await?;
        note._raw_gedcom
            .insert("linked_entity_id".to_string(), entity_id.to_string());
        note._raw_gedcom.insert(
            "linked_entity_type".to_string(),
            entity_type_to_str(entity_type).to_string(),
        );

        state
            .storage
            .create_assertion(
                entity_id,
                entity_type,
                "note_ref",
                &JsonAssertion {
                    id: EntityId::new(),
                    value: serde_json::to_value(NoteRef { note_id: note.id }).map_err(|err| {
                        ApiError::internal(format!(
                            "serialize note_ref assertion failed: {err}"
                        ))
                    })?,
                    confidence: 1.0,
                    status: AssertionStatus::Confirmed,
                    evidence_type: EvidenceType::Direct,
                    source_citations: Vec::new(),
                    proposed_by: ActorRef::User("api".to_string()),
                    created_at: Utc::now(),
                    reviewed_at: None,
                    reviewed_by: None,
                },
            )
            .await?;
    }

    apply_optional_annotation_position(&mut note, request.position_x_pct, request.position_y_pct)?;

    state.storage.update_note(&note).await?;

    Ok(Json(serde_json::json!({ "id": note_id })))
}

async fn delete_note(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let note_id = parse_entity_id(&id)?;
    let _ = state.storage.get_note(note_id).await?;
    state.storage.delete_note(note_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

fn sanitize_note_text(text: &str) -> Result<String, ApiError> {
    let cleaned = ammonia::clean(text).trim().to_string();
    if cleaned.is_empty() {
        return Err(ApiError::BadRequest {
            message: "Note text must not be empty after sanitization. Provide meaningful content for the note.".to_string(),
            details: Some(serde_json::json!({ "original_text": text })),
        });
    }

    Ok(cleaned)
}

fn note_to_detail_response(note: Note) -> NoteDetailResponse {
    let linked_entity_id = note
        ._raw_gedcom
        .get("linked_entity_id")
        .and_then(|value| Uuid::parse_str(value).ok())
        .map(EntityId);
    let linked_entity_type = note._raw_gedcom.get("linked_entity_type").cloned();
    let position_x_pct = note
        ._raw_gedcom
        .get("position_x_pct")
        .and_then(|value| value.parse::<u8>().ok());
    let position_y_pct = note
        ._raw_gedcom
        .get("position_y_pct")
        .and_then(|value| value.parse::<u8>().ok());

    NoteDetailResponse {
        id: note.id,
        text: note.text,
        note_type: note.note_type,
        linked_entity_id,
        linked_entity_type,
        position_x_pct,
        position_y_pct,
    }
}

fn apply_optional_annotation_position(
    note: &mut Note,
    position_x_pct: Option<u8>,
    position_y_pct: Option<u8>,
) -> Result<(), ApiError> {
    match (position_x_pct, position_y_pct) {
        (None, None) => {
            note._raw_gedcom.remove("position_x_pct");
            note._raw_gedcom.remove("position_y_pct");
            Ok(())
        }
        (Some(x), Some(y)) => {
            note._raw_gedcom
                .insert("position_x_pct".to_string(), x.to_string());
            note._raw_gedcom
                .insert("position_y_pct".to_string(), y.to_string());
            Ok(())
        }
        _ => Err(ApiError::BadRequest {
            message: "Incomplete position provided. Both 'position_x_pct' and 'position_y_pct' must be provided together or both omitted.".to_string(),
            details: Some(serde_json::json!({ "x": position_x_pct, "y": position_y_pct })),
        }),
    }
}



fn parse_optional_link(
    linked_entity_id: Option<EntityId>,
    linked_entity_type: Option<&str>,
) -> Result<Option<(EntityId, EntityType)>, ApiError> {
    match (linked_entity_id, linked_entity_type) {
        (None, None) => Ok(None),
        (Some(id), None) => Err(ApiError::BadRequest {
            message: "Incomplete link provided. 'linked_entity_id' was provided but 'linked_entity_type' is missing.".to_string(),
            details: Some(serde_json::json!({ "linked_entity_id": id, "missing": "linked_entity_type" })),
        }),
        (None, Some(t)) => Err(ApiError::BadRequest {
            message: "Incomplete link provided. 'linked_entity_type' was provided but 'linked_entity_id' is missing.".to_string(),
            details: Some(serde_json::json!({ "linked_entity_type": t, "missing": "linked_entity_id" })),
        }),
        (Some(entity_id), Some(entity_type)) => {
            let parsed_type = parse_entity_type(entity_type)?;
            Ok(Some((entity_id, parsed_type)))
        }
    }
}

fn parse_entity_type(raw: &str) -> Result<EntityType, ApiError> {
    match raw.trim().to_lowercase().as_str() {
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
        "ldsordinance" | "lds_ordinance" => Ok(EntityType::LdsOrdinance),
        _ => Err(ApiError::BadRequest {
            message: format!("Invalid linked_entity_type: '{raw}'. Valid types include: person, family, relationship, event, place, source, citation, repository, media, note."),
            details: Some(serde_json::json!({ "invalid_type": raw, "allowed": ["person", "family", "relationship", "event", "place", "source", "citation", "repository", "media", "note", "lds_ordinance"] })),
        }),
    }
}

async fn ensure_entity_exists(
    state: &AppState,
    entity_id: EntityId,
    entity_type: EntityType,
) -> Result<(), ApiError> {
    match entity_type {
        EntityType::Person => {
            let _ = state.storage.get_person(entity_id).await?;
        }
        EntityType::Family => {
            let _ = state.storage.get_family(entity_id).await?;
        }
        EntityType::Relationship => {
            let _ = state.storage.get_relationship(entity_id).await?;
        }
        EntityType::Event => {
            let _ = state.storage.get_event(entity_id).await?;
        }
        EntityType::Place => {
            let _ = state.storage.get_place(entity_id).await?;
        }
        EntityType::Source => {
            let _ = state.storage.get_source(entity_id).await?;
        }
        EntityType::Citation => {
            let _ = state.storage.get_citation(entity_id).await?;
        }
        EntityType::Repository => {
            let _ = state.storage.get_repository(entity_id).await?;
        }
        EntityType::Media => {
            let _ = state.storage.get_media(entity_id).await?;
        }
        EntityType::Note => {
            let _ = state.storage.get_note(entity_id).await?;
        }
        EntityType::LdsOrdinance => {
            let _ = state.storage.get_lds_ordinance(entity_id).await?;
        }
    }

    Ok(())
}

fn entity_type_to_str(entity_type: EntityType) -> &'static str {
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
