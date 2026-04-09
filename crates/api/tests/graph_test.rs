use std::sync::Arc;
use std::time::Duration;

mod common;


use reqwest::StatusCode;
use rusqlite::Connection;
use rustygene_api::{start_server, AppState};
use rustygene_core::family::{ChildLink, Family, PartnerLink};
use rustygene_core::person::{Person, PersonName, Surname};
use rustygene_core::types::{EntityId, Gender};
use rustygene_storage::run_migrations;
use rustygene_storage::sqlite_impl::SqliteBackend;
use rustygene_storage::Storage;
use serde::Deserialize;

fn in_memory_backend() -> Arc<SqliteBackend> {
    let mut conn = Connection::open_in_memory().expect("open in-memory sqlite connection");
    run_migrations(&mut conn).expect("run sqlite migrations");
    Arc::new(SqliteBackend::new(conn))
}

fn person_with_name(id: EntityId, given: &str, surname: &str) -> Person {
    Person {
        id,
        names: vec![PersonName {
            given_names: given.to_string(),
            surnames: vec![Surname {
                value: surname.to_string(),
                origin_type: Default::default(),
                connector: None,
            }],
            ..Default::default()
        }],
        gender: Gender::Unknown,
        living: false,
        private: false,
        original_xref: None,
        _raw_gedcom: Default::default(),
    }
}

#[derive(Debug, Deserialize)]
struct ImportAcceptedResponse {
    job_id: String,
}

#[derive(Debug, Deserialize)]
struct ImportJobStatusResponse {
    status: String,
    errors: Vec<String>,
}

async fn wait_for_import_completion(client: &reqwest::Client, base_url: &str, job_id: &str) {
    for _ in 0..300 {
        let response = client
            .get(format!("{base_url}/api/v1/import/{job_id}"))
            .send()
            .await
            .expect("poll import status");
        assert_eq!(response.status(), StatusCode::OK);

        let status: ImportJobStatusResponse = response.json().await.expect("parse import status");
        if status.status == "completed" {
            return;
        }

        if status.status == "failed" {
            panic!("import failed: {:?}", status.errors);
        }

        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    panic!("import did not complete in poll budget");
}

async fn start_server_with_kennedy_data() -> (rustygene_api::ServerHandle, String, reqwest::Client)
{
    let backend = in_memory_backend();
    let state = AppState::with_default_cors_sqlite(backend, 0).expect("build app state");
    let server = start_server(state, 0).await.expect("start server");

    let client = reqwest::Client::new();
    let base_url = format!("http://{}", server.local_addr);

    let content = include_str!("../../../testdata/gedcom/kennedy.ged");
    let part = reqwest::multipart::Part::text(content.to_string()).file_name("kennedy.ged");
    let form = reqwest::multipart::Form::new()
        .text("format", "gedcom")
        .part("file", part);

    let accepted = client
        .post(format!("{base_url}/api/v1/import"))
        .multipart(form)
        .send()
        .await
        .expect("submit import");
    assert_eq!(accepted.status(), StatusCode::ACCEPTED);

    let accepted_body: ImportAcceptedResponse = accepted.json().await.expect("accepted response");
    wait_for_import_completion(&client, &base_url, &accepted_body.job_id).await;

    (server, base_url, client)
}

async fn find_person_id_by_query(client: &reqwest::Client, base_url: &str, query: &str) -> String {
    let response = client
        .get(format!("{base_url}/api/v1/search?q={query}&type=person"))
        .send()
        .await
        .expect("search request");
    assert_eq!(response.status(), StatusCode::OK);

    let body: serde_json::Value = response.json().await.expect("parse search response");
    let results = body
        .get("results")
        .and_then(serde_json::Value::as_array)
        .expect("results array");

    let needle = query.replace("%20", " ").to_ascii_lowercase();

    results
        .iter()
        .find(|result| {
            result
                .get("display_name")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|name| name.to_ascii_lowercase().contains(&needle))
        })
        .or_else(|| results.first())
        .and_then(|result| result.get("entity_id").and_then(serde_json::Value::as_str))
        .expect("at least one person match")
        .to_string()
}

