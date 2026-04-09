use std::collections::BTreeMap;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use chrono::Utc;
use rustygene_core::assertion::{AssertionStatus, EvidenceType};
use rustygene_core::event::{Event, EventParticipant, EventRole, EventType};
use rustygene_core::evidence::CitationRef;
use rustygene_core::types::ActorRef;
use rustygene_core::types::EntityId;
use rustygene_core::types::{Calendar, DateValue, FuzzyDate};
use rustygene_storage::{EntityType, JsonAssertion, Pagination};
use serde::Deserialize;

use crate::errors::{ApiError, parse_entity_id};
use crate::models::events::{
    AddParticipantRequest, CreateEventRequest, EventDetailResponse, EventParticipantResponse,
};
use crate::models::persons::{AssertionValueResponse, CreateAssertionRequest};
use crate::AppState;

#[derive(Debug, Deserialize)]
struct EventsQuery {
    #[serde(default)]
    limit: Option<u32>,
    #[serde(default)]
    offset: Option<u32>,
    #[serde(default)]
    #[serde(rename = "person_id")]
    _person_id: Option<String>,
    #[serde(default)]
    #[serde(rename = "family_id")]
    _family_id: Option<String>,
    #[serde(default)]
    #[serde(rename = "event_type")]
    _event_type: Option<String>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_events).post(create_event))
        .route(
            "/:id",
            get(get_event).put(update_event).delete(delete_event),
        )
        .route(
            "/:id/assertions",
            get(get_event_assertions).post(create_event_assertion),
        )
        .route("/:id/participants", post(add_participant))
        .route("/:id/participants/:pid", delete(remove_participant))
}

async fn list_events(
    State(state): State<AppState>,
    Query(query): Query<EventsQuery>,
) -> Result<Json<Vec<EventDetailResponse>>, ApiError> {
    let pagination = Pagination {
        limit: query.limit.unwrap_or(100),
        offset: query.offset.unwrap_or(0),
    };

    let events = state.storage.list_events(pagination).await?;

    let mut response = Vec::with_capacity(events.len());
    for event in events {
        let assertions = state
            .storage
            .list_assertion_records_for_entity(event.id)
            .await?;

        let confidence = assertions
            .iter()
            .find(|a| a.field == "date")
            .map(|a| a.assertion.confidence)
            .unwrap_or(0.8);

        let citations = collect_event_citations(&assertions);

        response.push(EventDetailResponse {
            id: event.id,
            event_type: format!("{:?}", event.event_type),
            date: event.date.as_ref().map(format_date_value),
            place_id: event.place_ref,
            participants: event
                .participants
                .iter()
                .map(EventParticipantResponse::from)
                .collect(),
            citations,
            confidence,
        });
    }

    Ok(Json(response))
}

async fn create_event(
    State(state): State<AppState>,
    Json(request): Json<CreateEventRequest>,
) -> Result<impl IntoResponse, ApiError> {
    // Validate event_type
    let event_type = parse_event_type(&request.event_type)?;

    let event_id = EntityId::new();
    let event = Event {
        id: event_id,
        event_type,
        date: request.date.as_deref().map(parse_date_value),
        place_ref: request.place_id,
        participants: Vec::new(),
        description: request.description,
        _raw_gedcom: BTreeMap::new(),
    };

    state.storage.create_event(&event).await?;

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "id": event_id })),
    ))
}

