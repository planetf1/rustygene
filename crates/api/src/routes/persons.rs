use std::collections::BTreeMap;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use chrono::Utc;
use rustygene_core::assertion::{AssertionStatus, EvidenceType};
use rustygene_core::person::{Person, PersonName};
use rustygene_core::types::{ActorRef, EntityId, Gender};
use rustygene_storage::{EntityType, FieldAssertion, JsonAssertion, Pagination};
use serde::Deserialize;
use uuid::Uuid;

use crate::errors::ApiError;
use crate::models::persons::{
    AssertionValueResponse, CreateAssertionRequest, CreatePersonRequest, CreatedPersonResponse,
    FamilySummaryResponse, GenderAssertionResponse, PersonDetailResponse, PersonNameAssertion,
    PersonResponse, TimelineEventResponse,
};
use crate::AppState;

#[derive(Debug, Deserialize)]
struct PersonsQuery {
    #[serde(default)]
    limit: Option<u32>,
    #[serde(default)]
    offset: Option<u32>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_persons).post(create_person))
        .route(
            "/:id",
            get(get_person).put(update_person).delete(delete_person),
        )
        .route("/:id/assertions", get(get_person_assertions).post(create_person_assertion))
        .route("/:id/timeline", get(get_person_timeline))
}

async fn list_persons(
    State(state): State<AppState>,
    Query(query): Query<PersonsQuery>,
) -> Result<Json<Vec<PersonResponse>>, ApiError> {
    let persons = state
        .storage
        .list_persons(Pagination {
            limit: query.limit.unwrap_or(100),
            offset: query.offset.unwrap_or(0),
        })
        .await?;

    let mut response = Vec::with_capacity(persons.len());
    for person in persons {
        let display_name = display_name_for_person(&person);
        let assertions = state
            .storage
            .list_assertion_records_for_entity(person.id)
            .await?;
        let assertion_counts = assertion_counts(&assertions);
        let events = state.storage.list_events_for_person(person.id).await?;
        let (birth_year, death_year) = event_years(&events);

        response.push(PersonResponse {
            id: person.id,
            display_name,
            birth_year,
            death_year,
            assertion_counts,
        });
    }

    Ok(Json(response))
}

async fn create_person(
    State(state): State<AppState>,
    Json(request): Json<CreatePersonRequest>,
) -> Result<impl IntoResponse, ApiError> {
    validate_create_person_request(&request)?;

    let person_id = EntityId::new();
    let person_name = request.to_person_name();
    let person = Person {
        id: person_id,
        names: vec![person_name.clone()],
        gender: request.gender.clone().unwrap_or(Gender::Unknown),
        living: true,
        private: false,
        original_xref: None,
        _raw_gedcom: Default::default(),
    };

    state.storage.create_person(&person).await?;
    state
        .storage
        .create_assertion(
            person_id,
            EntityType::Person,
            "name",
            &json_assertion(person_name, Some(0.95), None, None, Vec::new(), None)?,
        )
        .await?;

    if let Some(gender) = request.gender {
        state
            .storage
            .create_assertion(
                person_id,
                EntityType::Person,
                "gender",
                &json_assertion(gender, Some(0.95), None, None, Vec::new(), None)?,
            )
            .await?;
    }

    Ok((
        StatusCode::CREATED,
        Json(CreatedPersonResponse { id: person_id }),
    ))
}

