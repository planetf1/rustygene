use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use reqwest::StatusCode;
use rusqlite::Connection;
use rustygene_api::{AppState, start_server};
use rustygene_storage::run_migrations;
use rustygene_storage::sqlite_impl::SqliteBackend;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct ImportAcceptedResponse {
    job_id: String,
}

#[derive(Debug, Deserialize)]
struct ImportJobStatusResponse {
    status: String,
    errors: Vec<String>,
}

fn in_memory_backend() -> Arc<SqliteBackend> {
    let mut conn = Connection::open_in_memory().expect("open in-memory sqlite connection");
    run_migrations(&mut conn).expect("run sqlite migrations");
    Arc::new(SqliteBackend::new(conn))
}

fn parse_gedcom_counts_and_names(content: &str) -> (usize, usize, Vec<String>) {
    let mut indi_count = 0usize;
    let mut fam_count = 0usize;
    let mut names: Vec<String> = Vec::new();
    let mut in_indi_record = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("0 ") {
            in_indi_record = trimmed.contains(" INDI");
            if in_indi_record {
                indi_count += 1;
            } else if trimmed.contains(" FAM") {
                fam_count += 1;
            }
        } else if in_indi_record {
            if let Some(rest) = trimmed.strip_prefix("1 NAME ") {
                let cleaned = rest.replace('/', " ");
                let normalized = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
                if !normalized.is_empty() {
                    names.push(normalized);
                }
            }
        }
    }

    names.truncate(3);
    (indi_count, fam_count, names)
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

fn collect_property_harness_inputs() -> Vec<(String, String)> {
    let mut inputs = vec![(
        "kennedy.ged".to_string(),
        include_str!("../../../testdata/gedcom/kennedy.ged").to_string(),
    )];

    let user_gedcom_path = PathBuf::from("/Users/jonesn/Downloads/Nigel475GEDCOM7.ged");
    if user_gedcom_path.exists() {
        let bytes = std::fs::read(&user_gedcom_path)
            .unwrap_or_else(|e| panic!("read {:?}: {e}", user_gedcom_path));
        let content = String::from_utf8_lossy(&bytes).to_string();
        inputs.push((
            user_gedcom_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("user-upload.ged")
                .to_string(),
            content,
        ));
    }

    inputs
}

fn is_person_uuid_fallback(display_name: &str) -> bool {
    if !display_name.starts_with("Person ") {
        return false;
    }

    let candidate = display_name.trim_start_matches("Person ");
    uuid::Uuid::parse_str(candidate).is_ok()
}

