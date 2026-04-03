use rustygene_gedcom::{
    build_gedcom_tree, generate_import_assertions, map_family_nodes, map_indi_nodes_to_events,
    map_indi_nodes_to_persons, map_media_note_lds, map_source_chain, tokenize_gedcom,
};
use rustygene_storage::EntityType;

#[test]
fn indi_and_fam_obje_links_generate_media_refs() {
    let input = r#"0 HEAD
1 SOUR TEST
1 GEDC
2 VERS 5.5.1
1 CHAR UTF-8
0 @M1@ OBJE
1 FILE /tmp/image-1.jpg
1 FORM jpg
1 TITL Root Media
0 @I1@ INDI
1 NAME John /Doe/
1 SEX M
1 OBJE @M1@
1 OBJE
2 FILE /tmp/inline-photo.jpg
2 TITL Inline Photo
0 @F1@ FAM
1 HUSB @I1@
1 OBJE @M1@
0 TRLR
"#;
    let lines = tokenize_gedcom(input).expect("tokenize ancestry GEDCOM");
    let roots = build_gedcom_tree(&lines).expect("build GEDCOM tree");

    let persons = map_indi_nodes_to_persons(&roots);
    let family_mapping = map_family_nodes(&roots);
    let source_mapping = map_source_chain(&roots);
    let media_note_lds_mapping = map_media_note_lds(&roots);
    let person_events = map_indi_nodes_to_events(&roots);

    let assertions = generate_import_assertions(
        "test-job",
        &persons,
        &[],
        &family_mapping,
        &source_mapping,
        &media_note_lds_mapping,
        &person_events,
    )
    .expect("generate import assertions");

    assert_eq!(media_note_lds_mapping.media.len(), 1);

    let media_ref_count = assertions
        .iter()
        .filter(|record| {
            record.field == "media_ref"
                && matches!(record.entity_type, EntityType::Person | EntityType::Family)
        })
        .count();

    assert!(media_ref_count >= 2);

    let inline_path_assertions = assertions
        .iter()
        .filter(|record| record.field == "media_ref")
        .filter(|record| {
            record
                .assertion
                .value
                .get("external_path")
                .and_then(serde_json::Value::as_str)
                .is_some()
        })
        .count();

    assert!(
        inline_path_assertions >= 1,
        "expected inline OBJE FILE path to be preserved as external media_ref"
    );
}
