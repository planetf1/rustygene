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
use crate::models::families::{
    CreateFamilyRequest, FamilyDetailResponse, FamilyListResponse, PartnerSummary,
};
use crate::models::persons::AssertionValueResponse;
use crate::AppState;

#[derive(Debug, Deserialize)]
struct FamiliesQuery {
    #[serde(default)]
    limit: Option<u32>,
    #[serde(default)]
    offset: Option<u32>,
    /// Free-text search on partner names (case-insensitive substring).
    #[serde(default)]
    q: Option<String>,
    /// Sort field: family | marriage_year | children (default: family)
    #[serde(default)]
    sort: Option<String>,
    /// Sort direction: asc | desc (default: asc)
    #[serde(default)]
    dir: Option<String>,
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
) -> Result<Json<FamilyListResponse>, ApiError> {
    let all_families = state
        .storage
        .list_families(Pagination {
            limit: u32::MAX,
            offset: 0,
        })
        .await?;

    let mut response = Vec::with_capacity(all_families.len());
    for family in all_families {
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
        let child_names = fetch_child_display_names(&state, &family.child_links).await;

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
                .map(|child| super::super::models::families::ChildSummary {
                    id: child.child_id,
                    display_name: child_names
                        .get(&child.child_id)
                        .cloned()
                        .unwrap_or_else(|| format!("Person {}", child.child_id)),
                    lineage_type: format!("{:?}", child.lineage_type),
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

    // Helper: family label for search/sort.
    let family_label = |f: &FamilyDetailResponse| -> String {
        let p1 = f
            .partner1
            .as_ref()
            .map_or("", |p| p.display_name.as_str())
            .to_lowercase();
        let p2 = f
            .partner2
            .as_ref()
            .map_or("", |p| p.display_name.as_str())
            .to_lowercase();
        format!("{p1} {p2}")
    };

    // Apply search filter.
    if let Some(ref q) = query.q {
        let q_lc = q.to_lowercase();
        response.retain(|f| family_label(f).contains(&q_lc));
    }

    // Helper: marriage year for sort.
    let marriage_year = |f: &FamilyDetailResponse| -> Option<i32> {
        f.events
            .iter()
            .find(|e| {
                e.event_type.to_lowercase().contains("marriage")
                    || e.event_type.to_lowercase() == "married"
            })
            .and_then(|e| e.date.as_ref())
            .and_then(|d| {
                d.chars()
                    .filter(|c| c.is_ascii_digit())
                    .collect::<String>()
                    .get(..4)
                    .and_then(|y| y.parse().ok())
            })
    };

    // Apply sort.
    let sort_field = query.sort.as_deref().unwrap_or("family");
    let descending = query.dir.as_deref() == Some("desc");
    match sort_field {
        "marriage_year" => response.sort_by(|a, b| {
            let ord = marriage_year(a).cmp(&marriage_year(b));
            if descending { ord.reverse() } else { ord }
        }),
        "children" => response.sort_by(|a, b| {
            let ord = a.children.len().cmp(&b.children.len());
            if descending { ord.reverse() } else { ord }
        }),
        _ /* "family" */ => response.sort_by(|a, b| {
            let ord = family_label(a).cmp(&family_label(b));
            if descending { ord.reverse() } else { ord }
        }),
    }

    let total = response.len();
    let limit = query.limit.unwrap_or(50) as usize;
    let offset = query.offset.unwrap_or(0) as usize;
    let items = response.into_iter().skip(offset).take(limit).collect();

    Ok(Json(FamilyListResponse { total, items }))
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
    let child_names = fetch_child_display_names(&state, &family.child_links).await;

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
            .map(|child| super::super::models::families::ChildSummary {
                id: child.child_id,
                display_name: child_names
                    .get(&child.child_id)
                    .cloned()
                    .unwrap_or_else(|| format!("Person {}", child.child_id)),
                lineage_type: format!("{:?}", child.lineage_type),
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

/// Fetch display names for all children in a family's child_links.
/// Returns a map from EntityId → display_name. Missing persons are omitted.
async fn fetch_child_display_names(
    state: &AppState,
    child_links: &[ChildLink],
) -> std::collections::HashMap<EntityId, String> {
    let mut names = std::collections::HashMap::with_capacity(child_links.len());
    for link in child_links {
        if let Ok(person) = state.storage.get_person(link.child_id).await {
            names.insert(link.child_id, display_name_for_person(&person));
        }
    }
    names
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
