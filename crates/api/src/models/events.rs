use rustygene_core::event::EventParticipant;
use rustygene_core::evidence::CitationRef;
use rustygene_core::types::EntityId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateEventRequest {
    pub event_type: String,
    #[serde(default)]
    pub date: Option<String>,
    #[serde(default)]
    pub place_id: Option<EntityId>,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddParticipantRequest {
    pub person_id: EntityId,
    #[serde(default)]
    pub role: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventDetailResponse {
    pub id: EntityId,
    pub event_type: String,
    pub date: Option<String>,
    pub place_id: Option<EntityId>,
    pub participants: Vec<EventParticipantResponse>,
    pub citations: Vec<CitationRef>,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventParticipantResponse {
    pub person_id: EntityId,
    pub role: String,
}

impl EventParticipantResponse {
    pub fn from(participant: &EventParticipant) -> Self {
        Self {
            person_id: participant.person_id,
            role: format!("{:?}", participant.role),
        }
    }
}
