use std::collections::{BTreeMap, HashMap};

use chrono::Utc;
use rustygene_core::assertion::{AssertionStatus, EvidenceType};
use rustygene_core::event::{Event, EventParticipant, EventRole, EventType};
use rustygene_core::evidence::{
    Citation, Media, Note, NoteType, Repository, RepositoryType, Source,
};
use rustygene_core::family::{
    ChildLink, Family, LineageType, PartnerLink, Relationship, RelationshipType,
};
use rustygene_core::person::{NameType, Person, PersonName, Surname, SurnameOrigin};
use rustygene_core::place::{Place, PlaceName, PlaceType};
use rustygene_core::types::{ActorRef, Calendar, DateValue, EntityId, FuzzyDate, Gender};
use rustygene_storage::{EntityType, JsonAssertion, Storage, sqlite_impl::SqliteBackend};
use uuid::Uuid;
use xmltree::{Element, XMLNode};

const GRAMPS_ENTITY_NAMESPACE: Uuid = Uuid::from_u128(0x2b8d1226_a3bf_4c0b_a98d_7a59d0132adf);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrampsImportReport {
    pub entities_created_by_type: BTreeMap<String, usize>,
    pub assertions_created: usize,
}

#[derive(Debug, Default)]
struct GrampsImportData {
    persons: Vec<Person>,
    families: Vec<Family>,
    relationships: Vec<Relationship>,
    events: Vec<Event>,
    places: Vec<Place>,
    sources: Vec<Source>,
    citations: Vec<Citation>,
    repositories: Vec<Repository>,
    notes: Vec<Note>,
    media: Vec<Media>,
}

fn entity_id_from_seed(entity_kind: &str, seed: &str) -> EntityId {
    EntityId(Uuid::new_v5(
        &GRAMPS_ENTITY_NAMESPACE,
        format!("{entity_kind}:{seed}").as_bytes(),
    ))
}

fn child_elements_named<'a>(
    element: &'a Element,
    name: &'a str,
) -> impl Iterator<Item = &'a Element> {
    element.children.iter().filter_map(move |node| match node {
        XMLNode::Element(child) if child.name == name => Some(child),
        _ => None,
    })
}

fn first_child_named<'a>(element: &'a Element, name: &'a str) -> Option<&'a Element> {
    child_elements_named(element, name).next()
}

fn element_text(element: &Element) -> Option<String> {
    let text = element
        .children
        .iter()
        .filter_map(|node| match node {
            XMLNode::Text(t) => Some(t.as_str()),
            _ => None,
        })
        .collect::<String>();
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn child_text(element: &Element, name: &str) -> Option<String> {
    first_child_named(element, name).and_then(element_text)
}

fn parse_gender(value: Option<&str>) -> Gender {
    match value
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "m" | "male" => Gender::Male,
        "f" | "female" => Gender::Female,
        "u" | "unknown" | "" => Gender::Unknown,
        custom => Gender::Custom(custom.to_string()),
    }
}

fn parse_date_value(raw: &str) -> Option<DateValue> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    let parts: Vec<&str> = trimmed.split('-').collect();
    if parts.is_empty() {
        return None;
    }

    let year = parts[0].parse::<i32>().ok()?;
    let month = parts.get(1).and_then(|m| m.parse::<u8>().ok());
    let day = parts.get(2).and_then(|d| d.parse::<u8>().ok());

    Some(DateValue::Exact {
        date: FuzzyDate::new(year, month, day),
        calendar: Calendar::Gregorian,
    })
}

fn parse_event_type(raw: Option<&str>) -> EventType {
    match raw.unwrap_or_default().trim().to_ascii_lowercase().as_str() {
        "birth" => EventType::Birth,
        "death" => EventType::Death,
        "marriage" => EventType::Marriage,
        "baptism" => EventType::Baptism,
        "burial" => EventType::Burial,
        "census" => EventType::Census,
        "residence" => EventType::Residence,
        "occupation" => EventType::Occupation,
        "immigration" => EventType::Immigration,
        "emigration" => EventType::Emigration,
        "naturalization" => EventType::Naturalization,
        "probate" => EventType::Probate,
        "will" => EventType::Will,
        "migration" => EventType::Migration,
        "graduation" => EventType::Graduation,
        "retirement" => EventType::Retirement,
        other => EventType::Custom(other.to_string()),
    }
}