#[tokio::test]
async fn graph_ancestors_for_kennedy_supports_four_generations() {
    let (server, base_url, client) = start_server_with_kennedy_data().await;

    let jfk_id = find_person_id_by_query(&client, &base_url, "John%20Fitzgerald%20Kennedy").await;

    let response = client
        .get(format!(
            "{base_url}/api/v1/graph/ancestors/{jfk_id}?generations=4"
        ))
        .send()
        .await
        .expect("ancestors request");
    assert_eq!(response.status(), StatusCode::OK);

    let body: serde_json::Value = response.json().await.expect("parse ancestors response");
    assert_eq!(
        body.get("person_id").and_then(serde_json::Value::as_str),
        Some(jfk_id.as_str())
    );

    fn max_depth(node: &serde_json::Value) -> usize {
        let father_depth = node
            .get("father")
            .filter(|value| !value.is_null())
            .map(max_depth)
            .unwrap_or(0);
        let mother_depth = node
            .get("mother")
            .filter(|value| !value.is_null())
            .map(max_depth)
            .unwrap_or(0);
        1 + father_depth.max(mother_depth)
    }

    let depth = max_depth(&body);
    assert!(
        depth >= 2,
        "expected at least root + one ancestor generation"
    );
    assert!(
        depth <= 5,
        "tree depth should not exceed root + requested generations"
    );

    server.shutdown().await.expect("shutdown server");
}

#[tokio::test]
async fn graph_network_radius_two_returns_nodes_and_edges() {
    let (server, base_url, client) = start_server_with_kennedy_data().await;

    let jfk_id = find_person_id_by_query(&client, &base_url, "John%20Fitzgerald%20Kennedy").await;

    let response = client
        .get(format!("{base_url}/api/v1/graph/network/{jfk_id}?radius=2"))
        .send()
        .await
        .expect("network request");
    assert_eq!(response.status(), StatusCode::OK);

    let body: serde_json::Value = response.json().await.expect("parse network response");
    let nodes = body
        .get("nodes")
        .and_then(serde_json::Value::as_array)
        .expect("nodes array");
    let edges = body
        .get("edges")
        .and_then(serde_json::Value::as_array)
        .expect("edges array");

    assert!(!nodes.is_empty(), "expected graph nodes");
    assert!(!edges.is_empty(), "expected graph edges");
    assert!(nodes.iter().any(|node| {
        node.get("id").and_then(serde_json::Value::as_str) == Some(jfk_id.as_str())
    }));

    server.shutdown().await.expect("shutdown server");
}

#[tokio::test]
async fn graph_generations_over_ten_returns_bad_request() {
    let (server, base_url, client) = start_server_with_kennedy_data().await;

    let jfk_id = find_person_id_by_query(&client, &base_url, "John%20Fitzgerald%20Kennedy").await;

    let response = client
        .get(format!(
            "{base_url}/api/v1/graph/ancestors/{jfk_id}?generations=11"
        ))
        .send()
        .await
        .expect("ancestors request");
    common::assert_api_error(response, StatusCode::BAD_REQUEST, "validation").await;

    server.shutdown().await.expect("shutdown server");
}

#[tokio::test]
async fn graph_descendants_and_path_work_for_cousins() {
    let backend = in_memory_backend();

    // Grandparents
    let gpa = EntityId::new();
    let gma = EntityId::new();
    // Parents
    let parent_a = EntityId::new();
    let parent_b = EntityId::new();
    let spouse_a = EntityId::new();
    let spouse_b = EntityId::new();
    // Cousins
    let cousin_1 = EntityId::new();
    let cousin_2 = EntityId::new();

    for (id, given) in [
        (gpa, "Grand"),
        (gma, "Parent"),
        (parent_a, "ParentA"),
        (parent_b, "ParentB"),
        (spouse_a, "SpouseA"),
        (spouse_b, "SpouseB"),
        (cousin_1, "CousinOne"),
        (cousin_2, "CousinTwo"),
    ] {
        backend
            .create_person(&person_with_name(id, given, "Family"))
            .await
            .expect("create person");
    }

    let grand_family = Family {
        id: EntityId::new(),
        partner1_id: Some(gpa),
        partner2_id: Some(gma),
        partner_link: PartnerLink::Married,
        couple_relationship: None,
        child_links: vec![
            ChildLink {
                child_id: parent_a,
                lineage_type: Default::default(),
            },
            ChildLink {
                child_id: parent_b,
                lineage_type: Default::default(),
            },
        ],
        original_xref: None,
        _raw_gedcom: Default::default(),
    };

    let family_a = Family {
        id: EntityId::new(),
        partner1_id: Some(parent_a),
        partner2_id: Some(spouse_a),
        partner_link: PartnerLink::Married,
        couple_relationship: None,
        child_links: vec![ChildLink {
            child_id: cousin_1,
            lineage_type: Default::default(),
        }],
        original_xref: None,
        _raw_gedcom: Default::default(),
    };

    let family_b = Family {
        id: EntityId::new(),
        partner1_id: Some(parent_b),
        partner2_id: Some(spouse_b),
        partner_link: PartnerLink::Married,
        couple_relationship: None,
        child_links: vec![ChildLink {
            child_id: cousin_2,
            lineage_type: Default::default(),
        }],
        original_xref: None,
        _raw_gedcom: Default::default(),
    };

    backend
        .create_family(&grand_family)
        .await
        .expect("create grand family");
    backend
        .create_family(&family_a)
        .await
        .expect("create family a");
    backend
        .create_family(&family_b)
        .await
        .expect("create family b");

    let state = AppState::with_default_cors(backend, 0).expect("build app state");
    let server = start_server(state, 0).await.expect("start server");
    let client = reqwest::Client::new();
    let base_url = format!("http://{}", server.local_addr);

    let descendants = client
        .get(format!(
            "{base_url}/api/v1/graph/descendants/{gpa}?generations=3"
        ))
        .send()
        .await
        .expect("descendants request");
    assert_eq!(descendants.status(), StatusCode::OK);
    let descendants_body: serde_json::Value = descendants.json().await.expect("descendants json");
    let root_children = descendants_body
        .get("children")
        .and_then(serde_json::Value::as_array)
        .expect("children array");
    assert!(
        !root_children.is_empty(),
        "expected descendants from grandparent"
    );

    let path = client
        .get(format!(
            "{base_url}/api/v1/graph/path/{cousin_1}/{cousin_2}"
        ))
        .send()
        .await
        .expect("path request");
    assert_eq!(path.status(), StatusCode::OK);
    let path_body: serde_json::Value = path.json().await.expect("path json");
    let steps = path_body
        .get("path")
        .and_then(serde_json::Value::as_array)
        .expect("path steps array");
    assert!(
        steps.len() >= 3,
        "expected multi-step cousin relationship path"
    );
    let kinship_name = path_body
        .get("kinship_name")
        .and_then(serde_json::Value::as_str)
        .expect("kinship_name string");
    assert!(
        !kinship_name.is_empty(),
        "expected kinship_name in response"
    );

    server.shutdown().await.expect("shutdown server");
}

