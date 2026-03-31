use crate::types::EntityId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Default, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineageType {
    #[default]
    Biological,
    Adopted,
    Foster,
    Step,
    Unknown,
    Custom(String),
}

#[derive(Default, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PartnerLink {
    Married,
    Unmarried,
    #[default]
    Unknown,
    Custom(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationshipType {
    Couple,
    ParentChild,
    Godparent,
    Guardian,
    Sibling,
    Associate,
    Custom(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChildLink {
    pub child_id: EntityId,
    pub lineage_type: LineageType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Relationship {
    pub id: EntityId,
    pub person1_id: EntityId,
    pub person2_id: EntityId,
    pub relationship_type: RelationshipType,
    /// Optional supporting event (e.g., marriage event for couple relationship)
    pub supporting_event: Option<EntityId>,
    #[serde(default)]
    pub _raw_gedcom: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Family {
    pub id: EntityId,
    pub partner1_id: Option<EntityId>,
    pub partner2_id: Option<EntityId>,
    pub partner_link: PartnerLink,
    /// Reference to the canonical couple relationship edge.
    pub couple_relationship: Option<EntityId>,
    #[serde(default)]
    pub child_links: Vec<ChildLink>,
    /// Original GEDCOM xref ID (e.g., "@F12@") for round-trip preservation
    #[serde(default)]
    pub original_xref: Option<String>,
    #[serde(default)]
    pub _raw_gedcom: BTreeMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_round_trip_family_and_relationship() {
        let person1 = EntityId::new();
        let person2 = EntityId::new();
        let child = EntityId::new();
        let event_id = EntityId::new();

        let relationship = Relationship {
            id: EntityId::new(),
            person1_id: person1,
            person2_id: person2,
            relationship_type: RelationshipType::Couple,
            supporting_event: Some(event_id),
            _raw_gedcom: BTreeMap::new(),
        };

        let family = Family {
            id: EntityId::new(),
            partner1_id: Some(person1),
            partner2_id: Some(person2),
            partner_link: PartnerLink::Married,
            couple_relationship: Some(relationship.id),
            child_links: vec![ChildLink {
                child_id: child,
                lineage_type: LineageType::Biological,
            }],
            original_xref: Some("@F1@".to_string()),
            _raw_gedcom: BTreeMap::new(),
        };

        let relationship_json =
            serde_json::to_string(&relationship).expect("serialize relationship");
        let family_json = serde_json::to_string(&family).expect("serialize family");

        let relationship_back: Relationship =
            serde_json::from_str(&relationship_json).expect("deserialize relationship");
        let family_back: Family = serde_json::from_str(&family_json).expect("deserialize family");

        assert_eq!(relationship_back, relationship);
        assert_eq!(family_back, family);
    }

    #[test]
    fn family_relationship_event_reference_integrity_by_id() {
        let event_id = EntityId::new();
        let relationship = Relationship {
            id: EntityId::new(),
            person1_id: EntityId::new(),
            person2_id: EntityId::new(),
            relationship_type: RelationshipType::Couple,
            supporting_event: Some(event_id),
            _raw_gedcom: BTreeMap::new(),
        };

        let family = Family {
            id: EntityId::new(),
            partner1_id: Some(relationship.person1_id),
            partner2_id: Some(relationship.person2_id),
            partner_link: PartnerLink::Married,
            couple_relationship: Some(relationship.id),
            child_links: vec![],
            original_xref: Some("@F2@".to_string()),
            _raw_gedcom: BTreeMap::new(),
        };

        assert_eq!(family.couple_relationship, Some(relationship.id));
        assert_eq!(relationship.supporting_event, Some(event_id));
    }
}