fn parse_database(root: &Element) -> GrampsImportData {
    let mut data = GrampsImportData::default();
    let mut event_id_by_gramps: HashMap<String, EntityId> = HashMap::new();
    let mut place_id_by_gramps: HashMap<String, EntityId> = HashMap::new();
    let mut person_id_by_gramps: HashMap<String, EntityId> = HashMap::new();
    let mut source_id_by_gramps: HashMap<String, EntityId> = HashMap::new();
    let mut repository_id_by_gramps: HashMap<String, EntityId> = HashMap::new();

    if let Some(places_root) = first_child_named(root, "places") {
        for place_elem in child_elements_named(places_root, "placeobj") {
            let Some(place_xml_id) = place_elem
                .attributes
                .get("id")
                .map(std::string::ToString::to_string)
            else {
                continue;
            };

            let place_id = entity_id_from_seed("place", &place_xml_id);
            place_id_by_gramps.insert(place_xml_id.clone(), place_id);

            let place_name = child_text(place_elem, "ptitle")
                .or_else(|| child_text(place_elem, "name"))
                .unwrap_or_else(|| place_xml_id.clone());

            data.places.push(Place {
                id: place_id,
                place_type: PlaceType::Unknown,
                names: vec![PlaceName {
                    name: place_name,
                    language: None,
                    date_range: None,
                }],
                coordinates: None,
                enclosed_by: Vec::new(),
                external_ids: Vec::new(),
            });
        }
    }

    if let Some(events_root) = first_child_named(root, "events") {
        for event_elem in child_elements_named(events_root, "event") {
            let Some(event_xml_id) = event_elem
                .attributes
                .get("id")
                .map(std::string::ToString::to_string)
            else {
                continue;
            };
            let event_id = entity_id_from_seed("event", &event_xml_id);
            event_id_by_gramps.insert(event_xml_id.clone(), event_id);

            let event_type =
                parse_event_type(event_elem.attributes.get("type").map(String::as_str));
            let date = first_child_named(event_elem, "dateval")
                .and_then(|d| d.attributes.get("val"))
                .and_then(|v| parse_date_value(v));

            let place_ref = first_child_named(event_elem, "place")
                .and_then(|p| p.attributes.get("hlink"))
                .and_then(|id| place_id_by_gramps.get(id))
                .copied();

            let description = child_text(event_elem, "description");

            data.events.push(Event {
                id: event_id,
                event_type,
                date,
                place_ref,
                participants: Vec::new(),
                description,
                _raw_gedcom: BTreeMap::new(),
            });
        }
    }

    if let Some(repositories_root) = first_child_named(root, "repositories") {
        for repository_elem in child_elements_named(repositories_root, "repository") {
            let Some(repo_xml_id) = repository_elem
                .attributes
                .get("id")
                .map(std::string::ToString::to_string)
            else {
                continue;
            };
            let repo_id = entity_id_from_seed("repository", &repo_xml_id);
            repository_id_by_gramps.insert(repo_xml_id.clone(), repo_id);

            let name = child_text(repository_elem, "rname").unwrap_or(repo_xml_id.clone());

            data.repositories.push(Repository {
                id: repo_id,
                name,
                repository_type: RepositoryType::Archive,
                address: child_text(repository_elem, "address"),
                urls: child_text(repository_elem, "url").into_iter().collect(),
                original_xref: Some(format!("@R{}@", repo_xml_id)),
                _raw_gedcom: BTreeMap::new(),
            });
        }
    }

    if let Some(sources_root) = first_child_named(root, "sources") {
        for source_elem in child_elements_named(sources_root, "source") {
            let Some(source_xml_id) = source_elem
                .attributes
                .get("id")
                .map(std::string::ToString::to_string)
            else {
                continue;
            };
            let source_id = entity_id_from_seed("source", &source_xml_id);
            source_id_by_gramps.insert(source_xml_id.clone(), source_id);

            let mut repository_refs = Vec::new();
            for reporef in child_elements_named(source_elem, "reporef") {
                if let Some(repo_xml_id) = reporef.attributes.get("hlink")
                    && let Some(repo_id) = repository_id_by_gramps.get(repo_xml_id)
                {
                    repository_refs.push(rustygene_core::evidence::RepositoryRef {
                        repository_id: *repo_id,
                        call_number: child_text(reporef, "callno"),
                        media_type: child_text(reporef, "mediatype"),
                    });
                }
            }

            data.sources.push(Source {
                id: source_id,
                title: child_text(source_elem, "stitle").unwrap_or(source_xml_id.clone()),
                author: child_text(source_elem, "author"),
                publication_info: child_text(source_elem, "publication"),
                abbreviation: child_text(source_elem, "abbrev"),
                repository_refs,
                original_xref: Some(format!("@S{}@", source_xml_id)),
                _raw_gedcom: BTreeMap::new(),
            });
        }
    }

    if let Some(citations_root) = first_child_named(root, "citations") {
        for citation_elem in child_elements_named(citations_root, "citation") {
            let Some(citation_xml_id) = citation_elem
                .attributes
                .get("id")
                .map(std::string::ToString::to_string)
            else {
                continue;
            };

            let source_xml_id = citation_elem
                .attributes
                .get("source")
                .map(std::string::ToString::to_string)
                .or_else(|| {
                    first_child_named(citation_elem, "sourceref")
                        .and_then(|s| s.attributes.get("hlink"))
                        .map(std::string::ToString::to_string)
                });
            let Some(source_xml_id) = source_xml_id else {
                continue;
            };
            let Some(source_id) = source_id_by_gramps.get(&source_xml_id).copied() else {
                continue;
            };

            data.citations.push(Citation {
                id: entity_id_from_seed("citation", &citation_xml_id),
                source_id,
                volume: child_text(citation_elem, "volume"),
                page: child_text(citation_elem, "page"),
                folio: child_text(citation_elem, "folio"),
                entry: child_text(citation_elem, "entry"),
                confidence_level: child_text(citation_elem, "confidence")
                    .and_then(|v| v.parse::<u8>().ok()),
                date_accessed: None,
                transcription: child_text(citation_elem, "text"),
                _raw_gedcom: BTreeMap::new(),
            });
        }
    }

    let mut person_event_refs: HashMap<EntityId, Vec<EntityId>> = HashMap::new();
    if let Some(people_root) = first_child_named(root, "people") {
        for person_elem in child_elements_named(people_root, "person") {
            let Some(person_xml_id) = person_elem
                .attributes
                .get("id")
                .map(std::string::ToString::to_string)
            else {
                continue;
            };
            let person_id = entity_id_from_seed("person", &person_xml_id);
            person_id_by_gramps.insert(person_xml_id.clone(), person_id);

            let name_elem = first_child_named(person_elem, "name");
            let given_names = name_elem
                .and_then(|name| {
                    name.attributes
                        .get("first")
                        .cloned()
                        .or_else(|| child_text(name, "first"))
                })
                .unwrap_or_else(|| "Unknown".to_string());
            let surname_value = name_elem
                .and_then(|name| {
                    name.attributes
                        .get("surname")
                        .cloned()
                        .or_else(|| child_text(name, "surname"))
                })
                .unwrap_or_else(|| "Unknown".to_string());

            let names = vec![PersonName {
                name_type: NameType::Birth,
                date_range: None,
                given_names,
                call_name: None,
                surnames: vec![Surname {
                    value: surname_value,
                    origin_type: SurnameOrigin::Patrilineal,
                    connector: None,
                }],
                prefix: None,
                suffix: None,
                sort_as: None,
            }];

            data.persons.push(Person {
                id: person_id,
                names,
                gender: parse_gender(child_text(person_elem, "gender").as_deref()),
                living: false,
                private: false,
                original_xref: Some(format!("@I{}@", person_xml_id)),
                _raw_gedcom: BTreeMap::new(),
            });

            for eventref in child_elements_named(person_elem, "eventref") {
                if let Some(event_xml_id) = eventref.attributes.get("hlink")
                    && let Some(event_id) = event_id_by_gramps.get(event_xml_id)
                {
                    person_event_refs
                        .entry(person_id)
                        .or_default()
                        .push(*event_id);
                }
            }
        }
    }

    let mut family_event_refs: HashMap<EntityId, Vec<EntityId>> = HashMap::new();
    if let Some(families_root) = first_child_named(root, "families") {
        for family_elem in child_elements_named(families_root, "family") {
            let Some(family_xml_id) = family_elem
                .attributes
                .get("id")
                .map(std::string::ToString::to_string)
            else {
                continue;
            };
            let family_id = entity_id_from_seed("family", &family_xml_id);

            let partner1_id = first_child_named(family_elem, "father")
                .and_then(|f| f.attributes.get("hlink"))
                .and_then(|p| person_id_by_gramps.get(p))
                .copied();
            let partner2_id = first_child_named(family_elem, "mother")
                .and_then(|m| m.attributes.get("hlink"))
                .and_then(|p| person_id_by_gramps.get(p))
                .copied();

            let child_links: Vec<ChildLink> = child_elements_named(family_elem, "childref")
                .filter_map(|childref| {
                    childref
                        .attributes
                        .get("hlink")
                        .and_then(|person_xml_id| person_id_by_gramps.get(person_xml_id))
                        .copied()
                        .map(|child_id| ChildLink {
                            child_id,
                            lineage_type: LineageType::Biological,
                        })
                })
                .collect();

            let couple_relationship = if let (Some(p1), Some(p2)) = (partner1_id, partner2_id) {
                let rel = Relationship {
                    id: entity_id_from_seed("relationship", &format!("{}:{}:couple", p1, p2)),
                    person1_id: p1,
                    person2_id: p2,
                    relationship_type: RelationshipType::Couple,
                    supporting_event: None,
                    _raw_gedcom: BTreeMap::new(),
                };
                let rel_id = rel.id;
                data.relationships.push(rel);
                Some(rel_id)
            } else {
                None
            };

            for child in &child_links {
                for parent in [partner1_id, partner2_id].into_iter().flatten() {
                    data.relationships.push(Relationship {
                        id: entity_id_from_seed(
                            "relationship",
                            &format!("{}:{}:parent_child", parent, child.child_id),
                        ),
                        person1_id: parent,
                        person2_id: child.child_id,
                        relationship_type: RelationshipType::ParentChild,
                        supporting_event: None,
                        _raw_gedcom: BTreeMap::new(),
                    });
                }
            }

            data.families.push(Family {
                id: family_id,
                partner1_id,
                partner2_id,
                partner_link: PartnerLink::Unknown,
                couple_relationship,
                child_links,
                original_xref: Some(format!("@F{}@", family_xml_id)),
                _raw_gedcom: BTreeMap::new(),
            });

            for eventref in child_elements_named(family_elem, "eventref") {
                if let Some(event_xml_id) = eventref.attributes.get("hlink")
                    && let Some(event_id) = event_id_by_gramps.get(event_xml_id)
                {
                    family_event_refs
                        .entry(family_id)
                        .or_default()
                        .push(*event_id);
                }
            }
        }
    }

    for event in &mut data.events {
        for (person_id, event_ids) in &person_event_refs {
            if event_ids.contains(&event.id)
                && !event
                    .participants
                    .iter()
                    .any(|participant| participant.person_id == *person_id)
            {
                event.participants.push(EventParticipant {
                    person_id: *person_id,
                    role: EventRole::Principal,
                    census_role: None,
                });
            }
        }

        for (family_id, event_ids) in &family_event_refs {
            if !event_ids.contains(&event.id) {
                continue;
            }

            if let Some(family) = data.families.iter().find(|family| family.id == *family_id) {
                for partner in [family.partner1_id, family.partner2_id]
                    .into_iter()
                    .flatten()
                {
                    if !event
                        .participants
                        .iter()
                        .any(|participant| participant.person_id == partner)
                    {
                        event.participants.push(EventParticipant {
                            person_id: partner,
                            role: EventRole::Principal,
                            census_role: None,
                        });
                    }
                }
            }
        }
    }

    if let Some(notes_root) = first_child_named(root, "notes") {
        for note_elem in child_elements_named(notes_root, "note") {
            let Some(note_xml_id) = note_elem
                .attributes
                .get("id")
                .map(std::string::ToString::to_string)
            else {
                continue;
            };
            data.notes.push(Note {
                id: entity_id_from_seed("note", &note_xml_id),
                text: child_text(note_elem, "text")
                    .or_else(|| element_text(note_elem))
                    .unwrap_or_default(),
                note_type: NoteType::General,
                original_xref: Some(format!("@N{}@", note_xml_id)),
                _raw_gedcom: BTreeMap::new(),
            });
        }
    }

    if let Some(media_root) = first_child_named(root, "media") {
        for media_elem in child_elements_named(media_root, "object") {
            let Some(media_xml_id) = media_elem
                .attributes
                .get("id")
                .map(std::string::ToString::to_string)
            else {
                continue;
            };
            let file_path = first_child_named(media_elem, "file")
                .and_then(|f| {
                    f.attributes
                        .get("src")
                        .map(std::string::ToString::to_string)
                })
                .or_else(|| child_text(media_elem, "file"))
                .unwrap_or_default();

            let mime_type = child_text(media_elem, "mime")
                .unwrap_or_else(|| "application/octet-stream".to_string());

            data.media.push(Media {
                id: entity_id_from_seed("media", &media_xml_id),
                file_path,
                content_hash: format!("gramps:{media_xml_id}"),
                mime_type,
                thumbnail_path: None,
                ocr_text: None,
                dimensions_px: None,
                physical_dimensions_mm: None,
                caption: child_text(media_elem, "title"),
                original_xref: Some(format!("@O{}@", media_xml_id)),
                _raw_gedcom: BTreeMap::new(),
            });
        }
    }

    data
}

