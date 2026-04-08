use rustygene_core::event::EventType;
use rustygene_gedcom::{
    build_gedcom_tree, generate_import_assertions, map_family_nodes, map_indi_nodes_to_events,
    map_indi_nodes_to_persons, map_media_note_lds, map_source_chain, tokenize_gedcom,
};
use rustygene_storage::EntityType;

#[test]
fn inline_note_links_and_text_generate_typed_note_assertions_for_person_and_event() {
    let input = r#"0 HEAD
1 SOUR TEST
1 GEDC
2 VERS 5.5.1
1 CHAR UTF-8
0 @S1@ SOUR
1 TITL Parish register
0 @N1@ NOTE Root note text
0 @I1@ INDI
1 NAME John /Doe/
1 SEX M
1 NOTE @N1@
1 NOTE Inline person note
2 SOUR @S1@
1 BIRT
2 DATE 1 JAN 1900
2 NOTE @N1@
2 NOTE Inline birth note
3 SOUR @S1@
0 TRLR
"#;

    let lines = tokenize_gedcom(input).expect("tokenize GEDCOM");
    let roots = build_gedcom_tree(&lines).expect("build GEDCOM tree");

    let persons = map_indi_nodes_to_persons(&roots);
    let family_mapping = map_family_nodes(&roots);
    let source_mapping = map_source_chain(&roots);
    let media_note_lds_mapping = map_media_note_lds(&roots);
    let person_events = map_indi_nodes_to_events(&roots);

    let assertions = generate_import_assertions(
        "test-note-ref-job",
        &persons,
        &[],
        &family_mapping,
        &source_mapping,
        &media_note_lds_mapping,
        &person_events,
    )
    .expect("generate import assertions");

    let person_id = persons[0].id;
    let birth_event_id = person_events
        .iter()
        .find(|event| matches!(event.event_type, EventType::Birth))
        .expect("birth event")
        .id;

    let person_note_ref_count = assertions
        .iter()
        .filter(|record| {
            record.entity_type == EntityType::Person
                && record.entity_id == person_id
                && record.field == "note_ref"
        })
        .count();
    let person_note_text_count = assertions
        .iter()
        .filter(|record| {
            record.entity_type == EntityType::Person
                && record.entity_id == person_id
                && record.field == "note"
        })
        .count();

    assert!(
        person_note_ref_count >= 1,
        "expected typed note_ref assertion for INDI NOTE @N...@"
    );
    assert!(
        person_note_text_count >= 1,
        "expected typed note assertion for inline INDI NOTE text"
    );

    let event_note_ref_count = assertions
        .iter()
        .filter(|record| {
            record.entity_type == EntityType::Event
                && record.entity_id == birth_event_id
                && record.field == "note_ref"
        })
        .count();
    let event_note_text_count = assertions
        .iter()
        .filter(|record| {
            record.entity_type == EntityType::Event
                && record.entity_id == birth_event_id
                && record.field == "note"
        })
        .count();

    assert!(
        event_note_ref_count >= 1,
        "expected typed note_ref assertion for event NOTE @N...@"
    );
    assert!(
        event_note_text_count >= 1,
        "expected typed note assertion for event inline NOTE text"
    );

    assert!(
        source_mapping
            .node_citation_refs
            .iter()
            .any(|reference| reference.root_tag == "INDI" && reference.owner_tag == "NOTE"),
        "expected SOUR under NOTE to be captured as NOTE-scoped citation reference"
    );
}