#[tokio::test]
async fn graph_pedigree_collapse_populates_collapsed_from() {
    let backend = in_memory_backend();

    let common = EntityId::new();
    let root = EntityId::new();
    let father = EntityId::new();
    let mother = EntityId::new();

    for (id, given) in [
        (common, "Common"),
        (father, "Father"),
        (mother, "Mother"),
        (root, "Root"),
    ] {
        backend
            .create_person(&person_with_name(id, given, "Collapse"))
            .await
            .expect("create person");
    }

    // Common ancestor is parent of both father and mother.
    backend
        .create_family(&Family {
            id: EntityId::new(),
            partner1_id: Some(common),
            partner2_id: None,
            partner_link: PartnerLink::Unknown,
            couple_relationship: None,
            child_links: vec![
                ChildLink {
                    child_id: father,
                    lineage_type: Default::default(),
                },
                ChildLink {
                    child_id: mother,
                    lineage_type: Default::default(),
                },
            ],
            original_xref: None,
            _raw_gedcom: Default::default(),
        })
        .await
        .expect("create common ancestor family");

    backend
        .create_family(&Family {
            id: EntityId::new(),
            partner1_id: Some(father),
            partner2_id: Some(mother),
            partner_link: PartnerLink::Married,
            couple_relationship: None,
            child_links: vec![ChildLink {
                child_id: root,
                lineage_type: Default::default(),
            }],
            original_xref: None,
            _raw_gedcom: Default::default(),
        })
        .await
        .expect("create root family");

    let state = AppState::with_default_cors(backend, 0).expect("build app state");
    let server = start_server(state, 0).await.expect("start server");
    let client = reqwest::Client::new();
    let base_url = format!("http://{}", server.local_addr);

    let response = client
        .get(format!(
            "{base_url}/api/v1/graph/pedigree/{root}?generations=4&collapse_pedigree=true"
        ))
        .send()
        .await
        .expect("pedigree request");
    assert_eq!(response.status(), StatusCode::OK);

    let body: serde_json::Value = response.json().await.expect("pedigree json");
    let nodes = body
        .get("nodes")
        .and_then(serde_json::Value::as_array)
        .expect("nodes array");

    let common_node = nodes.iter().find(|node| {
        node.get("person_id").and_then(serde_json::Value::as_str)
            == Some(common.to_string().as_str())
    });
    let Some(common_node) = common_node else {
        panic!("expected common ancestor node in pedigree");
    };

    let collapsed = common_node
        .get("collapsed_from")
        .and_then(serde_json::Value::as_array)
        .expect("collapsed_from array");
    assert!(!collapsed.is_empty(), "expected pedigree collapse markers");

    server.shutdown().await.expect("shutdown server");
}
