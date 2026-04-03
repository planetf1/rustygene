use std::collections::BTreeMap;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use rustygene_core::event::Event;
use rustygene_core::family::{ChildLink, Family, PartnerLink, Relationship, RelationshipType};
use rustygene_core::types::EntityId;
use rustygene_storage::Pagination;
use serde::Deserialize;
use uuid::Uuid;

use crate::errors::ApiError;
use crate::models::families::{CreateFamilyRequest, FamilyDetailResponse, PartnerSummary};
use crate::models::persons::AssertionValueResponse;
use crate::AppState;

#[derive(Debug, Deserialize)]
struct FamiliesQuery {
    #[serde(default)]
    limit: Option<u32>,
    #[serde(default)]
    offset: Option<u32>,
    #[serde(default)]
    #[serde(rename = "person_id")]
    _person_id: Option<String>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_families).post(create_family))
        .route(
            "/:id",
            get(get_family).put(update_family).delete(delete_family),
        )
        .route("/:id/assertions", get(get_family_assertions))
}

async fn list_families(
    State(state): State<AppState>,
    Query(query): Query<FamiliesQuery>,
) -> Result<Json<Vec<FamilyDetailResponse>>, ApiError> {
    let pagination = Pagination {
        limit: query.limit.unwrap_or(100),
        offset: query.offset.unwrap_or(0),
    };

    let families = state.storage.list_families(pagination).await?;

    let mut response = Vec::with_capacity(families.len());
    for family in families {
        let partner1 = if let Some(pid) = family.partner1_id {
            state.storage.get_person(pid).await.ok()
        } else {
            None
        };
        let partner2 = if let Some(pid) = family.partner2_id {
            state.storage.get_person(pid).await.ok()
        } else {
            None
        };
        let events = fetch_family_events(&state, &family).await?;
        let assertions = state
            .storage
            .list_assertion_records_for_entity(family.id)
            .await?;

        let detail = FamilyDetailResponse {
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
                .filter_map(|child| {
                    vec![&partner1, &partner2]
                        .into_iter()
                        .find_map(|p| {
                            p.as_ref().and_then(|person| {
                                if person.id == child.child_id {
                                    Some(super::super::models::families::ChildSummary {
                                        id: child.child_id,
                                        display_name: display_name_for_person(person),
                                        lineage_type: format!("{:?}", child.lineage_type),
                                    })
                                } else {
                                    None
                                }
                            })
                        })
                        .or_else(|| {
                            Some(super::super::models::families::ChildSummary {
                                id: child.child_id,
                                display_name: format!("Person {}", child.child_id),
                                lineage_type: format!("{:?}", child.lineage_type),
                            })
                        })
                })
                .collect(),
            events: events
                .into_iter()
                .map(|e| super::super::models::families::EventSummary {
                    id: e.id,
                    event_type: format!("{:?}", e.event_type),
                    date: e.date.as_ref().map(|d| format!("{:?}", d)),
                })
                .collect(),
            assertion_counts: assertions.iter().fold(BTreeMap::new(), |mut acc, asrt| {
                *acc.entry(asrt.field.clone()).or_insert(0) += 1;
                acc
            }),
        };

        response.push(detail);
    }

    Ok(Json(response))
}

async fn create_family(
    State(state): State<AppState>,
    Json(request): Json<CreateFamilyRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let family_id = EntityId::new();
    let partner_link = request.partner_link.unwrap_or(PartnerLink::Unknown);

    // Create the family unit with Principle 2: linking assertions, not just foreign keys
    let family = Family {
        id: family_id,
        partner1_id: request.partner1_id,
        partner2_id: request.partner2_id,
        partner_link: partner_link.clone(),
        couple_relationship: None, // Will be set after creating relationship
        child_links: request
            .child_ids
            .iter()
            .map(|&child_id| ChildLink {
                child_id,
                lineage_type: Default::default(),
            })
            .collect(),
        original_xref: None,
        _raw_gedcom: BTreeMap::new(),
    };

    state.storage.create_family(&family).await?;

    // If both partners exist, create a couple relationship (Principle 2)
    if let (Some(partner1_id), Some(partner2_id)) = (request.partner1_id, request.partner2_id) {
        let relationship = Relationship {
            id: EntityId::new(),
            person1_id: partner1_id,
            person2_id: partner2_id,
            relationship_type: RelationshipType::Couple,
            supporting_event: None,
            _raw_gedcom: BTreeMap::new(),
        };

        state.storage.create_relationship(&relationship).await?;
    }

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "id": family_id })),
    ))
}

