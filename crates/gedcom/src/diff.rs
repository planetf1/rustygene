use std::collections::{BTreeMap, HashSet};

use rustygene_core::event::{Event, EventType};
use rustygene_core::person::Person;
use rustygene_core::types::EntityId;
use serde_json::Value;

use crate::matching::{GedcomRef, MatchResult};

#[derive(Debug, Clone, PartialEq)]
pub struct FieldDiff {
    pub entity_id: EntityId,
    pub field: String,
    pub old_value: Value,
    pub new_value: Value,
    pub source: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ImportDiff {
    pub new_entities: Vec<GedcomRef>,
    pub updated_fields: Vec<FieldDiff>,
    pub unchanged: usize,
}

#[must_use]
pub fn generate_person_import_diff(
    match_result: &MatchResult,
    gedcom_persons: &[Person],
    gedcom_events: &[Event],
    existing_persons: &[Person],
    existing_events: &[Event],
    source: &str,
) -> ImportDiff {
    let mut updated_fields = Vec::new();
    let mut unchanged = 0usize;

    for matched in &match_result.matched {
        let GedcomRef::Person { .. } = &matched.gedcom_ref else {
            continue;
        };

        let Some(gedcom_person) = find_person_for_ref(gedcom_persons, &matched.gedcom_ref) else {
            continue;
        };

        let Some(existing_person) = existing_persons
            .iter()
            .find(|person| person.id == matched.entity_id)
        else {
            continue;
        };

        let old_fields = person_field_map(existing_person, existing_events);
        let new_fields = person_field_map(gedcom_person, gedcom_events);

        let mut changed_for_entity = 0usize;

        let mut keys: HashSet<String> = old_fields.keys().cloned().collect();
        keys.extend(new_fields.keys().cloned());

        for key in keys {
            let old_value = old_fields.get(&key).cloned().unwrap_or(Value::Null);
            let new_value = new_fields.get(&key).cloned().unwrap_or(Value::Null);
            if old_value != new_value {
                updated_fields.push(FieldDiff {
                    entity_id: matched.entity_id,
                    field: key,
                    old_value,
                    new_value,
                    source: source.to_string(),
                });
                changed_for_entity += 1;
            }
        }

        if changed_for_entity == 0 {
            unchanged += 1;
        }
    }

    let new_entities = match_result
        .unmatched
        .iter()
        .filter_map(|entry| match entry {
            GedcomRef::Person { .. } => Some(entry.clone()),
            _ => None,
        })
        .collect();

    ImportDiff {
        new_entities,
        updated_fields,
        unchanged,
    }
}

fn find_person_for_ref<'a>(persons: &'a [Person], r#ref: &GedcomRef) -> Option<&'a Person> {
    let GedcomRef::Person { xref, label } = r#ref else {
        return None;
    };

    if let Some(xref) = xref
        && let Some(found) = persons
            .iter()
            .find(|person| person.original_xref.as_deref() == Some(xref.as_str()))
    {
        return Some(found);
    }

    persons.iter().find(|person| person_label(person) == *label)
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

fn person_field_map(person: &Person, events: &[Event]) -> BTreeMap<String, Value> {
    let mut fields = BTreeMap::new();

    let primary = person.primary_name();
    let primary_surname = primary
        .surnames
        .first()
        .map(|s| s.value.clone())
        .unwrap_or_default();

    fields.insert(
        "name.given".to_string(),
        Value::String(primary.given_names.clone()),
    );
    fields.insert("name.surname".to_string(), Value::String(primary_surname));
    fields.insert(
        "gender".to_string(),
        Value::String(format!("{:?}", person.gender)),
    );

    if let Some(birth_date) = person_event_date(person.id, events, &EventType::Birth) {
        fields.insert("birth.date".to_string(), date_value_to_json(birth_date));
    }

    if let Some(death_date) = person_event_date(person.id, events, &EventType::Death) {
        fields.insert("death.date".to_string(), date_value_to_json(death_date));
    }

    fields
}

fn person_event_date<'a>(
    person_id: EntityId,
    events: &'a [Event],
    event_type: &EventType,
) -> Option<&'a rustygene_core::types::DateValue> {
    events
        .iter()
        .find(|event| {
            &event.event_type == event_type
                && event
                    .participants
                    .iter()
                    .any(|participant| participant.person_id == person_id)
        })
        .and_then(|event| event.date.as_ref())
}

fn date_value_to_json(date: &rustygene_core::types::DateValue) -> Value {
    serde_json::to_value(date).unwrap_or(Value::Null)
}
