use crate::evidence::CitationRef;
use crate::types::{ActorRef, EntityId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Default, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssertionStatus {
    #[default]
    Confirmed,
    Proposed,
    Disputed,
    Rejected,
}

#[derive(Default, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceType {
    #[default]
    Direct,
    Indirect,
    Negative,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Assertion<T> {
    pub id: EntityId,
    pub value: T,
    pub confidence: f64,
    pub status: AssertionStatus,
    pub evidence_type: EvidenceType,
    #[serde(default)]
    pub source_citations: Vec<CitationRef>,
    pub proposed_by: ActorRef,
    pub created_at: DateTime<Utc>,
    pub reviewed_at: Option<DateTime<Utc>>,
    pub reviewed_by: Option<ActorRef>,
}

#[derive(Default, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SandboxStatus {
    #[default]
    Active,
    Promoted,
    Discarded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sandbox {
    pub id: EntityId,
    pub name: String,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub parent_sandbox: Option<EntityId>,
    pub status: SandboxStatus,
}

#[derive(Debug, Serialize)]
struct IdempotencySourceCitation<'a> {
    citation_id: String,
    note: &'a Option<String>,
}

#[derive(Debug, Serialize)]
struct IdempotencyPayload<'a, T> {
    entity_id: String,
    field: &'a str,
    value: &'a T,
    source_citations: Vec<IdempotencySourceCitation<'a>>,
}

/// Compute deterministic idempotency key from assertion factual content only.
///
/// Included in hash:
/// - entity_id
/// - field
/// - value
/// - source_citations (sorted by citation_id + note)
///
/// Excluded by design:
/// - confidence, status, evidence type, proposed_by, reviewer fields, timestamps
pub fn compute_assertion_idempotency_key<T: Serialize>(
    entity_id: EntityId,
    field: &str,
    value: &T,
    source_citations: &[CitationRef],
) -> Result<String, serde_json::Error> {
    let mut sorted: Vec<&CitationRef> = source_citations.iter().collect();
    sorted.sort_by(|a, b| {
        let left = (
            a.citation_id.to_string(),
            a.note.clone().unwrap_or_default(),
        );
        let right = (
            b.citation_id.to_string(),
            b.note.clone().unwrap_or_default(),
        );
        left.cmp(&right)
    });

    let payload = IdempotencyPayload {
        entity_id: entity_id.to_string(),
        field,
        value,
        source_citations: sorted
            .iter()
            .map(|c| IdempotencySourceCitation {
                citation_id: c.citation_id.to_string(),
                note: &c.note,
            })
            .collect(),
    };

    let encoded = serde_json::to_vec(&payload)?;
    let digest = Sha256::digest(encoded);
    Ok(digest.iter().map(|b| format!("{b:02x}")).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ActorRef;

    #[test]
    fn idempotency_key_independent_of_citation_order() {
        let entity_id = EntityId::new();
        let c1 = CitationRef {
            citation_id: EntityId::new(),
            note: Some("n1".to_string()),
        };
        let c2 = CitationRef {
            citation_id: EntityId::new(),
            note: Some("n2".to_string()),
        };

        let key_a = compute_assertion_idempotency_key(
            entity_id,
            "birth_date",
            &"1850-05-01",
            &[c1.clone(), c2.clone()],
        )
        .expect("hash a");

        let key_b =
            compute_assertion_idempotency_key(entity_id, "birth_date", &"1850-05-01", &[c2, c1])
                .expect("hash b");

        assert_eq!(key_a, key_b);
    }

    #[test]
    fn idempotency_key_changes_when_value_changes() {
        let entity_id = EntityId::new();
        let key_a =
            compute_assertion_idempotency_key(entity_id, "name", &"John Doe", &[]).expect("hash a");
        let key_b =
            compute_assertion_idempotency_key(entity_id, "name", &"John Roe", &[]).expect("hash b");

        assert_ne!(key_a, key_b);
    }

    #[test]
    fn serde_round_trip_assertion_and_sandbox() {
        let assertion = Assertion {
            id: EntityId::new(),
            value: "John /Doe/".to_string(),
            confidence: 0.88,
            status: AssertionStatus::Proposed,
            evidence_type: EvidenceType::Indirect,
            source_citations: vec![CitationRef {
                citation_id: EntityId::new(),
                note: Some("census household".to_string()),
            }],
            proposed_by: ActorRef::Agent("discoverer".to_string()),
            created_at: Utc::now(),
            reviewed_at: None,
            reviewed_by: None,
        };

        let sandbox = Sandbox {
            id: EntityId::new(),
            name: "Richards patronymic hypothesis".to_string(),
            description: Some("Test alternate parent assignment".to_string()),
            created_at: Utc::now(),
            parent_sandbox: None,
            status: SandboxStatus::Active,
        };

        let assertion_json = serde_json::to_string(&assertion).expect("serialize assertion");
        let sandbox_json = serde_json::to_string(&sandbox).expect("serialize sandbox");

        let assertion_back: Assertion<String> =
            serde_json::from_str(&assertion_json).expect("deserialize assertion");
        let sandbox_back: Sandbox =
            serde_json::from_str(&sandbox_json).expect("deserialize sandbox");

        assert_eq!(assertion_back, assertion);
        assert_eq!(sandbox_back, sandbox);
    }
}