async fn get_family(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<FamilyDetailResponse>, ApiError> {
    let family_id = parse_entity_id(&id)?;
    let family = state.storage.get_family(family_id).await?;

    let partner1 = if let Some(pid) = family.partner1_id {
        state.storage.get_person(pid).await.ok()
    } else {
        None
    };
    let partner2 = if let Some(pid) = family.partner2_id {
        state.storage.get_person(pid).await.ok()
    } else {
        None
    };
    let events = fetch_family_events(&state, &family).await?;
    let assertions = state
        .storage
        .list_assertion_records_for_entity(family_id)
        .await?;

    Ok(Json(FamilyDetailResponse {
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
            .filter_map(|child| {
                vec![&partner1, &partner2]
                    .into_iter()
                    .find_map(|p| {
                        p.as_ref().and_then(|person| {
                            if person.id == child.child_id {
                                Some(super::super::models::families::ChildSummary {
                                    id: child.child_id,
                                    display_name: display_name_for_person(person),
                                    lineage_type: format!("{:?}", child.lineage_type),
                                })
                            } else {
                                None
                            }
                        })
                    })
                    .or_else(|| {
                        Some(super::super::models::families::ChildSummary {
                            id: child.child_id,
                            display_name: format!("Person {}", child.child_id),
                            lineage_type: format!("{:?}", child.lineage_type),
                        })
                    })
            })
            .collect(),
        events: events
            .into_iter()
            .map(|e| super::super::models::families::EventSummary {
                id: e.id,
                event_type: format!("{:?}", e.event_type),
                date: e.date.as_ref().map(|d| format!("{:?}", d)),
            })
            .collect(),
        assertion_counts: assertions.iter().fold(BTreeMap::new(), |mut acc, asrt| {
            *acc.entry(asrt.field.clone()).or_insert(0) += 1;
            acc
        }),
    }))
}

async fn update_family(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<CreateFamilyRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let family_id = parse_entity_id(&id)?;
    let mut family = state.storage.get_family(family_id).await?;

    // Update partners if provided
    if let Some(pid) = request.partner1_id {
        family.partner1_id = Some(pid);
    }
    if let Some(pid) = request.partner2_id {
        family.partner2_id = Some(pid);
    }
    if let Some(link) = request.partner_link {
        family.partner_link = link;
    }

    // Update child links
    if !request.child_ids.is_empty() {
        family.child_links = request
            .child_ids
            .into_iter()
            .map(|child_id| ChildLink {
                child_id,
                lineage_type: Default::default(),
            })
            .collect();
    }

    state.storage.update_family(&family).await?;

    Ok(Json(serde_json::json!({ "id": family_id })))
}

async fn delete_family(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let family_id = parse_entity_id(&id)?;
    let _ = state.storage.get_family(family_id).await?;
    state.storage.delete_family(family_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_family_assertions(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<BTreeMap<String, Vec<AssertionValueResponse>>>, ApiError> {
    let family_id = parse_entity_id(&id)?;
    let _ = state.storage.get_family(family_id).await?;
    let records = state
        .storage
        .list_assertion_records_for_entity(family_id)
        .await?;

    let mut grouped: BTreeMap<String, Vec<AssertionValueResponse>> = BTreeMap::new();
    for record in records {
        grouped
            .entry(record.field.clone())
            .or_default()
            .push(AssertionValueResponse {
                assertion_id: record.assertion.id,
                field: record.field,
                value: record.assertion.value.clone(),
                status: record.assertion.status.clone(),
                confidence: record.assertion.confidence,
                evidence_type: record.assertion.evidence_type.clone(),
                sources: record.assertion.source_citations.clone(),
            });
    }

    Ok(Json(grouped))
}

// Helpers

fn parse_entity_id(raw: &str) -> Result<EntityId, ApiError> {
    Uuid::parse_str(raw)
        .map(EntityId)
        .map_err(|_| ApiError::BadRequest(format!("invalid entity id: {raw}")))
}

async fn fetch_family_events(_state: &AppState, _family: &Family) -> Result<Vec<Event>, ApiError> {
    // Fetch all events, filter by participants
    // For now, return empty - this would need filtering logic
    // In a real implementation, we'd query events where participants include the partners
    Ok(Vec::new())
}

fn display_name_for_person(person: &rustygene_core::person::Person) -> String {
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
