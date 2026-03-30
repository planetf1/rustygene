use crate::types::EntityId;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Default, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchResult {
    #[default]
    Found,
    NotFound,
    PartiallyFound,
    Inconclusive,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResearchLogEntry {
    pub id: EntityId,
    pub date: DateTime<Utc>,
    pub objective: String,
    pub repository: Option<EntityId>,
    pub repository_name: Option<String>,
    #[serde(default)]
    pub search_terms: Vec<String>,
    pub source_searched: Option<EntityId>,
    pub result: SearchResult,
    pub findings: Option<String>,
    #[serde(default)]
    pub citations_created: Vec<EntityId>,
    pub next_steps: Option<String>,
    #[serde(default)]
    pub person_refs: Vec<EntityId>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_round_trip_research_log_entry() {
        let entry = ResearchLogEntry {
            id: EntityId::new(),
            date: Utc::now(),
            objective: "Find birth record for John Smith b. ~1850".to_string(),
            repository: Some(EntityId::new()),
            repository_name: Some("The National Archives".to_string()),
            search_terms: vec!["John Smith".to_string(), "1850".to_string()],
            source_searched: Some(EntityId::new()),
            result: SearchResult::PartiallyFound,
            findings: Some("Found likely census entries, no baptism yet.".to_string()),
            citations_created: vec![EntityId::new(), EntityId::new()],
            next_steps: Some("Search parish records in adjacent counties.".to_string()),
            person_refs: vec![EntityId::new()],
            tags: vec!["census".to_string(), "todo".to_string()],
        };

        let json = serde_json::to_string(&entry).expect("serialize research log entry");
        let round_trip: ResearchLogEntry =
            serde_json::from_str(&json).expect("deserialize research log entry");

        assert_eq!(round_trip, entry);
    }

    #[test]
    fn search_result_serializes_as_snake_case() {
        let result = SearchResult::NotFound;
        let json = serde_json::to_string(&result).expect("serialize search result");
        assert_eq!(json, "\"not_found\"");

        let back: SearchResult = serde_json::from_str(&json).expect("deserialize search result");
        assert_eq!(back, SearchResult::NotFound);
    }
}