fn make_assertion(value: serde_json::Value, import_job_id: &str) -> JsonAssertion {
    JsonAssertion {
        id: EntityId::new(),
        value,
        confidence: 0.95,
        status: AssertionStatus::Confirmed,
        evidence_type: EvidenceType::Direct,
        source_citations: Vec::new(),
        proposed_by: ActorRef::Import(import_job_id.to_string()),
        created_at: Utc::now(),
        reviewed_at: None,
        reviewed_by: None,
    }
}

async fn import_gramps_parsed_data(
    backend: &SqliteBackend,
    import_job_id: &str,
    data: &GrampsImportData,
) -> Result<GrampsImportReport, String> {
    for person in &data.persons {
        backend
            .create_person(person)
            .await
            .map_err(|e| e.message.clone())?;
    }
    for family in &data.families {
        backend
            .create_family(family)
            .await
            .map_err(|e| e.message.clone())?;
    }
    for relationship in &data.relationships {
        backend
            .create_relationship(relationship)
            .await
            .map_err(|e| e.message.clone())?;
    }
    for event in &data.events {
        backend
            .create_event(event)
            .await
            .map_err(|e| e.message.clone())?;
    }
    for place in &data.places {
        backend
            .create_place(place)
            .await
            .map_err(|e| e.message.clone())?;
    }
    for repository in &data.repositories {
        backend
            .create_repository(repository)
            .await
            .map_err(|e| e.message.clone())?;
    }
    for source in &data.sources {
        backend
            .create_source(source)
            .await
            .map_err(|e| e.message.clone())?;
    }
    for citation in &data.citations {
        backend
            .create_citation(citation)
            .await
            .map_err(|e| e.message.clone())?;
    }
    for note in &data.notes {
        backend
            .create_note(note)
            .await
            .map_err(|e| e.message.clone())?;
    }
    for media in &data.media {
        backend
            .create_media(media)
            .await
            .map_err(|e| e.message.clone())?;
    }

    let mut assertions_created = 0usize;
    for person in &data.persons {
        for name in &person.names {
            backend
                .create_assertion(
                    person.id,
                    EntityType::Person,
                    "name",
                    &make_assertion(
                        serde_json::to_value(name).map_err(|e| e.to_string())?,
                        import_job_id,
                    ),
                )
                .await
                .map_err(|e| e.message.clone())?;
            assertions_created += 1;
        }
        backend
            .create_assertion(
                person.id,
                EntityType::Person,
                "gender",
                &make_assertion(
                    serde_json::to_value(&person.gender).map_err(|e| e.to_string())?,
                    import_job_id,
                ),
            )
            .await
            .map_err(|e| e.message.clone())?;
        assertions_created += 1;
    }

    for family in &data.families {
        if let Some(partner1) = family.partner1_id {
            backend
                .create_assertion(
                    family.id,
                    EntityType::Family,
                    "partner1_id",
                    &make_assertion(
                        serde_json::to_value(partner1).map_err(|e| e.to_string())?,
                        import_job_id,
                    ),
                )
                .await
                .map_err(|e| e.message.clone())?;
            assertions_created += 1;
        }
        if let Some(partner2) = family.partner2_id {
            backend
                .create_assertion(
                    family.id,
                    EntityType::Family,
                    "partner2_id",
                    &make_assertion(
                        serde_json::to_value(partner2).map_err(|e| e.to_string())?,
                        import_job_id,
                    ),
                )
                .await
                .map_err(|e| e.message.clone())?;
            assertions_created += 1;
        }
        for child in &family.child_links {
            backend
                .create_assertion(
                    family.id,
                    EntityType::Family,
                    "child_link",
                    &make_assertion(
                        serde_json::to_value(child).map_err(|e| e.to_string())?,
                        import_job_id,
                    ),
                )
                .await
                .map_err(|e| e.message.clone())?;
            assertions_created += 1;
        }
    }

    for event in &data.events {
        backend
            .create_assertion(
                event.id,
                EntityType::Event,
                "event_type",
                &make_assertion(
                    serde_json::to_value(&event.event_type).map_err(|e| e.to_string())?,
                    import_job_id,
                ),
            )
            .await
            .map_err(|e| e.message.clone())?;
        assertions_created += 1;

        if let Some(date) = &event.date {
            backend
                .create_assertion(
                    event.id,
                    EntityType::Event,
                    "date",
                    &make_assertion(
                        serde_json::to_value(date).map_err(|e| e.to_string())?,
                        import_job_id,
                    ),
                )
                .await
                .map_err(|e| e.message.clone())?;
            assertions_created += 1;
        }

        if let Some(place_ref) = event.place_ref {
            backend
                .create_assertion(
                    event.id,
                    EntityType::Event,
                    "place_ref",
                    &make_assertion(
                        serde_json::to_value(place_ref).map_err(|e| e.to_string())?,
                        import_job_id,
                    ),
                )
                .await
                .map_err(|e| e.message.clone())?;
            assertions_created += 1;
        }

        for participant in &event.participants {
            backend
                .create_assertion(
                    event.id,
                    EntityType::Event,
                    "participant",
                    &make_assertion(
                        serde_json::to_value(participant).map_err(|e| e.to_string())?,
                        import_job_id,
                    ),
                )
                .await
                .map_err(|e| e.message.clone())?;
            assertions_created += 1;
        }
    }

    for place in &data.places {
        for name in &place.names {
            backend
                .create_assertion(
                    place.id,
                    EntityType::Place,
                    "name",
                    &make_assertion(
                        serde_json::to_value(name).map_err(|e| e.to_string())?,
                        import_job_id,
                    ),
                )
                .await
                .map_err(|e| e.message.clone())?;
            assertions_created += 1;
        }
    }

    for source in &data.sources {
        backend
            .create_assertion(
                source.id,
                EntityType::Source,
                "title",
                &make_assertion(
                    serde_json::to_value(&source.title).map_err(|e| e.to_string())?,
                    import_job_id,
                ),
            )
            .await
            .map_err(|e| e.message.clone())?;
        assertions_created += 1;
    }

    for citation in &data.citations {
        backend
            .create_assertion(
                citation.id,
                EntityType::Citation,
                "source_id",
                &make_assertion(
                    serde_json::to_value(citation.source_id).map_err(|e| e.to_string())?,
                    import_job_id,
                ),
            )
            .await
            .map_err(|e| e.message.clone())?;
        assertions_created += 1;
    }

    let mut entities_created_by_type = BTreeMap::new();
    entities_created_by_type.insert("person".to_string(), data.persons.len());
    entities_created_by_type.insert("family".to_string(), data.families.len());
    entities_created_by_type.insert("relationship".to_string(), data.relationships.len());
    entities_created_by_type.insert("event".to_string(), data.events.len());
    entities_created_by_type.insert("place".to_string(), data.places.len());
    entities_created_by_type.insert("source".to_string(), data.sources.len());
    entities_created_by_type.insert("citation".to_string(), data.citations.len());
    entities_created_by_type.insert("repository".to_string(), data.repositories.len());
    entities_created_by_type.insert("note".to_string(), data.notes.len());
    entities_created_by_type.insert("media".to_string(), data.media.len());

    Ok(GrampsImportReport {
        entities_created_by_type,
        assertions_created,
    })
}

