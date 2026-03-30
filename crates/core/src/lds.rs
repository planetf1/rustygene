use crate::types::{DateValue, EntityId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LdsOrdinanceType {
    Baptism,
    Confirmation,
    Initiatory,
    Endowment,
    SealingToParents,
    SealingToSpouse,
    Custom(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LdsStatus {
    Bic,
    Canceled,
    Child,
    Completed,
    Cleared,
    Dns,
    Excluded,
    Infant,
    Invalid,
    NotNeeded,
    Qualified,
    Stillborn,
    Submitted,
    Uncleared,
    InProgress,
    NeedsMoreInformation,
    Ready,
    Reserved,
    Printed,
    Shared,
    TempleDone,
    Custom(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LdsOrdinance {
    pub id: EntityId,
    pub ordinance_type: LdsOrdinanceType,
    pub status: LdsStatus,
    pub temple_code: Option<String>,
    pub date: Option<DateValue>,
    pub place_ref: Option<EntityId>,
    /// Set for family-level ordinances such as spouse sealings.
    pub family_ref: Option<EntityId>,
    #[serde(default)]
    pub _raw_gedcom: BTreeMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Calendar, FuzzyDate};

    #[test]
    fn lds_status_has_expected_variant_coverage() {
        let statuses = [
            LdsStatus::Bic,
            LdsStatus::Canceled,
            LdsStatus::Child,
            LdsStatus::Completed,
            LdsStatus::Cleared,
            LdsStatus::Dns,
            LdsStatus::Excluded,
            LdsStatus::Infant,
            LdsStatus::Invalid,
            LdsStatus::NotNeeded,
            LdsStatus::Qualified,
            LdsStatus::Stillborn,
            LdsStatus::Submitted,
            LdsStatus::Uncleared,
            LdsStatus::InProgress,
            LdsStatus::NeedsMoreInformation,
            LdsStatus::Ready,
            LdsStatus::Reserved,
            LdsStatus::Printed,
            LdsStatus::Shared,
            LdsStatus::TempleDone,
        ];

        assert!(statuses.len() >= 20);
    }

    #[test]
    fn serde_round_trip_lds_ordinance() {
        let ordinance = LdsOrdinance {
            id: EntityId::new(),
            ordinance_type: LdsOrdinanceType::SealingToSpouse,
            status: LdsStatus::Completed,
            temple_code: Some("LON".to_string()),
            date: Some(DateValue::Exact {
                date: FuzzyDate::new(1988, Some(6), Some(14)),
                calendar: Calendar::Gregorian,
            }),
            place_ref: Some(EntityId::new()),
            family_ref: Some(EntityId::new()),
            _raw_gedcom: BTreeMap::new(),
        };

        let json = serde_json::to_string(&ordinance).expect("serialize ordinance");
        let back: LdsOrdinance = serde_json::from_str(&json).expect("deserialize ordinance");

        assert_eq!(back, ordinance);
    }
}
