use crate::types::{DateValue, EntityId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    Birth,
    Death,
    Marriage,
    Census,
    Baptism,
    Burial,
    Migration,
    Occupation,
    Residence,
    Immigration,
    Emigration,
    Naturalization,
    Probate,
    Will,
    Graduation,
    Retirement,
    Custom(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventRole {
    Principal,
    Witness,
    Godparent,
    Informant,
    Clergy,
    Registrar,
    Celebrant,
    Parent,
    Spouse,
    Child,
    Servant,
    Boarder,
    Custom(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CensusRole {
    Head,
    Wife,
    Husband,
    Son,
    Daughter,
    Servant,
    Boarder,
    Visitor,
    Lodger,
    Inmate,
    Patient,
    Scholar,
    Apprentice,
    OtherRelative,
    Other,
    Custom(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventParticipant {
    pub person_id: EntityId,
    pub role: EventRole,
    pub census_role: Option<CensusRole>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    pub id: EntityId,
    pub event_type: EventType,
    pub date: Option<DateValue>,
    pub place_ref: Option<EntityId>,
    #[serde(default)]
    pub participants: Vec<EventParticipant>,
    pub description: Option<String>,
    #[serde(default)]
    pub _raw_gedcom: BTreeMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Calendar, FuzzyDate};

    #[test]
    fn serde_round_trip_event_with_participants() {
        let person_a = EntityId::new();
        let person_b = EntityId::new();
        let place = EntityId::new();

        let event = Event {
            id: EntityId::new(),
            event_type: EventType::Census,
            date: Some(DateValue::Exact {
                date: FuzzyDate::new(1881, Some(4), Some(3)),
                calendar: Calendar::Gregorian,
            }),
            place_ref: Some(place),
            participants: vec![
                EventParticipant {
                    person_id: person_a,
                    role: EventRole::Principal,
                    census_role: Some(CensusRole::Head),
                },
                EventParticipant {
                    person_id: person_b,
                    role: EventRole::Principal,
                    census_role: Some(CensusRole::Wife),
                },
            ],
            description: Some("1881 census household".to_string()),
            _raw_gedcom: BTreeMap::new(),
        };

        let json = serde_json::to_string(&event).expect("serialize event");
        let back: Event = serde_json::from_str(&json).expect("deserialize event");

        assert_eq!(back, event);
    }

    #[test]
    fn supports_custom_event_and_roles() {
        let participant = EventParticipant {
            person_id: EntityId::new(),
            role: EventRole::Custom("executor".to_string()),
            census_role: Some(CensusRole::Custom("nephew".to_string())),
        };

        let event = Event {
            id: EntityId::new(),
            event_type: EventType::Custom("military_service".to_string()),
            date: None,
            place_ref: None,
            participants: vec![participant],
            description: None,
            _raw_gedcom: BTreeMap::new(),
        };

        let json = serde_json::to_string(&event).expect("serialize event");
        let back: Event = serde_json::from_str(&json).expect("deserialize event");
        assert_eq!(back, event);
    }
}
