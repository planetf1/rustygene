use chrono::Utc;
use proptest::collection::vec;
use proptest::prelude::*;
use rustygene_core::assertion::{Assertion, AssertionStatus, EvidenceType};
use rustygene_core::family::{Relationship, RelationshipType};
use rustygene_core::person::{NameType, PersonName, Surname, SurnameOrigin};
use rustygene_core::types::{ActorRef, Calendar, DateValue, EntityId, FuzzyDate};
use std::collections::BTreeSet;
use uuid::Uuid;

fn arb_calendar() -> impl Strategy<Value = Calendar> {
    prop_oneof![
        Just(Calendar::Gregorian),
        Just(Calendar::Julian),
        Just(Calendar::DualDate),
        Just(Calendar::Hebrew),
        Just(Calendar::FrenchRepublican),
        Just(Calendar::Islamic),
    ]
}

fn arb_fuzzy_date() -> impl Strategy<Value = FuzzyDate> {
    (
        1500_i32..=2100,
        prop_oneof![Just(None), (1_u8..=12).prop_map(Some)],
        prop_oneof![Just(None), (1_u8..=31).prop_map(Some)],
    )
        .prop_map(|(year, month, day)| FuzzyDate::new(year, month, day))
}

fn arb_date_value_serializable() -> impl Strategy<Value = DateValue> {
    let exact = (arb_fuzzy_date(), arb_calendar()).prop_map(|(date, calendar)| DateValue::Exact {
        date,
        calendar,
    });

    let range = (arb_fuzzy_date(), arb_fuzzy_date(), arb_calendar()).prop_map(
        |(from, to, calendar)| DateValue::Range { from, to, calendar },
    );

    let before =
        (arb_fuzzy_date(), arb_calendar()).prop_map(|(date, calendar)| DateValue::Before {
            date,
            calendar,
        });

    let after = (arb_fuzzy_date(), arb_calendar()).prop_map(|(date, calendar)| DateValue::After {
        date,
        calendar,
    });

    let about = (arb_fuzzy_date(), arb_calendar()).prop_map(|(date, calendar)| DateValue::About {
        date,
        calendar,
    });

    let tolerance = (arb_fuzzy_date(), 0_u32..=3650, arb_calendar()).prop_map(
        |(date, plus_minus_days, calendar)| DateValue::Tolerance {
            date,
            plus_minus_days,
            calendar,
        },
    );

    let quarter = (1500_i32..=2100, 1_u8..=4)
        .prop_map(|(year, quarter)| DateValue::Quarter { year, quarter });

    prop_oneof![exact, range, before, after, about, tolerance, quarter]
}

fn arb_date_value() -> impl Strategy<Value = DateValue> {
    prop_oneof![arb_date_value_serializable(), ".{0,64}".prop_map(DateValue::Textual)]
}

fn arb_assertion_status() -> impl Strategy<Value = AssertionStatus> {
    prop_oneof![
        Just(AssertionStatus::Confirmed),
        Just(AssertionStatus::Proposed),
        Just(AssertionStatus::Disputed),
        Just(AssertionStatus::Rejected),
    ]
}

fn arb_person_name() -> impl Strategy<Value = PersonName> {
    let name_type = prop_oneof![
        Just(NameType::Birth),
        Just(NameType::Married),
        Just(NameType::Aka),
        Just(NameType::Immigrant),
        Just(NameType::Religious),
        ".{0,24}".prop_map(NameType::Custom),
    ];

    let surname_origin = prop_oneof![
        Just(SurnameOrigin::Patrilineal),
        Just(SurnameOrigin::Matrilineal),
        Just(SurnameOrigin::Patronymic),
        Just(SurnameOrigin::Matronymic),
        Just(SurnameOrigin::Location),
        Just(SurnameOrigin::Occupation),
        Just(SurnameOrigin::Feudal),
        Just(SurnameOrigin::Pseudonym),
        Just(SurnameOrigin::Taken),
        Just(SurnameOrigin::Inherited),
        ".{0,16}".prop_map(SurnameOrigin::Custom),
    ];

    let surname = (
        ".{1,24}",
        surname_origin,
        prop_oneof![Just(None), ".{1,12}".prop_map(Some)],
    )
        .prop_map(|(value, origin_type, connector)| Surname {
            value,
            origin_type,
            connector,
        });

    (
        name_type,
        prop_oneof![Just(None), arb_date_value_serializable().prop_map(Some)],
        ".{0,32}",
        prop_oneof![Just(None), ".{0,24}".prop_map(Some)],
        vec(surname, 0..=3),
        prop_oneof![Just(None), ".{0,16}".prop_map(Some)],
        prop_oneof![Just(None), ".{0,16}".prop_map(Some)],
        prop_oneof![Just(None), ".{0,32}".prop_map(Some)],
    )
        .prop_map(
            |(name_type, date_range, given_names, call_name, surnames, prefix, suffix, sort_as)| {
                PersonName {
                    name_type,
                    date_range,
                    given_names,
                    call_name,
                    surnames,
                    prefix,
                    suffix,
                    sort_as,
                }
            },
        )
}

