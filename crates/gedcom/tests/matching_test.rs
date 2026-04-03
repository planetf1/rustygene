use std::collections::HashMap;

use rustygene_gedcom::matching::{MatchConfidence, match_persons};
use rustygene_gedcom::{
    build_gedcom_tree, map_indi_nodes_to_events, map_indi_nodes_to_persons, tokenize_gedcom,
};

#[test]
fn kennedy_fixture_xref_map_produces_exact_matches() {
    let text = include_str!("../../../testdata/gedcom/kennedy.ged");
    let lines = tokenize_gedcom(text).expect("tokenize kennedy");
    let nodes = build_gedcom_tree(&lines).expect("build kennedy tree");

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
    assert!(
        result
            .matched
            .iter()
            .all(|entry| entry.confidence == MatchConfidence::Exact)
    );
}
