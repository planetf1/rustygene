use std::collections::HashMap;

use rustygene_core::event::EventType;
use rustygene_core::person::{Person, PersonName, Surname, SurnameOrigin};
use rustygene_core::types::EntityId;
use rustygene_gedcom::diff::generate_person_import_diff;
use rustygene_gedcom::matching::match_persons;
use rustygene_gedcom::{
    build_gedcom_tree, map_indi_nodes_to_events, map_indi_nodes_to_persons, tokenize_gedcom,
};

#[test]
fn kennedy_modified_fixture_classifies_new_changed_and_unchanged() {
    let text = include_str!("../../../testdata/gedcom/kennedy.ged");
    let lines = tokenize_gedcom(text).expect("tokenize kennedy");
    let nodes = build_gedcom_tree(&lines).expect("build kennedy tree");

    let existing_persons = map_indi_nodes_to_persons(&nodes);
    let existing_events = map_indi_nodes_to_events(&nodes);

    let mut modified_persons = existing_persons.clone();
    let mut modified_events = existing_events.clone();

    let target_person_id = modified_persons[0].id;
    let birth_event = modified_events
        .iter_mut()
        .find(|event| {
            event.event_type == EventType::Birth
                && event
                    .participants
                    .iter()
                    .any(|participant| participant.person_id == target_person_id)
        })
        .expect("kennedy fixture has a birth event for first person");
    birth_event.date = Some(rustygene_core::types::DateValue::Textual {
        value: "1 JAN 2001".to_string(),
    });

    let new_person = Person {
        id: EntityId::new(),
        names: vec![PersonName {
            given_names: "MergeTest".to_string(),
            surnames: vec![Surname {
                value: "Kennedy".to_string(),
                origin_type: SurnameOrigin::Patrilineal,
                connector: None,
            }],
            ..Default::default()
        }],
        gender: rustygene_core::types::Gender::Unknown,
        living: false,
        private: false,
        original_xref: Some("@I9999@".to_string()),
        _raw_gedcom: std::collections::BTreeMap::new(),
    };
    modified_persons.push(new_person);

    let match_result = match_persons(
        &modified_persons,
        &modified_events,
        &existing_persons,
        &existing_events,
        &HashMap::new(),
    );

    let diff = generate_person_import_diff(
        &match_result,
        &modified_persons,
        &modified_events,
        &existing_persons,
        &existing_events,
        "kennedy-modified.ged",
    );

    assert_eq!(diff.new_entities.len(), 1);
    assert!(diff.updated_fields.iter().any(|entry| {
        entry.entity_id == target_person_id
            && entry.field == "birth.date"
            && entry.source == "kennedy-modified.ged"
    }));
    assert!(diff.unchanged > 0);
}