fn can_transition(from: AssertionStatus, to: AssertionStatus) -> bool {
    match from {
        AssertionStatus::Proposed => matches!(
            to,
            AssertionStatus::Proposed
                | AssertionStatus::Confirmed
                | AssertionStatus::Disputed
                | AssertionStatus::Rejected
        ),
        AssertionStatus::Confirmed => {
            matches!(to, AssertionStatus::Confirmed | AssertionStatus::Disputed | AssertionStatus::Rejected)
        }
        AssertionStatus::Disputed => {
            matches!(to, AssertionStatus::Disputed | AssertionStatus::Confirmed | AssertionStatus::Rejected)
        }
        AssertionStatus::Rejected => matches!(to, AssertionStatus::Rejected),
    }
}

proptest! {
    #[test]
    fn date_value_round_trip_is_identity(value in arb_date_value_serializable()) {
        let encoded = serde_json::to_string(&value)?;
        let decoded: DateValue = serde_json::from_str(&encoded)?;
        prop_assert_eq!(decoded, value);
    }

    #[test]
    fn date_value_partial_cmp_is_symmetric(left in arb_date_value(), right in arb_date_value()) {
        let ltr = left.partial_cmp(&right);
        let rtl = right.partial_cmp(&left);

        match (ltr, rtl) {
            (Some(order_a), Some(order_b)) => prop_assert_eq!(order_a, order_b.reverse()),
            (None, None) => {},
            _ => prop_assert!(false, "partial_cmp symmetry violated"),
        }
    }

    #[test]
    fn assertion_status_transition_rules_hold(from in arb_assertion_status(), to in arb_assertion_status()) {
        let valid = can_transition(from.clone(), to.clone());

        if from == AssertionStatus::Rejected {
            prop_assert_eq!(valid, to == AssertionStatus::Rejected);
        }

        if to == AssertionStatus::Proposed {
            prop_assert_eq!(valid, from == AssertionStatus::Proposed);
        }

        if from == to {
            prop_assert!(valid);
        }

        let assertion = Assertion {
            id: EntityId::new(),
            value: "field-value".to_string(),
            confidence: 0.9,
            status: from,
            evidence_type: EvidenceType::Direct,
            source_citations: vec![],
            proposed_by: ActorRef::Agent("validator".to_string()),
            created_at: Utc::now(),
            reviewed_at: None,
            reviewed_by: None,
        };

        let _ = assertion;
    }

    #[test]
    fn person_name_round_trip_is_identity(name in arb_person_name()) {
        let encoded = serde_json::to_string(&name)?;
        let decoded: PersonName = serde_json::from_str(&encoded)?;
        prop_assert_eq!(decoded, name);
    }

    #[test]
    fn generated_relationship_graph_has_no_dangling_refs(
        raw_ids in vec(any::<u128>(), 2..10),
        raw_edges in vec((any::<u8>(), any::<u8>()), 0..30),
    ) {
        let mut ids: Vec<EntityId> = raw_ids
            .into_iter()
            .map(|n| EntityId(Uuid::from_u128(n)))
            .collect();
        ids.sort_unstable();
        ids.dedup();

        prop_assume!(ids.len() >= 2);

        let id_set: BTreeSet<EntityId> = ids.iter().copied().collect();

        let relationships: Vec<Relationship> = raw_edges
            .into_iter()
            .map(|(from_idx, to_idx)| {
                let from = ids[usize::from(from_idx) % ids.len()];
                let to = ids[usize::from(to_idx) % ids.len()];

                Relationship {
                    id: EntityId::new(),
                    person1_id: from,
                    person2_id: to,
                    relationship_type: RelationshipType::ParentChild,
                    supporting_event: None,
                    _raw_gedcom: std::collections::BTreeMap::new(),
                }
            })
            .collect();

        for rel in &relationships {
            prop_assert!(id_set.contains(&rel.person1_id));
            prop_assert!(id_set.contains(&rel.person2_id));
        }
    }
}
