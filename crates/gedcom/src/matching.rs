use std::collections::{HashMap, HashSet};

use rustygene_core::event::{Event, EventType};
use rustygene_core::evidence::Source;
use rustygene_core::family::Family;
use rustygene_core::person::{Person, PersonName};
use rustygene_core::types::{DateValue, EntityId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchConfidence {
    Exact,
    High,
    Medium,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GedcomRef {
    Person { xref: Option<String>, label: String },
    Family { xref: Option<String>, label: String },
    Source { xref: Option<String>, label: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchedEntity {
    pub gedcom_ref: GedcomRef,
    pub entity_id: EntityId,
    pub confidence: MatchConfidence,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MatchResult {
    pub matched: Vec<MatchedEntity>,
    pub unmatched: Vec<GedcomRef>,
}

#[must_use]
pub fn match_persons(
    gedcom_persons: &[Person],
    gedcom_events: &[Event],
    existing_persons: &[Person],
    existing_events: &[Event],
    prior_xref_map: &HashMap<String, EntityId>,
) -> MatchResult {
    let mut matched = Vec::new();
    let mut unmatched = Vec::new();
    let mut used_existing_ids = HashSet::new();

    let existing_xref_map: HashMap<String, EntityId> = existing_persons
        .iter()
        .filter_map(|person| person.original_xref.clone().map(|xref| (xref, person.id)))
        .collect();

    for gedcom_person in gedcom_persons {
        let gedcom_ref = GedcomRef::Person {
            xref: gedcom_person.original_xref.clone(),
            label: person_label(gedcom_person),
        };

        if let Some(xref) = gedcom_person.original_xref.as_deref() {
            let exact = prior_xref_map
                .get(xref)
                .copied()
                .or_else(|| existing_xref_map.get(xref).copied());
            if let Some(entity_id) = exact {
                matched.push(MatchedEntity {
                    gedcom_ref,
                    entity_id,
                    confidence: MatchConfidence::Exact,
                });
                used_existing_ids.insert(entity_id);
                continue;
            }
        }

        let gedcom_birth = person_birth_date(gedcom_person.id, gedcom_events);
        let gedcom_birth_year = birth_year(gedcom_birth);
        let gedcom_name = normalized_primary_name(gedcom_person);

        let mut high_candidate: Option<EntityId> = None;
        let mut medium_candidate: Option<EntityId> = None;

        for existing_person in existing_persons {
            if used_existing_ids.contains(&existing_person.id) {
                continue;
            }

            let existing_name = normalized_primary_name(existing_person);
            if existing_name.is_empty() || gedcom_name.is_empty() {
                continue;
            }
            let existing_birth = person_birth_date(existing_person.id, existing_events);
            let existing_birth_year = birth_year(existing_birth);

            if names_equal(&gedcom_name, &existing_name)
                && dates_equal(gedcom_birth, existing_birth)
                && gedcom_birth.is_some()
            {
                high_candidate = Some(existing_person.id);
                break;
            }

            if fuzzy_name_match(&gedcom_name, &existing_name)
                && gedcom_birth_year.is_some()
                && gedcom_birth_year == existing_birth_year
            {
                medium_candidate = Some(existing_person.id);
            }
        }

        if let Some(entity_id) = high_candidate {
            matched.push(MatchedEntity {
                gedcom_ref,
                entity_id,
                confidence: MatchConfidence::High,
            });
            used_existing_ids.insert(entity_id);
        } else if let Some(entity_id) = medium_candidate {
            matched.push(MatchedEntity {
                gedcom_ref,
                entity_id,
                confidence: MatchConfidence::Medium,
            });
            used_existing_ids.insert(entity_id);
        } else {
            unmatched.push(gedcom_ref);
        }
    }

    MatchResult { matched, unmatched }
}

#[must_use]
pub fn match_families(
    gedcom_families: &[Family],
    existing_families: &[Family],
    person_match_map: &HashMap<EntityId, EntityId>,
) -> MatchResult {
    let mut matched = Vec::new();
    let mut unmatched = Vec::new();
    let mut used_existing_ids = HashSet::new();

    for gedcom_family in gedcom_families {
        let gedcom_ref = GedcomRef::Family {
            xref: gedcom_family.original_xref.clone(),
            label: family_label(gedcom_family),
        };

        let mapped_pair = canonical_pair(
            gedcom_family
                .partner1_id
                .and_then(|id| person_match_map.get(&id).copied()),
            gedcom_family
                .partner2_id
                .and_then(|id| person_match_map.get(&id).copied()),
        );

        let Some(mapped_pair) = mapped_pair else {
            unmatched.push(gedcom_ref);
            continue;
        };

        let existing_match = existing_families
            .iter()
            .filter(|family| !used_existing_ids.contains(&family.id))
            .find(|family| {
                canonical_pair(family.partner1_id, family.partner2_id) == Some(mapped_pair)
            })
            .map(|family| family.id);

        if let Some(entity_id) = existing_match {
            used_existing_ids.insert(entity_id);
            matched.push(MatchedEntity {
                gedcom_ref,
                entity_id,
                confidence: MatchConfidence::High,
            });
        } else {
            unmatched.push(gedcom_ref);
        }
    }

    MatchResult { matched, unmatched }
}

#[must_use]
pub fn match_sources(gedcom_sources: &[Source], existing_sources: &[Source]) -> MatchResult {
    let mut matched = Vec::new();
    let mut unmatched = Vec::new();
    let mut used_existing_ids = HashSet::new();

    for gedcom_source in gedcom_sources {
        let gedcom_ref = GedcomRef::Source {
            xref: gedcom_source.original_xref.clone(),
            label: gedcom_source.title.clone(),
        };

        let target_title = normalize_text(&gedcom_source.title);
        let target_author = gedcom_source.author.as_deref().map(normalize_text);

        let existing_match = existing_sources
            .iter()
            .filter(|source| !used_existing_ids.contains(&source.id))
            .find(|source| {
                normalize_text(&source.title) == target_title
                    && source.author.as_deref().map(normalize_text) == target_author
            })
            .map(|source| source.id);

        if let Some(entity_id) = existing_match {
            used_existing_ids.insert(entity_id);
            matched.push(MatchedEntity {
                gedcom_ref,
                entity_id,
                confidence: MatchConfidence::High,
            });
        } else {
            unmatched.push(gedcom_ref);
        }
    }

    MatchResult { matched, unmatched }
}

fn person_label(person: &Person) -> String {
    let primary = person.primary_name();
    let given = primary.given_names.trim();
    let surname = primary
        .surnames
        .first()
        .map(|s| s.value.trim())
        .unwrap_or_default();

    if given.is_empty() && surname.is_empty() {
        "unknown-person".to_string()
    } else if surname.is_empty() {
        given.to_string()
    } else if given.is_empty() {
        surname.to_string()
    } else {
        format!("{given} {surname}")
    }
}

fn family_label(family: &Family) -> String {
    let p1 = family
        .partner1_id
        .map(|id| id.to_string())
        .unwrap_or_else(|| "none".to_string());
    let p2 = family
        .partner2_id
        .map(|id| id.to_string())
        .unwrap_or_else(|| "none".to_string());
    format!("{p1}:{p2}")
}

fn normalize_text(value: &str) -> String {
    value
        .to_ascii_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn normalized_primary_name(person: &Person) -> String {
    normalize_name(&person.primary_name())
}

fn normalize_name(name: &PersonName) -> String {
    let surname = name
        .surnames
        .first()
        .map(|s| s.value.as_str())
        .unwrap_or_default();
    normalize_text(&format!("{} {}", name.given_names, surname))
}

fn names_equal(left: &str, right: &str) -> bool {
    left == right
}

fn fuzzy_name_match(left: &str, right: &str) -> bool {
    if left == right {
        return true;
    }

    let left_tokens: Vec<&str> = left.split_whitespace().collect();
    let right_tokens: Vec<&str> = right.split_whitespace().collect();
    let (Some(left_given), Some(left_surname), Some(right_given), Some(right_surname)) = (
        left_tokens.first(),
        left_tokens.last(),
        right_tokens.first(),
        right_tokens.last(),
    ) else {
        return false;
    };

    if left_surname != right_surname {
        return false;
    }

    left_given.chars().next() == right_given.chars().next()
}

fn person_birth_date(person_id: EntityId, events: &[Event]) -> Option<&DateValue> {
    events
        .iter()
        .find(|event| {
            event.event_type == EventType::Birth
                && event
                    .participants
                    .iter()
                    .any(|participant| participant.person_id == person_id)
        })
        .and_then(|event| event.date.as_ref())
}

fn dates_equal(left: Option<&DateValue>, right: Option<&DateValue>) -> bool {
    left.is_some() && right.is_some() && left == right
}

fn birth_year(date: Option<&DateValue>) -> Option<i32> {
    match date {
        Some(DateValue::Exact { date, .. })
        | Some(DateValue::Before { date, .. })
        | Some(DateValue::After { date, .. })
        | Some(DateValue::About { date, .. })
        | Some(DateValue::Tolerance { date, .. }) => Some(date.year),
        Some(DateValue::Range { from, .. }) => Some(from.year),
        Some(DateValue::Quarter { year, .. }) => Some(*year),
        Some(DateValue::Textual { value }) => first_four_digit_year(value),
        None => None,
    }
}

fn first_four_digit_year(value: &str) -> Option<i32> {
    let mut digits = String::new();
    for ch in value.chars() {
        if ch.is_ascii_digit() {
            digits.push(ch);
            if digits.len() == 4 {
                break;
            }
        } else {
            digits.clear();
        }
    }

    if digits.len() == 4 {
        digits.parse().ok()
    } else {
        None
    }
}

fn canonical_pair(left: Option<EntityId>, right: Option<EntityId>) -> Option<(EntityId, EntityId)> {
    match (left, right) {
        (Some(a), Some(b)) if a <= b => Some((a, b)),
        (Some(a), Some(b)) => Some((b, a)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use rustygene_core::types::{DateValue, EntityId};
    use std::collections::HashMap;

    use crate::{
        build_gedcom_tree, map_indi_nodes_to_events, map_indi_nodes_to_persons, tokenize_gedcom,
    };

    use super::{MatchConfidence, match_persons};

    #[test]
    fn simpsons_fixture_matches_with_high_or_exact_confidence() {
        let text = include_str!("../../../testdata/gedcom/simpsons.ged");
        let lines = tokenize_gedcom(text).expect("tokenize simpsons");
        let nodes = build_gedcom_tree(&lines).expect("build simpsons tree");

        let gedcom_persons = map_indi_nodes_to_persons(&nodes);
        let gedcom_events = map_indi_nodes_to_events(&nodes);

        let mut prior_xref_map = HashMap::new();
        for person in &gedcom_persons {
            if let Some(xref) = &person.original_xref {
                prior_xref_map.insert(xref.clone(), person.id);
            }
        }

        let result = match_persons(
            &gedcom_persons,
            &gedcom_events,
            &gedcom_persons,
            &gedcom_events,
            &prior_xref_map,
        );

        assert_eq!(result.matched.len(), gedcom_persons.len());
        assert!(result.unmatched.is_empty());
        assert!(result.matched.iter().all(|entry| matches!(
            entry.confidence,
            MatchConfidence::Exact | MatchConfidence::High
        )));
    }

    #[test]
    fn fuzzy_name_and_birth_year_yields_medium_confidence() {
        let gedcom_person = rustygene_core::person::Person {
            id: EntityId::new(),
            names: vec![rustygene_core::person::PersonName {
                given_names: "Homerx".to_string(),
                surnames: vec![rustygene_core::person::Surname {
                    value: "Simpson".to_string(),
                    origin_type: rustygene_core::person::SurnameOrigin::Patrilineal,
                    connector: None,
                }],
                ..Default::default()
            }],
            gender: rustygene_core::types::Gender::Male,
            living: false,
            private: false,
            original_xref: None,
            _raw_gedcom: std::collections::BTreeMap::new(),
        };

        let existing_person = rustygene_core::person::Person {
            id: EntityId::new(),
            names: vec![rustygene_core::person::PersonName {
                given_names: "Homer".to_string(),
                surnames: vec![rustygene_core::person::Surname {
                    value: "Simpson".to_string(),
                    origin_type: rustygene_core::person::SurnameOrigin::Patrilineal,
                    connector: None,
                }],
                ..Default::default()
            }],
            gender: rustygene_core::types::Gender::Male,
            living: false,
            private: false,
            original_xref: None,
            _raw_gedcom: std::collections::BTreeMap::new(),
        };

        let gedcom_events = vec![rustygene_core::event::Event {
            id: EntityId::new(),
            event_type: rustygene_core::event::EventType::Birth,
            date: Some(DateValue::Textual {
                value: "12 MAY 1956".to_string(),
            }),
            place_ref: None,
            participants: vec![rustygene_core::event::EventParticipant {
                person_id: gedcom_person.id,
                role: rustygene_core::event::EventRole::Principal,
                census_role: None,
            }],
            description: None,
            _raw_gedcom: std::collections::BTreeMap::new(),
        }];

        let existing_events = vec![rustygene_core::event::Event {
            id: EntityId::new(),
            event_type: rustygene_core::event::EventType::Birth,
            date: Some(DateValue::Textual {
                value: "MAY 1956".to_string(),
            }),
            place_ref: None,
            participants: vec![rustygene_core::event::EventParticipant {
                person_id: existing_person.id,
                role: rustygene_core::event::EventRole::Principal,
                census_role: None,
            }],
            description: None,
            _raw_gedcom: std::collections::BTreeMap::new(),
        }];

        let result = match_persons(
            &[gedcom_person],
            &gedcom_events,
            &[existing_person],
            &existing_events,
            &HashMap::new(),
        );

        assert_eq!(result.matched.len(), 1);
        assert!(result.unmatched.is_empty());
        assert_eq!(result.matched[0].confidence, MatchConfidence::Medium);
    }
}