async fn get_person(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<PersonDetailResponse>, ApiError> {
    let person_id = parse_entity_id(&id)?;
    let person = state.storage.get_person(person_id).await?;
    let assertions = state
        .storage
        .list_assertion_records_for_entity(person_id)
        .await?;
    let events = state.storage.list_events_for_person(person_id).await?;
    let families = state.storage.list_families_for_person(person_id).await?;

    Ok(Json(PersonDetailResponse {
        id: person.id,
        names: name_assertions(&assertions),
        events: events.iter().map(TimelineEventResponse::from).collect(),
        gender_assertions: gender_assertions(&assertions),
        families: families.iter().map(FamilySummaryResponse::from).collect(),
    }))
}

async fn update_person(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<CreatePersonRequest>,
) -> Result<Json<CreatedPersonResponse>, ApiError> {
    validate_create_person_request(&request)?;

    let person_id = parse_entity_id(&id)?;
    let mut person = state.storage.get_person(person_id).await?;
    let person_name = request.to_person_name();

    person.names.push(person_name.clone());
    if let Some(gender) = request.gender.clone() {
        person.gender = gender;
    }

    state.storage.update_person(&person).await?;
    state
        .storage
        .create_assertion(
            person_id,
            EntityType::Person,
            "name",
            &json_assertion(person_name, Some(0.95), None, None, Vec::new(), None)?,
        )
        .await?;

    if let Some(gender) = request.gender {
        state
            .storage
            .create_assertion(
                person_id,
                EntityType::Person,
                "gender",
                &json_assertion(gender, Some(0.95), None, None, Vec::new(), None)?,
            )
            .await?;
    }

    Ok(Json(CreatedPersonResponse { id: person_id }))
}

async fn delete_person(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let person_id = parse_entity_id(&id)?;
    let _ = state.storage.get_person(person_id).await?;
    state.storage.delete_person(person_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_person_assertions(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<BTreeMap<String, Vec<AssertionValueResponse>>>, ApiError> {
    let person_id = parse_entity_id(&id)?;
    let _ = state.storage.get_person(person_id).await?;
    let records = state
        .storage
        .list_assertion_records_for_entity(person_id)
        .await?;

    let mut grouped: BTreeMap<String, Vec<AssertionValueResponse>> = BTreeMap::new();
    for record in records {
        grouped.entry(record.field.clone()).or_default().push(AssertionValueResponse {
            assertion_id: record.assertion.id,
            field: record.field,
            value: record.assertion.value,
            status: record.assertion.status,
            confidence: record.assertion.confidence,
            sources: record.assertion.source_citations,
        });
    }

    Ok(Json(grouped))
}

async fn create_person_assertion(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<CreateAssertionRequest>,
) -> Result<impl IntoResponse, ApiError> {
    if request.field.trim().is_empty() {
        return Err(ApiError::BadRequest(
            "assertion field must not be empty".to_string(),
        ));
    }

    let person_id = parse_entity_id(&id)?;
    let _ = state.storage.get_person(person_id).await?;
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
        .create_assertion(person_id, EntityType::Person, &request.field, &assertion)
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(AssertionValueResponse {
            assertion_id: assertion.id,
            field: request.field,
            value: assertion.value,
            status: assertion.status,
            confidence: assertion.confidence,
            sources: assertion.source_citations,
        }),
    ))
}

async fn get_person_timeline(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Vec<TimelineEventResponse>>, ApiError> {
    let person_id = parse_entity_id(&id)?;
    let _ = state.storage.get_person(person_id).await?;
    let events = state.storage.list_events_for_person(person_id).await?;
    Ok(Json(
        events.iter().map(TimelineEventResponse::from).collect(),
    ))
}

fn parse_entity_id(raw: &str) -> Result<EntityId, ApiError> {
    Uuid::parse_str(raw)
        .map(EntityId)
        .map_err(|_| ApiError::BadRequest(format!("invalid entity id: {raw}")))
}

fn validate_create_person_request(request: &CreatePersonRequest) -> Result<(), ApiError> {
    if request.given_names.is_empty() {
        return Err(ApiError::BadRequest(
            "given_names must contain at least one value".to_string(),
        ));
    }

    if request.surnames.is_empty() {
        return Err(ApiError::BadRequest(
            "surnames must contain at least one value".to_string(),
        ));
    }

    Ok(())
}

fn display_name_for_person(person: &Person) -> String {
    let primary = person.primary_name();
    let surname = primary
        .surnames
        .iter()
        .map(|surname| surname.value.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    if surname.is_empty() {
        primary.given_names
    } else if primary.given_names.is_empty() {
        surname
    } else {
        format!("{} {}", primary.given_names, surname)
    }
}

fn assertion_counts(assertions: &[FieldAssertion]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for record in assertions {
        *counts.entry(record.field.clone()).or_insert(0) += 1;
    }
    counts
}

fn event_years(events: &[rustygene_core::event::Event]) -> (Option<i32>, Option<i32>) {
    let mut birth_year = None;
    let mut death_year = None;

    for event in events {
        let year = match event.date.as_ref() {
            Some(rustygene_core::types::DateValue::Exact { date, .. })
            | Some(rustygene_core::types::DateValue::Before { date, .. })
            | Some(rustygene_core::types::DateValue::After { date, .. })
            | Some(rustygene_core::types::DateValue::About { date, .. })
            | Some(rustygene_core::types::DateValue::Tolerance { date, .. }) => Some(date.year),
            Some(rustygene_core::types::DateValue::Range { from, .. }) => Some(from.year),
            Some(rustygene_core::types::DateValue::Quarter { year, .. }) => Some(*year),
            Some(rustygene_core::types::DateValue::Textual { .. }) | None => None,
        };

        match event.event_type {
            rustygene_core::event::EventType::Birth if birth_year.is_none() => birth_year = year,
            rustygene_core::event::EventType::Death if death_year.is_none() => death_year = year,
            _ => {}
        }
    }

    (birth_year, death_year)
}

fn name_assertions(assertions: &[FieldAssertion]) -> Vec<PersonNameAssertion> {
    assertions
        .iter()
        .filter(|record| record.field == "name")
        .filter_map(|record| {
            serde_json::from_value::<PersonName>(record.assertion.value.clone())
                .ok()
                .map(|name| PersonNameAssertion {
                    assertion_id: record.assertion.id,
                    given_names: name
                        .given_names
                        .split_whitespace()
                        .map(ToString::to_string)
                        .collect(),
                    surnames: name.surnames,
                    name_type: Some(name.name_type),
                    sort_as: name.sort_as,
                    call_name: name.call_name,
                    confidence: record.assertion.confidence,
                    sources: record.assertion.source_citations.clone(),
                })
        })
        .collect()
}

fn gender_assertions(assertions: &[FieldAssertion]) -> Vec<GenderAssertionResponse> {
    assertions
        .iter()
        .filter(|record| record.field == "gender")
        .filter_map(|record| {
            serde_json::from_value::<Gender>(record.assertion.value.clone())
                .ok()
                .map(|gender| GenderAssertionResponse {
                    assertion_id: record.assertion.id,
                    value: gender,
                    confidence: record.assertion.confidence,
                    sources: record.assertion.source_citations.clone(),
                })
        })
        .collect()
}

fn json_assertion<T: serde::Serialize>(
    value: T,
    confidence: Option<f64>,
    status: Option<AssertionStatus>,
    evidence_type: Option<EvidenceType>,
    source_citations: Vec<rustygene_core::evidence::CitationRef>,
    proposed_by: Option<ActorRef>,
) -> Result<JsonAssertion, ApiError> {
    Ok(JsonAssertion {
        id: EntityId::new(),
        value: serde_json::to_value(value)
            .map_err(|err| ApiError::InternalError(format!("serialize assertion failed: {err}")))?,
        confidence: confidence.unwrap_or(0.9),
        status: status.unwrap_or(AssertionStatus::Confirmed),
        evidence_type: evidence_type.unwrap_or(EvidenceType::Direct),
        source_citations,
        proposed_by: proposed_by.unwrap_or_else(|| ActorRef::User("api".to_string())),
        created_at: Utc::now(),
        reviewed_at: None,
        reviewed_by: None,
    })
}
