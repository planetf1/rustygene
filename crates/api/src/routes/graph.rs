use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::Json;
use axum::Router;
use rustygene_core::event::{Event, EventType};
use rustygene_core::kinship::{compute_kinship, PathStep as CorePathStep};
use rustygene_core::person::Person;
use rustygene_core::types::{DateValue, EntityId, Gender};
use rustygene_storage::Pagination;
use serde::Deserialize;
use uuid::Uuid;

use crate::errors::ApiError;
use crate::models::graph::{
    AncestorTreeNode, DescendantTreeNode, NetworkEdge, NetworkGraph, NetworkNode, PathStep,
    PathWithKinship, PedigreeEdge, PedigreeGraph, PedigreeNode,
};
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/ancestors/:id", get(get_ancestors))
        .route("/descendants/:id", get(get_descendants))
        .route("/pedigree/:id", get(get_pedigree))
        .route("/path/:id1/:id2", get(get_path))
        .route("/network/:id", get(get_network))
}

#[derive(Debug, Deserialize)]
struct GenerationsQuery {
    #[serde(default)]
    generations: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct PedigreeQuery {
    #[serde(default)]
    generations: Option<u32>,
    #[serde(default)]
    collapse_pedigree: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct NetworkQuery {
    #[serde(default)]
    radius: Option<u32>,
}

#[derive(Debug, Clone)]
struct PersonSummary {
    display_name: String,
    birth_year: Option<i32>,
    death_year: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct EdgeKey {
    source: EntityId,
    target: EntityId,
    label: String,
    edge_type: String,
}

#[derive(Debug, Clone)]
struct Neighbor {
    to: EntityId,
    label: String,
    direction: String,
    edge_type: String,
}

#[derive(Debug, Clone, Default)]
struct FamilyGraph {
    parents_by_child: HashMap<EntityId, Vec<EntityId>>,
    children_by_parent: HashMap<EntityId, Vec<EntityId>>,
    neighbors: HashMap<EntityId, Vec<Neighbor>>,
}

async fn get_ancestors(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<GenerationsQuery>,
) -> Result<Json<AncestorTreeNode>, ApiError> {
    let person_id = parse_entity_id(&id)?;
    let generations = validate_generations(query.generations.unwrap_or(4))?;

    let _ = state.storage.get_person(person_id).await?;
    let graph = load_family_graph(&state).await?;
    let summaries = collect_summaries_for_ancestors(&state, &graph, person_id, generations).await?;

    let mut lineage_stack = HashSet::new();
    let root = build_ancestor_tree(
        &graph,
        &summaries,
        person_id,
        generations,
        &mut lineage_stack,
    );
    Ok(Json(root))
}

async fn get_descendants(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<GenerationsQuery>,
) -> Result<Json<DescendantTreeNode>, ApiError> {
    let person_id = parse_entity_id(&id)?;
    let generations = validate_generations(query.generations.unwrap_or(3))?;

    let _ = state.storage.get_person(person_id).await?;
    let graph = load_family_graph(&state).await?;
    let summaries =
        collect_summaries_for_descendants(&state, &graph, person_id, generations).await?;

    let mut lineage_stack = HashSet::new();
    let root = build_descendant_tree(
        &graph,
        &summaries,
        person_id,
        generations,
        &mut lineage_stack,
    );
    Ok(Json(root))
}

async fn get_pedigree(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<PedigreeQuery>,
) -> Result<Json<PedigreeGraph>, ApiError> {
    let person_id = parse_entity_id(&id)?;
    let generations = validate_generations(query.generations.unwrap_or(4))?;
    let collapse = query.collapse_pedigree.unwrap_or(true);

    let _ = state.storage.get_person(person_id).await?;
    let graph = load_family_graph(&state).await?;
    let summaries = collect_summaries_for_ancestors(&state, &graph, person_id, generations).await?;

    let mut positions_by_person: BTreeMap<EntityId, Vec<String>> = BTreeMap::new();
    let mut edges: BTreeSet<EdgeKey> = BTreeSet::new();

    let mut queue = VecDeque::new();
    queue.push_back((person_id, "1".to_string(), 0_u32));
    while let Some((current, position, depth)) = queue.pop_front() {
        positions_by_person
            .entry(current)
            .or_default()
            .push(position.clone());

        if depth >= generations {
            continue;
        }

        let (father_id, mother_id) = select_parents(&graph, &summaries, current);
        if let Some(father) = father_id {
            edges.insert(EdgeKey {
                source: father,
                target: current,
                label: "father".to_string(),
                edge_type: "parent_of".to_string(),
            });

            if !collapse || !positions_by_person.contains_key(&father) {
                queue.push_back((father, format!("{position}f"), depth + 1));
            } else {
                positions_by_person
                    .entry(father)
                    .or_default()
                    .push(format!("{position}f"));
            }
        }

        if let Some(mother) = mother_id {
            edges.insert(EdgeKey {
                source: mother,
                target: current,
                label: "mother".to_string(),
                edge_type: "parent_of".to_string(),
            });

            if !collapse || !positions_by_person.contains_key(&mother) {
                queue.push_back((mother, format!("{position}m"), depth + 1));
            } else {
                positions_by_person
                    .entry(mother)
                    .or_default()
                    .push(format!("{position}m"));
            }
        }
    }

    let nodes = positions_by_person
        .into_iter()
        .map(|(pid, mut positions)| {
            positions.sort();
            positions.dedup();
            let summary = summaries
                .get(&pid)
                .cloned()
                .unwrap_or_else(|| PersonSummary {
                    display_name: format!("Unknown ({pid})"),
                    birth_year: None,
                    death_year: None,
                });

            PedigreeNode {
                person_id: pid,
                display_name: summary.display_name,
                birth_year: summary.birth_year,
                death_year: summary.death_year,
                confidence: 1.0,
                primary_position: positions
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "1".to_string()),
                collapsed_from: positions.into_iter().skip(1).collect(),
            }
        })
        .collect::<Vec<_>>();

    let edges = edges
        .into_iter()
        .map(|edge| PedigreeEdge {
            source: edge.source,
            target: edge.target,
            label: edge.label,
        })
        .collect::<Vec<_>>();

    Ok(Json(PedigreeGraph {
        root_id: person_id,
        nodes,
        edges,
    }))
}

async fn get_path(
    State(state): State<AppState>,
    Path((id1, id2)): Path<(String, String)>,
) -> Result<Json<PathWithKinship>, ApiError> {
    let from_id = parse_entity_id(&id1)?;
    let to_id = parse_entity_id(&id2)?;

    let _ = state.storage.get_person(from_id).await?;
    let _ = state.storage.get_person(to_id).await?;

    if from_id == to_id {
        let path_with_kinship = PathWithKinship {
            path: vec![PathStep {
                person_id: from_id,
                relationship_label: "self".to_string(),
                direction: "none".to_string(),
            }],
            kinship_name: "self".to_string(),
        };
        return Ok(Json(path_with_kinship));
    }

    let graph = load_family_graph(&state).await?;
    let Some(path) = shortest_path(&graph, from_id, to_id) else {
        return Err(ApiError::NotFound(format!(
            "no relationship path between {from_id} and {to_id}"
        )));
    };

    // Convert PathStep to kinship calculator format
    let kinship_path: Vec<CorePathStep> = path
        .iter()
        .map(|step| CorePathStep {
            direction: step.direction.clone(),
            label: step.relationship_label.clone(),
            person_id: step.person_id,
        })
        .collect();

    let kinship_result = compute_kinship(&kinship_path);
    let path_with_kinship = PathWithKinship {
        path,
        kinship_name: kinship_result.kinship_name,
    };

    Ok(Json(path_with_kinship))
}

async fn get_network(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<NetworkQuery>,
) -> Result<Json<NetworkGraph>, ApiError> {
    let center_id = parse_entity_id(&id)?;
    let radius = query.radius.unwrap_or(3).min(10);

    let _ = state.storage.get_person(center_id).await?;
    let graph = load_family_graph(&state).await?;

    let mut visited_depth: HashMap<EntityId, u32> = HashMap::new();
    let mut queue = VecDeque::new();
    visited_depth.insert(center_id, 0);
    queue.push_back(center_id);

    let mut edge_set: BTreeSet<EdgeKey> = BTreeSet::new();
    while let Some(current) = queue.pop_front() {
        let current_depth = visited_depth.get(&current).copied().unwrap_or(0);
        if current_depth >= radius {
            continue;
        }

        for neighbor in graph.neighbors.get(&current).cloned().unwrap_or_default() {
            edge_set.insert(EdgeKey {
                source: current,
                target: neighbor.to,
                label: neighbor.label.clone(),
                edge_type: neighbor.edge_type,
            });

            if let std::collections::hash_map::Entry::Vacant(entry) =
                visited_depth.entry(neighbor.to)
            {
                entry.insert(current_depth + 1);
                queue.push_back(neighbor.to);
            }
        }
    }

    let mut ids = visited_depth.keys().copied().collect::<Vec<_>>();
    ids.sort();
    let summaries = collect_summaries_for_ids(&state, ids.iter().copied()).await?;

    let nodes = ids
        .into_iter()
        .map(|pid| {
            let summary = summaries
                .get(&pid)
                .cloned()
                .unwrap_or_else(|| PersonSummary {
                    display_name: format!("Unknown ({pid})"),
                    birth_year: None,
                    death_year: None,
                });

            NetworkNode {
                id: pid,
                label: summary.display_name,
                r#type: "person".to_string(),
                birth_year: summary.birth_year,
                death_year: summary.death_year,
            }
        })
        .collect::<Vec<_>>();

    let visited_ids = visited_depth.keys().copied().collect::<HashSet<_>>();
    let edges = edge_set
        .into_iter()
        .filter(|edge| visited_ids.contains(&edge.source) && visited_ids.contains(&edge.target))
        .map(|edge| NetworkEdge {
            source: edge.source,
            target: edge.target,
            label: edge.label,
            edge_type: edge.edge_type,
        })
        .collect::<Vec<_>>();

    Ok(Json(NetworkGraph { nodes, edges }))
}

fn parse_entity_id(raw: &str) -> Result<EntityId, ApiError> {
    Uuid::parse_str(raw)
        .map(EntityId)
        .map_err(|_| ApiError::BadRequest(format!("invalid entity id: {raw}")))
}

fn validate_generations(generations: u32) -> Result<u32, ApiError> {
    if generations > 10 {
        return Err(ApiError::BadRequest(
            "generations must be <= 10".to_string(),
        ));
    }
    Ok(generations)
}

async fn load_family_graph(state: &AppState) -> Result<FamilyGraph, ApiError> {
    let mut offset = 0_u32;
    let mut families = Vec::new();
    loop {
        let batch = state
            .storage
            .list_families(Pagination { limit: 500, offset })
            .await?;

        if batch.is_empty() {
            break;
        }

        offset += batch.len() as u32;
        families.extend(batch);
    }

    let mut graph = FamilyGraph::default();

    for family in families {
        let mut parents = Vec::new();
        if let Some(p1) = family.partner1_id {
            parents.push(p1);
        }
        if let Some(p2) = family.partner2_id {
            parents.push(p2);
        }

        if parents.len() == 2 {
            add_neighbor(
                &mut graph.neighbors,
                parents[0],
                Neighbor {
                    to: parents[1],
                    label: "partner".to_string(),
                    direction: "undirected".to_string(),
                    edge_type: "partner".to_string(),
                },
            );
            add_neighbor(
                &mut graph.neighbors,
                parents[1],
                Neighbor {
                    to: parents[0],
                    label: "partner".to_string(),
                    direction: "undirected".to_string(),
                    edge_type: "partner".to_string(),
                },
            );
        }

        for child in family.child_links {
            for parent in &parents {
                graph
                    .parents_by_child
                    .entry(child.child_id)
                    .or_default()
                    .push(*parent);
                graph
                    .children_by_parent
                    .entry(*parent)
                    .or_default()
                    .push(child.child_id);

                add_neighbor(
                    &mut graph.neighbors,
                    *parent,
                    Neighbor {
                        to: child.child_id,
                        label: "parent_of".to_string(),
                        direction: "outbound".to_string(),
                        edge_type: "parent_of".to_string(),
                    },
                );
                add_neighbor(
                    &mut graph.neighbors,
                    child.child_id,
                    Neighbor {
                        to: *parent,
                        label: "child_of".to_string(),
                        direction: "inbound".to_string(),
                        edge_type: "child_of".to_string(),
                    },
                );
            }
        }
    }

    for parents in graph.parents_by_child.values_mut() {
        parents.sort();
        parents.dedup();
    }
    for children in graph.children_by_parent.values_mut() {
        children.sort();
        children.dedup();
    }

    Ok(graph)
}

fn add_neighbor(
    neighbors: &mut HashMap<EntityId, Vec<Neighbor>>,
    from: EntityId,
    neighbor: Neighbor,
) {
    let entry = neighbors.entry(from).or_default();
    if entry
        .iter()
        .any(|existing| existing.to == neighbor.to && existing.label == neighbor.label)
    {
        return;
    }
    entry.push(neighbor);
}

async fn collect_summaries_for_ancestors(
    state: &AppState,
    graph: &FamilyGraph,
    root_id: EntityId,
    generations: u32,
) -> Result<HashMap<EntityId, PersonSummary>, ApiError> {
    let mut ids = BTreeSet::new();
    ids.insert(root_id);

    let mut frontier = vec![root_id];
    for _ in 0..generations {
        let mut next = Vec::new();
        for current in frontier {
            for parent in graph
                .parents_by_child
                .get(&current)
                .cloned()
                .unwrap_or_default()
            {
                if ids.insert(parent) {
                    next.push(parent);
                }
            }
        }
        if next.is_empty() {
            break;
        }
        frontier = next;
    }

    collect_summaries_for_ids(state, ids.into_iter()).await
}

async fn collect_summaries_for_descendants(
    state: &AppState,
    graph: &FamilyGraph,
    root_id: EntityId,
    generations: u32,
) -> Result<HashMap<EntityId, PersonSummary>, ApiError> {
    let mut ids = BTreeSet::new();
    ids.insert(root_id);

    let mut frontier = vec![root_id];
    for _ in 0..generations {
        let mut next = Vec::new();
        for current in frontier {
            for child in graph
                .children_by_parent
                .get(&current)
                .cloned()
                .unwrap_or_default()
            {
                if ids.insert(child) {
                    next.push(child);
                }
            }
        }
        if next.is_empty() {
            break;
        }
        frontier = next;
    }

    collect_summaries_for_ids(state, ids.into_iter()).await
}

async fn collect_summaries_for_ids(
    state: &AppState,
    ids: impl Iterator<Item = EntityId>,
) -> Result<HashMap<EntityId, PersonSummary>, ApiError> {
    let mut summaries = HashMap::new();
    for id in ids {
        let person = match state.storage.get_person(id).await {
            Ok(person) => person,
            Err(_) => continue,
        };
        let events = state
            .storage
            .list_events_for_person(id)
            .await
            .unwrap_or_default();
        let (birth_year, death_year) = event_years(&events);

        summaries.insert(
            id,
            PersonSummary {
                display_name: person_display_name(&person),
                birth_year,
                death_year,
            },
        );
    }
    Ok(summaries)
}

fn build_ancestor_tree(
    graph: &FamilyGraph,
    summaries: &HashMap<EntityId, PersonSummary>,
    person_id: EntityId,
    generations_left: u32,
    lineage_stack: &mut HashSet<EntityId>,
) -> AncestorTreeNode {
    let summary = summaries
        .get(&person_id)
        .cloned()
        .unwrap_or_else(|| PersonSummary {
            display_name: format!("Unknown ({person_id})"),
            birth_year: None,
            death_year: None,
        });

    if generations_left == 0 || !lineage_stack.insert(person_id) {
        return AncestorTreeNode {
            person_id,
            display_name: summary.display_name,
            birth_year: summary.birth_year,
            death_year: summary.death_year,
            confidence: 1.0,
            father: None,
            mother: None,
        };
    }

    let (father_id, mother_id) = select_parents(graph, summaries, person_id);

    let father = father_id.map(|fid| {
        Box::new(build_ancestor_tree(
            graph,
            summaries,
            fid,
            generations_left - 1,
            lineage_stack,
        ))
    });
    let mother = mother_id.map(|mid| {
        Box::new(build_ancestor_tree(
            graph,
            summaries,
            mid,
            generations_left - 1,
            lineage_stack,
        ))
    });

    lineage_stack.remove(&person_id);

    AncestorTreeNode {
        person_id,
        display_name: summary.display_name,
        birth_year: summary.birth_year,
        death_year: summary.death_year,
        confidence: 1.0,
        father,
        mother,
    }
}

fn build_descendant_tree(
    graph: &FamilyGraph,
    summaries: &HashMap<EntityId, PersonSummary>,
    person_id: EntityId,
    generations_left: u32,
    lineage_stack: &mut HashSet<EntityId>,
) -> DescendantTreeNode {
    let summary = summaries
        .get(&person_id)
        .cloned()
        .unwrap_or_else(|| PersonSummary {
            display_name: format!("Unknown ({person_id})"),
            birth_year: None,
            death_year: None,
        });

    if generations_left == 0 || !lineage_stack.insert(person_id) {
        return DescendantTreeNode {
            person_id,
            display_name: summary.display_name,
            birth_year: summary.birth_year,
            death_year: summary.death_year,
            confidence: 1.0,
            children: Vec::new(),
        };
    }

    let mut children = graph
        .children_by_parent
        .get(&person_id)
        .cloned()
        .unwrap_or_default();
    children.sort();
    children.dedup();

    let child_nodes = children
        .into_iter()
        .map(|child_id| {
            build_descendant_tree(
                graph,
                summaries,
                child_id,
                generations_left - 1,
                lineage_stack,
            )
        })
        .collect::<Vec<_>>();

    lineage_stack.remove(&person_id);

    DescendantTreeNode {
        person_id,
        display_name: summary.display_name,
        birth_year: summary.birth_year,
        death_year: summary.death_year,
        confidence: 1.0,
        children: child_nodes,
    }
}

fn select_parents(
    graph: &FamilyGraph,
    summaries: &HashMap<EntityId, PersonSummary>,
    person_id: EntityId,
) -> (Option<EntityId>, Option<EntityId>) {
    let mut parents = graph
        .parents_by_child
        .get(&person_id)
        .cloned()
        .unwrap_or_default();

    parents.sort();
    parents.dedup();

    if parents.is_empty() {
        return (None, None);
    }

    let mut father = None;
    let mut mother = None;

    for parent_id in &parents {
        if let Some(summary) = summaries.get(parent_id) {
            if summary.display_name.is_empty() {
                continue;
            }
        }
    }

    for parent_id in &parents {
        let gender = infer_gender_from_summary(*parent_id, summaries);
        match gender {
            Some(Gender::Male) if father.is_none() => father = Some(*parent_id),
            Some(Gender::Female) if mother.is_none() => mother = Some(*parent_id),
            _ => {}
        }
    }

    for parent_id in parents {
        if father.is_none() {
            father = Some(parent_id);
            continue;
        }
        if mother.is_none() && Some(parent_id) != father {
            mother = Some(parent_id);
        }
    }

    (father, mother)
}

fn infer_gender_from_summary(
    _person_id: EntityId,
    _summaries: &HashMap<EntityId, PersonSummary>,
) -> Option<Gender> {
    None
}

fn shortest_path(graph: &FamilyGraph, from_id: EntityId, to_id: EntityId) -> Option<Vec<PathStep>> {
    let mut queue = VecDeque::new();
    let mut visited = HashSet::new();
    let mut prev: HashMap<EntityId, (EntityId, Neighbor)> = HashMap::new();

    queue.push_back(from_id);
    visited.insert(from_id);

    while let Some(current) = queue.pop_front() {
        if current == to_id {
            break;
        }

        for neighbor in graph.neighbors.get(&current).cloned().unwrap_or_default() {
            if visited.insert(neighbor.to) {
                prev.insert(neighbor.to, (current, neighbor.clone()));
                queue.push_back(neighbor.to);
            }
        }
    }

    if !visited.contains(&to_id) {
        return None;
    }

    let mut nodes = vec![to_id];
    let mut cursor = to_id;
    while cursor != from_id {
        let (parent, _) = prev.get(&cursor)?.clone();
        nodes.push(parent);
        cursor = parent;
    }
    nodes.reverse();

    let mut steps = Vec::with_capacity(nodes.len());
    steps.push(PathStep {
        person_id: nodes[0],
        relationship_label: "self".to_string(),
        direction: "none".to_string(),
    });

    for pair in nodes.windows(2) {
        let to = pair[1];
        let (_, meta) = prev.get(&to)?.clone();
        steps.push(PathStep {
            person_id: to,
            relationship_label: meta.label,
            direction: meta.direction,
        });
    }

    Some(steps)
}

fn person_display_name(person: &Person) -> String {
    let primary = person.primary_name();
    let surname = primary
        .surnames
        .iter()
        .map(|item| item.value.as_str())
        .collect::<Vec<_>>()
        .join(" ");

    if surname.is_empty() {
        primary.given_names
    } else if primary.given_names.is_empty() {
        surname
    } else {
        format!("{} {}", primary.given_names, surname)
    }
}

fn event_years(events: &[Event]) -> (Option<i32>, Option<i32>) {
    let mut birth_year = None;
    let mut death_year = None;

    for event in events {
        let year = match event.date.as_ref() {
            Some(DateValue::Exact { date, .. })
            | Some(DateValue::Before { date, .. })
            | Some(DateValue::After { date, .. })
            | Some(DateValue::About { date, .. })
            | Some(DateValue::Tolerance { date, .. }) => Some(date.year),
            Some(DateValue::Range { from, .. }) => Some(from.year),
            Some(DateValue::Quarter { year, .. }) => Some(*year),
            Some(DateValue::Textual { .. }) | None => None,
        };

        match event.event_type {
            EventType::Birth if birth_year.is_none() => birth_year = year,
            EventType::Death if death_year.is_none() => death_year = year,
            _ => {}
        }
    }

    (birth_year, death_year)
}
