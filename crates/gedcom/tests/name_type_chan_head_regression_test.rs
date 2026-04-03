use rustygene_core::person::NameType;
use rustygene_gedcom::{
    build_gedcom_tree, map_indi_nodes_to_persons, person_to_indi_node, render_gedcom_file,
    tokenize_gedcom,
};

#[test]
fn name_type_and_chan_survive_import_export_reimport_and_head_has_required_fields() {
    let input = concat!(
        "0 HEAD\n",
        "1 SOUR TEST\n",
        "1 GEDC\n",
        "2 VERS 5.5.1\n",
        "1 CHAR UTF-8\n",
        "0 @I1@ INDI\n",
        "1 NAME John /Doe/\n",
        "2 TYPE AKA\n",
        "1 CHAN\n",
        "2 DATE 1 APR 2026\n",
        "3 TIME 20:15:01\n",
        "0 TRLR\n",
    );

    let lines = tokenize_gedcom(input).expect("tokenize input");
    let roots = build_gedcom_tree(&lines).expect("build tree");
    let persons = map_indi_nodes_to_persons(&roots);
    let person = persons.first().expect("person parsed");
    assert_eq!(
        person.names.first().map(|n| &n.name_type),
        Some(&NameType::Aka)
    );

    let exported = render_gedcom_file(&[person_to_indi_node(person, &[], &[], "@I1@")]);

    assert!(exported.contains("2 TYPE AKA\n"));
    assert!(exported.contains("1 CHAN\n2 DATE 1 APR 2026\n3 TIME 20:15:01\n"));

    assert!(exported.contains("1 SOUR RUSTYGENE\n"));
    assert!(exported.contains("2 VERS 5.5.1\n"));
    assert!(exported.contains("2 FORM LINEAGE-LINKED\n"));
    assert!(exported.contains("1 CHAR UTF-8\n"));
    assert!(exported.contains("1 LANG ENG\n"));

    let exported_lines = tokenize_gedcom(&exported).expect("tokenize exported");
    let exported_roots = build_gedcom_tree(&exported_lines).expect("build exported tree");
    let roundtrip_persons = map_indi_nodes_to_persons(&exported_roots);
    let roundtrip_person = roundtrip_persons.first().expect("roundtrip person");

    assert_eq!(
        roundtrip_person.names.first().map(|n| &n.name_type),
        Some(&NameType::Aka)
    );
}
