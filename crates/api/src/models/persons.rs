use std::collections::BTreeMap;

use rustygene_core::assertion::AssertionStatus;
use rustygene_core::event::{Event, EventType};
use rustygene_core::evidence::CitationRef;
use rustygene_core::family::Family;
use rustygene_core::person::{NameType, PersonName, Surname, SurnameOrigin};
use rustygene_core::types::{DateValue, EntityId, Gender};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use rustygene_core::assertion::EvidenceType;
use rustygene_core::types::ActorRef;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SurnameInput {
    pub value: String,
    #[serde(default)]
    pub origin_type: Option<SurnameOrigin>,
    #[serde(default)]
    pub connector: Option<String>,
}

impl From<SurnameInput> for Surname {
    fn from(value: SurnameInput) -> Self {
        Self {
            value: value.value,
            origin_type: value.origin_type.unwrap_or_default(),
            connector: value.connector,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePersonRequest {
    pub given_names: Vec<String>,
    pub surnames: Vec<SurnameInput>,
    #[serde(default)]
    pub name_type: Option<NameType>,
    #[serde(default)]
    pub birth_date: Option<DateValue>,
    #[serde(default)]
    pub birth_place: Option<String>,
    #[serde(default)]
    pub gender: Option<Gender>,
    #[serde(default)]
    pub call_name: Option<String>,
    #[serde(default)]
    pub sort_as: Option<String>,
    #[serde(default)]
    pub prefix: Option<String>,
    #[serde(default)]
    pub suffix: Option<String>,
}

impl CreatePersonRequest {
    #[must_use]
    pub fn to_person_name(&self) -> PersonName {
        PersonName {
            name_type: self.name_type.clone().unwrap_or_default(),
            date_range: None,
            given_names: self.given_names.join(" ").trim().to_string(),
            call_name: self.call_name.clone(),
            surnames: self.surnames.clone().into_iter().map(Into::into).collect(),
            prefix: self.prefix.clone(),
            suffix: self.suffix.clone(),
            sort_as: self.sort_as.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PersonResponse {
    pub id: EntityId,
    pub display_name: String,
    pub birth_year: Option<i32>,
    pub death_year: Option<i32>,
    pub assertion_counts: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PersonListResponse {
    pub total: usize,
    pub items: Vec<PersonResponse>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PersonNameAssertion {
    pub assertion_id: EntityId,
    pub given_names: Vec<String>,
    pub surnames: Vec<Surname>,
    pub name_type: Option<NameType>,
    pub sort_as: Option<String>,
    pub call_name: Option<String>,
    pub confidence: f64,
    pub sources: Vec<CitationRef>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GenderAssertionResponse {
    pub assertion_id: EntityId,
    pub value: Gender,
    pub confidence: f64,
    pub sources: Vec<CitationRef>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FamilySummaryResponse {
    pub id: EntityId,
}

impl From<&Family> for FamilySummaryResponse {
    fn from(value: &Family) -> Self {
        Self { id: value.id }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TimelineEventResponse {
    pub id: EntityId,
    pub event_type: EventType,
    pub date: Option<DateValue>,
    pub description: Option<String>,
}

impl From<&Event> for TimelineEventResponse {
    fn from(value: &Event) -> Self {
        Self {
            id: value.id,
            event_type: value.event_type.clone(),
            date: value.date.clone(),
            description: value.description.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AssertionValueResponse {
    pub assertion_id: EntityId,
    pub field: String,
    pub value: Value,
    pub status: AssertionStatus,
    pub confidence: f64,
    pub evidence_type: EvidenceType,
    pub sources: Vec<CitationRef>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreatedPersonResponse {
    pub id: EntityId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateAssertionRequest {
    pub field: String,
    pub value: Value,
    #[serde(default)]
    pub confidence: Option<f64>,
    #[serde(default)]
    pub status: Option<AssertionStatus>,
    #[serde(default)]
    pub evidence_type: Option<EvidenceType>,
    #[serde(default)]
    pub source_citations: Vec<CitationRef>,
    #[serde(default)]
    pub proposed_by: Option<ActorRef>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PersonDetailResponse {
    pub id: EntityId,
    pub names: Vec<PersonNameAssertion>,
    pub events: Vec<TimelineEventResponse>,
    pub gender_assertions: Vec<GenderAssertionResponse>,
    pub families: Vec<FamilySummaryResponse>,
}
