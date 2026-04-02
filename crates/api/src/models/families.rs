use rustygene_core::event::Event;
use rustygene_core::family::{Family, PartnerLink};
use rustygene_core::person::Person;
use rustygene_core::types::EntityId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateFamilyRequest {
    pub partner1_id: Option<EntityId>,
    pub partner2_id: Option<EntityId>,
    #[serde(default)]
    pub partner_link: Option<PartnerLink>,
    #[serde(default)]
    pub child_ids: Vec<EntityId>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FamilyDetailResponse {
    pub id: EntityId,
    pub partner1: Option<PartnerSummary>,
    pub partner2: Option<PartnerSummary>,
    pub partner_link: PartnerLink,
    pub children: Vec<ChildSummary>,
    pub events: Vec<EventSummary>,
    pub assertion_counts: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PartnerSummary {
    pub id: EntityId,
    pub display_name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChildSummary {
    pub id: EntityId,
    pub display_name: String,
    pub lineage_type: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventSummary {
    pub id: EntityId,
    pub event_type: String,
    pub date: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FamilySummaryForPerson {
    pub id: EntityId,
    pub partner1: Option<PartnerSummary>,
    pub partner2: Option<PartnerSummary>,
    pub your_role: String,
}

impl FamilyDetailResponse {
    pub fn from_family_persons_events(
        family: Family,
        partner1: Option<Person>,
        partner2: Option<Person>,
        events: Vec<Event>,
        assertions: Vec<super::super::models::persons::AssertionValueResponse>,
    ) -> Self {
        let assertion_counts = assertions
            .iter()
            .fold(BTreeMap::new(), |mut acc, asrt| {
                *acc.entry(asrt.field.clone()).or_insert(0) += 1;
                acc
            });

        Self {
            id: family.id,
            partner1: partner1.as_ref().map(|p| PartnerSummary {
                id: p.id,
                display_name: display_name_for_person(p),
            }),
            partner2: partner2.as_ref().map(|p| PartnerSummary {
                id: p.id,
                display_name: display_name_for_person(p),
            }),
            partner_link: family.partner_link.clone(),
            children: family
                .child_links
                .iter()
                .map(|child| {
                    vec![partner1.as_ref(), partner2.as_ref()]
                        .into_iter()
                        .find_map(|p| {
                            p.as_ref().and_then(|person| {
                                if person.id == child.child_id {
                                    Some(ChildSummary {
                                        id: child.child_id,
                                        display_name: display_name_for_person(person),
                                        lineage_type: format!("{:?}", child.lineage_type),
                                    })
                                } else {
                                    None
                                }
                            })
                        })
                        .unwrap_or_else(|| ChildSummary {
                            id: child.child_id,
                            display_name: format!("Person {}", child.child_id),
                            lineage_type: format!("{:?}", child.lineage_type),
                        })
                })
                .collect(),
            events: events
                .into_iter()
                .map(|e| EventSummary {
                    id: e.id,
                    event_type: format!("{:?}", e.event_type),
                    date: e.date.as_ref().map(|d| format!("{:?}", d)),
                })
                .collect(),
            assertion_counts,
        }
    }
}

fn display_name_for_person(person: &Person) -> String {
    person
        .names
        .first()
        .map(|n| {
            let given = n.given_names.trim();
            let surnames = n
                .surnames
                .iter()
                .map(|s| s.value.as_str())
                .collect::<Vec<_>>()
                .join(" ");

            if surnames.is_empty() {
                given.to_string()
            } else {
                format!("{} {}", given, surnames)
            }
        })
        .unwrap_or_else(|| format!("Person {}", person.id))
}