#[tokio::test]
async fn gedcom_driven_api_property_harness() {
    let inputs = collect_property_harness_inputs();

    for (file_name, content) in inputs {
        let (expected_person_count, expected_family_count, search_terms) =
            parse_gedcom_counts_and_names(&content);

        let backend = in_memory_backend();
        let state = AppState::with_default_cors_sqlite(backend, 0).expect("build app state");
        let server = start_server(state, 0).await.expect("start server");
        let client = reqwest::Client::new();
        let base_url = format!("http://{}", server.local_addr);

        let part = reqwest::multipart::Part::text(content.clone()).file_name(file_name.clone());
        let form = reqwest::multipart::Form::new()
            .text("format", "gedcom")
            .part("file", part);

        let accepted = client
            .post(format!("{base_url}/api/v1/import"))
            .multipart(form)
            .send()
            .await
            .expect("submit import");
        assert_eq!(accepted.status(), StatusCode::ACCEPTED, "fixture {file_name}");

        let accepted_body: ImportAcceptedResponse = accepted.json().await.expect("accepted response");
        wait_for_import_completion(&client, &base_url, &accepted_body.job_id).await;

        // (1) Persons count property
        let persons_resp: serde_json::Value = client
            .get(format!("{base_url}/api/v1/persons?limit=1&offset=0"))
            .send()
            .await
            .expect("get persons")
            .json()
            .await
            .expect("parse persons");
        let api_person_total = persons_resp["total"]
            .as_u64()
            .expect("persons.total as number") as usize;
        assert_eq!(
            api_person_total, expected_person_count,
            "{file_name}: persons total should match INDI count"
        );

        // (2) Families count property
        let families_resp: serde_json::Value = client
            .get(format!("{base_url}/api/v1/families?limit=1&offset=0"))
            .send()
            .await
            .expect("get families")
            .json()
            .await
            .expect("parse families");
        let api_family_total = families_resp["total"]
            .as_u64()
            .expect("families.total as number") as usize;
        assert_eq!(
            api_family_total, expected_family_count,
            "{file_name}: families total should match FAM count"
        );

        // Pull all persons and all families for cross-entity invariants.
        let persons_full_resp: serde_json::Value = client
            .get(format!(
                "{base_url}/api/v1/persons?limit={}&offset=0",
                api_person_total.max(1)
            ))
            .send()
            .await
            .expect("get persons full")
            .json()
            .await
            .expect("parse persons full");
        let person_items = persons_full_resp["items"]
            .as_array()
            .expect("persons.items array");
        let person_ids: HashSet<String> = person_items
            .iter()
            .filter_map(|p| p["id"].as_str().map(ToString::to_string))
            .collect();

        let families_full_resp: serde_json::Value = client
            .get(format!(
                "{base_url}/api/v1/families?limit={}&offset=0",
                api_family_total.max(1)
            ))
            .send()
            .await
            .expect("get families full")
            .json()
            .await
            .expect("parse families full");
        let family_items = families_full_resp["items"]
            .as_array()
            .expect("families.items array");

        // (3) Every person in /families appears in /persons.
        for family in family_items {
            if let Some(p1) = family["partner1"].as_object() {
                if let Some(id) = p1.get("id").and_then(|v| v.as_str()) {
                    assert!(
                        person_ids.contains(id),
                        "{file_name}: partner1 id {id} must exist in /persons"
                    );
                }
            }
            if let Some(p2) = family["partner2"].as_object() {
                if let Some(id) = p2.get("id").and_then(|v| v.as_str()) {
                    assert!(
                        person_ids.contains(id),
                        "{file_name}: partner2 id {id} must exist in /persons"
                    );
                }
            }
            if let Some(children) = family["children"].as_array() {
                for child in children {
                    if let Some(id) = child["id"].as_str() {
                        assert!(
                            person_ids.contains(id),
                            "{file_name}: child id {id} must exist in /persons"
                        );
                    }
                }
            }
        }

        // (4) Search for first up-to-3 INDI names returns at least one result.
        for term in search_terms {
            let search_resp: serde_json::Value = client
            .get(format!("{base_url}/api/v1/search"))
            .query(&[("q", term.as_str())])
                .send()
                .await
                .expect("search by name")
                .json()
                .await
                .expect("parse search result");
            let results = search_resp["results"]
                .as_array()
                .expect("search.results array");
            assert!(
                !results.is_empty(),
                "{file_name}: search should return results for term '{term}'"
            );
        }

        // (5) No family detail child has display_name "Person {uuid}" fallback.
        for family in family_items {
            let Some(family_id) = family["id"].as_str() else {
                continue;
            };
            let detail: serde_json::Value = client
                .get(format!("{base_url}/api/v1/families/{family_id}"))
                .send()
                .await
                .expect("get family detail")
                .json()
                .await
                .expect("parse family detail");

            if let Some(children) = detail["children"].as_array() {
                for child in children {
                    let display_name = child["display_name"].as_str().unwrap_or("");
                    assert!(
                        !is_person_uuid_fallback(display_name),
                        "{file_name}: family {family_id} child fallback name leaked: {display_name}"
                    );
                }
            }
        }

        server.shutdown().await.expect("shutdown server");
    }
}