async fn get_event(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<EventDetailResponse>, ApiError> {
    let event_id = parse_entity_id(&id)?;
    let event = state.storage.get_event(event_id).await?;

    let assertions = state
        .storage
        .list_assertion_records_for_entity(event_id)
        .await?;

    let confidence = assertions
        .iter()
        .find(|a| a.field == "date")
        .map(|a| a.assertion.confidence)
        .unwrap_or(0.8);

    let citations = collect_event_citations(&assertions);

    Ok(Json(EventDetailResponse {
        id: event.id,
        event_type: format!("{:?}", event.event_type),
        date: event.date.as_ref().map(format_date_value),
        place_id: event.place_ref,
        participants: event
            .participants
            .iter()
            .map(EventParticipantResponse::from)
            .collect(),
        citations,
        confidence,
    }))
}

async fn update_event(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<CreateEventRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let event_id = parse_entity_id(&id)?;
    let mut event = state.storage.get_event(event_id).await?;

    // Validate and update event_type
    event.event_type = parse_event_type(&request.event_type)?;

    // Update other fields if provided
    if let Some(description) = request.description {
        event.description = Some(description);
    }
    if let Some(place_id) = request.place_id {
        event.place_ref = Some(place_id);
    }
    if let Some(date_str) = request.date {
        event.date = Some(parse_date_value(&date_str));
    }

    state.storage.update_event(&event).await?;

    Ok(Json(serde_json::json!({ "id": event_id })))
}

async fn delete_event(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let event_id = parse_entity_id(&id)?;
    let _ = state.storage.get_event(event_id).await?;
    state.storage.delete_event(event_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn add_participant(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<AddParticipantRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let event_id = parse_entity_id(&id)?;
    let mut event = state.storage.get_event(event_id).await?;

    // Validate person exists
    let _ = state.storage.get_person(request.person_id).await?;

    // Validate and parse role
    let role = if let Some(role_str) = request.role {
        parse_event_role(&role_str)?
    } else {
        EventRole::Principal
    };

    // Add participant
    event.participants.push(EventParticipant {
        person_id: request.person_id,
        role,
        census_role: None,
    });

    state.storage.update_event(&event).await?;

    Ok((StatusCode::CREATED, Json(serde_json::json!({}))))
}

async fn remove_participant(
    State(state): State<AppState>,
    Path((id, pid)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    let event_id = parse_entity_id(&id)?;
    let participant_id = parse_entity_id(&pid)?;

    let mut event = state.storage.get_event(event_id).await?;

    // Remove participant
    event.participants.retain(|p| p.person_id != participant_id);

    state.storage.update_event(&event).await?;

    Ok(StatusCode::NO_CONTENT)
}

async fn get_event_assertions(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<BTreeMap<String, Vec<AssertionValueResponse>>>, ApiError> {
    let event_id = parse_entity_id(&id)?;
    let _ = state.storage.get_event(event_id).await?;
    let records = state
        .storage
        .list_assertion_records_for_entity(event_id)
        .await?;

    let mut grouped: BTreeMap<String, Vec<AssertionValueResponse>> = BTreeMap::new();
    for record in records {
        grouped
            .entry(record.field.clone())
            .or_default()
            .push(AssertionValueResponse {
                assertion_id: record.assertion.id,
                field: record.field,
                value: record.assertion.value.clone(),
                status: record.assertion.status.clone(),
                confidence: record.assertion.confidence,
                evidence_type: record.assertion.evidence_type.clone(),
                sources: record.assertion.source_citations.clone(),
            });
    }

    Ok(Json(grouped))
}

async fn create_event_assertion(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<CreateAssertionRequest>,
) -> Result<impl IntoResponse, ApiError> {
    if request.field.trim().is_empty() {
        return Err(ApiError::BadRequest {
            message: "Assertion field must not be empty. Provide a non-empty string for the field name.".to_string(),
            details: Some(serde_json::json!({ "field": request.field })),
        });
    }

    let event_id = parse_entity_id(&id)?;
    let _ = state.storage.get_event(event_id).await?;

    let assertion = JsonAssertion {
        id: EntityId::new(),
        value: request.value,
        confidence: request.confidence.unwrap_or(0.8),
        status: request.status.unwrap_or(AssertionStatus::Proposed),
        evidence_type: request.evidence_type.unwrap_or(EvidenceType::Direct),
        source_citations: request.source_citations,
        proposed_by: request
            .proposed_by
            .unwrap_or_else(|| ActorRef::User("api".to_string())),
        created_at: Utc::now(),
        reviewed_at: None,
        reviewed_by: None,
    };

    state
        .storage
        .create_assertion(event_id, EntityType::Event, &request.field, &assertion)
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(AssertionValueResponse {
            assertion_id: assertion.id,
            field: request.field,
            value: assertion.value,
            status: assertion.status,
            confidence: assertion.confidence,
            evidence_type: assertion.evidence_type,
            sources: assertion.source_citations,
        }),
    ))
}

// Helpers



fn parse_event_type(event_type_str: &str) -> Result<EventType, ApiError> {
    match event_type_str.to_lowercase().as_str() {
        "birth" => Ok(EventType::Birth),
        "death" => Ok(EventType::Death),
        "marriage" => Ok(EventType::Marriage),
        "census" => Ok(EventType::Census),
        "baptism" => Ok(EventType::Baptism),
        "burial" => Ok(EventType::Burial),
        "migration" => Ok(EventType::Migration),
        "occupation" => Ok(EventType::Occupation),
        "residence" => Ok(EventType::Residence),
        "immigration" => Ok(EventType::Immigration),
        "emigration" => Ok(EventType::Emigration),
        "naturalization" => Ok(EventType::Naturalization),
        "probate" => Ok(EventType::Probate),
        "will" => Ok(EventType::Will),
        "graduation" => Ok(EventType::Graduation),
        "retirement" => Ok(EventType::Retirement),
        custom => Ok(EventType::Custom(custom.to_string())),
    }
}

fn parse_event_role(role_str: &str) -> Result<EventRole, ApiError> {
    match role_str.to_lowercase().as_str() {
        "principal" => Ok(EventRole::Principal),
        "witness" => Ok(EventRole::Witness),
        "godparent" => Ok(EventRole::Godparent),
        "informant" => Ok(EventRole::Informant),
        "clergy" => Ok(EventRole::Clergy),
        "registrar" => Ok(EventRole::Registrar),
        "celebrant" => Ok(EventRole::Celebrant),
        "parent" => Ok(EventRole::Parent),
        "spouse" => Ok(EventRole::Spouse),
        "child" => Ok(EventRole::Child),
        "servant" => Ok(EventRole::Servant),
        "boarder" => Ok(EventRole::Boarder),
        custom => Ok(EventRole::Custom(custom.to_string())),
    }
}

fn parse_date_value(input: &str) -> DateValue {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return DateValue::Textual {
            value: String::new(),
        };
    }

    if let Some((year, month, day)) = parse_iso_parts(trimmed) {
        return DateValue::Exact {
            date: FuzzyDate::new(year, month, day),
            calendar: Calendar::Gregorian,
        };
    }

    DateValue::Textual {
        value: trimmed.to_string(),
    }
}

fn parse_iso_parts(value: &str) -> Option<(i32, Option<u8>, Option<u8>)> {
    let segments: Vec<&str> = value.split('-').collect();
    match segments.as_slice() {
        [year] => Some((year.parse::<i32>().ok()?, None, None)),
        [year, month] => Some((
            year.parse::<i32>().ok()?,
            Some(month.parse::<u8>().ok()?),
            None,
        )),
        [year, month, day] => Some((
            year.parse::<i32>().ok()?,
            Some(month.parse::<u8>().ok()?),
            Some(day.parse::<u8>().ok()?),
        )),
        _ => None,
    }
}

fn format_date_value(date: &DateValue) -> String {
    match date {
        DateValue::Exact { date, .. }
        | DateValue::Before { date, .. }
        | DateValue::After { date, .. }
        | DateValue::About { date, .. }
        | DateValue::Tolerance { date, .. } => match (date.month, date.day) {
            (Some(month), Some(day)) => format!("{:04}-{:02}-{:02}", date.year, month, day),
            (Some(month), None) => format!("{:04}-{:02}", date.year, month),
            (None, _) => format!("{:04}", date.year),
        },
        DateValue::Range { from, to, .. } => format!(
            "{:04}-{:02}-{:02}/{:04}-{:02}-{:02}",
            from.year,
            from.month.unwrap_or(1),
            from.day.unwrap_or(1),
            to.year,
            to.month.unwrap_or(12),
            to.day.unwrap_or(31)
        ),
        DateValue::Quarter { year, quarter } => format!("Q{} {}", quarter, year),
        DateValue::Textual { value } => value.clone(),
    }
}

fn collect_event_citations(assertions: &[rustygene_storage::FieldAssertion]) -> Vec<CitationRef> {
    let mut citations: Vec<CitationRef> = Vec::new();
    for record in assertions {
        for citation in &record.assertion.source_citations {
            if !citations.iter().any(|existing| existing == citation) {
                citations.push(citation.clone());
            }
        }
    }
    citations
}
