use crate::types::{DateValue, EntityId};
use serde::{Deserialize, Serialize};

#[derive(Default, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaceType {
    #[default]
    Unknown,
    Country,
    State,
    Province,
    County,
    District,
    Parish,
    Town,
    City,
    Village,
    Hamlet,
    Farm,
    Cemetery,
    Church,
    Custom(String),
}

#[derive(Default, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HierarchyType {
    #[default]
    Admin,
    Religious,
    Geographic,
    Judicial,
    Cultural,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaceName {
    pub name: String,
    pub language: Option<String>,
    /// Present in the model from day one; Phase 1A leaves this as None in storage/import.
    pub date_range: Option<DateValue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaceRef {
    pub place_id: EntityId,
    pub hierarchy_type: HierarchyType,
    /// Present in the model from day one; Phase 1A leaves this as None in storage/import.
    pub date_range: Option<DateValue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalId {
    pub system: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Place {
    pub id: EntityId,
    pub place_type: PlaceType,
    #[serde(default)]
    pub names: Vec<PlaceName>,
    pub coordinates: Option<(f64, f64)>,
    #[serde(default)]
    pub enclosed_by: Vec<PlaceRef>,
    #[serde(default)]
    pub external_ids: Vec<ExternalId>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_round_trip_place_subset() {
        let parent_id = EntityId::new();
        let place = Place {
            id: EntityId::new(),
            place_type: PlaceType::Village,
            names: vec![PlaceName {
                name: "Llanpumsaint".to_string(),
                language: Some("cy".to_string()),
                date_range: None,
            }],
            coordinates: Some((51.9517, -4.2885)),
            enclosed_by: vec![PlaceRef {
                place_id: parent_id,
                hierarchy_type: HierarchyType::Admin,
                date_range: None,
            }],
            external_ids: vec![ExternalId {
                system: "geonames".to_string(),
                value: "2644697".to_string(),
            }],
        };

        let json = serde_json::to_string(&place).expect("serialize place");
        let back: Place = serde_json::from_str(&json).expect("deserialize place");

        assert_eq!(back, place);
    }

    #[test]
    fn hierarchy_type_defaults_to_admin() {
        let href = PlaceRef {
            place_id: EntityId::new(),
            hierarchy_type: HierarchyType::default(),
            date_range: None,
        };

        assert_eq!(href.hierarchy_type, HierarchyType::Admin);
    }
}
