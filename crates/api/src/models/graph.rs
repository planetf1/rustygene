use rustygene_core::types::EntityId;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct AncestorTreeNode {
    pub person_id: EntityId,
    pub display_name: String,
    pub birth_year: Option<i32>,
    pub death_year: Option<i32>,
    pub confidence: f32,
    pub father: Option<Box<AncestorTreeNode>>,
    pub mother: Option<Box<AncestorTreeNode>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DescendantTreeNode {
    pub person_id: EntityId,
    pub display_name: String,
    pub birth_year: Option<i32>,
    pub death_year: Option<i32>,
    pub confidence: f32,
    pub children: Vec<DescendantTreeNode>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PedigreeNode {
    pub person_id: EntityId,
    pub display_name: String,
    pub birth_year: Option<i32>,
    pub death_year: Option<i32>,
    pub confidence: f32,
    pub primary_position: String,
    pub collapsed_from: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PedigreeEdge {
    pub source: EntityId,
    pub target: EntityId,
    pub label: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PedigreeGraph {
    pub root_id: EntityId,
    pub nodes: Vec<PedigreeNode>,
    pub edges: Vec<PedigreeEdge>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PathStep {
    pub person_id: EntityId,
    pub relationship_label: String,
    pub direction: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct NetworkNode {
    pub id: EntityId,
    pub label: String,
    pub r#type: String,
    pub birth_year: Option<i32>,
    pub death_year: Option<i32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NetworkEdge {
    pub source: EntityId,
    pub target: EntityId,
    pub label: String,
    pub edge_type: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct NetworkGraph {
    pub nodes: Vec<NetworkNode>,
    pub edges: Vec<NetworkEdge>,
}