pub fn import_gramps_xml_to_sqlite(
    backend: &SqliteBackend,
    import_job_id: &str,
    input: &str,
) -> Result<GrampsImportReport, String> {
    let root = Element::parse(input.as_bytes()).map_err(|e| format!("invalid Gramps XML: {e}"))?;
    let data = parse_database(&root);

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .map_err(|e| format!("failed to build runtime: {e}"))?;

    runtime.block_on(import_gramps_parsed_data(backend, import_job_id, &data))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use rustygene_storage::run_migrations;

    #[test]
    fn imports_minimal_gramps_xml() {
        let xml = r#"
<database>
  <places>
    <placeobj id="PL1"><ptitle>Springfield</ptitle></placeobj>
  </places>
  <events>
    <event id="E1" type="Birth"><dateval val="1900-01-01"/><place hlink="PL1"/></event>
  </events>
  <repositories>
    <repository id="R1"><rname>State Archive</rname></repository>
  </repositories>
  <sources>
    <source id="S1"><stitle>Birth Register</stitle><reporef hlink="R1"/></source>
  </sources>
  <citations>
    <citation id="C1" source="S1"><page>42</page></citation>
  </citations>
  <notes>
    <note id="N1"><text>Research note</text></note>
  </notes>
  <media>
    <object id="O1"><file src="media/birth.jpg"/><mime>image/jpeg</mime><title>Birth Scan</title></object>
  </media>
  <people>
    <person id="P1"><name first="John" surname="Doe"/><gender>M</gender><eventref hlink="E1"/></person>
    <person id="P2"><name first="Jane" surname="Doe"/><gender>F</gender></person>
  </people>
  <families>
    <family id="F1"><father hlink="P1"/><mother hlink="P2"/><eventref hlink="E1"/></family>
  </families>
</database>
"#;

        let mut conn = Connection::open_in_memory().expect("open db");
        run_migrations(&mut conn).expect("migrate");
        let backend = SqliteBackend::new(conn);

        let report =
            import_gramps_xml_to_sqlite(&backend, "gramps-test", xml).expect("import gramps xml");

        assert_eq!(report.entities_created_by_type.get("person"), Some(&2));
        assert_eq!(report.entities_created_by_type.get("family"), Some(&1));
        assert_eq!(report.entities_created_by_type.get("event"), Some(&1));
        assert_eq!(report.entities_created_by_type.get("source"), Some(&1));
        assert_eq!(report.entities_created_by_type.get("citation"), Some(&1));
        assert_eq!(report.entities_created_by_type.get("repository"), Some(&1));
        assert_eq!(report.entities_created_by_type.get("note"), Some(&1));
        assert_eq!(report.entities_created_by_type.get("media"), Some(&1));
        assert!(report.assertions_created > 0);
    }
}
