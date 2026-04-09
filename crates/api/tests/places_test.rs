use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;

mod common;


use reqwest::StatusCode;
use rusqlite::Connection;
use rustygene_api::{start_server, AppState};
use rustygene_storage::run_migrations;
use rustygene_storage::sqlite_impl::SqliteBackend;

fn in_memory_backend() -> Arc<SqliteBackend> {
    let mut conn = Connection::open_in_memory().expect("open in-memory sqlite connection");
    run_migrations(&mut conn).expect("run sqlite migrations");
    Arc::new(SqliteBackend::new(conn))
}

#[tokio::test]
async fn places_crud_lifecycle() {
    let backend = in_memory_backend();
    let state = AppState::with_default_cors_sqlite(backend, 0).expect("build app state");
    let server = start_server(state, 0).await.expect("start server");
    assert_eq!(server.local_addr.ip(), IpAddr::V4(Ipv4Addr::LOCALHOST));

    let client = reqwest::Client::new();
    let base_url = format!("http://{}", server.local_addr);

    // CREATE a place
    let create_resp = client
        .post(format!("{base_url}/api/v1/places"))
        .json(&serde_json::json!({
            "place_type": "town",
            "names": [{"name": "Springfield", "language": null, "date_range": null}],
            "coordinates": [39.7817, -89.6501],
            "enclosed_by": [],
            "external_ids": []
        }))
        .send()
        .await
        .expect("create place");
    assert_eq!(create_resp.status(), StatusCode::CREATED);
    let create_body: serde_json::Value = create_resp.json().await.expect("create body");
    let place_id = create_body
        .get("id")
        .and_then(serde_json::Value::as_str)
        .expect("place id");

    // GET the created place
    let get_resp = client
        .get(format!("{base_url}/api/v1/places/{place_id}"))
        .send()
        .await
        .expect("get place");
    assert_eq!(get_resp.status(), StatusCode::OK);
    let place: serde_json::Value = get_resp.json().await.expect("place body");
    assert_eq!(place["id"].as_str(), Some(place_id));
    assert_eq!(place["place_type"].as_str(), Some("town"));
    assert_eq!(place["names"][0]["name"].as_str(), Some("Springfield"));

    // LIST places - should include the created one
    let list_resp = client
        .get(format!("{base_url}/api/v1/places"))
        .send()
        .await
        .expect("list places");
    assert_eq!(list_resp.status(), StatusCode::OK);
    let places: Vec<serde_json::Value> = list_resp.json().await.expect("places list");
    assert!(
        places.iter().any(|p| p["id"].as_str() == Some(place_id)),
        "created place must appear in list"
    );

    // UPDATE the place - change name and coordinates
    let update_resp = client
        .put(format!("{base_url}/api/v1/places/{place_id}"))
        .json(&serde_json::json!({
            "place_type": "city",
            "names": [{"name": "Springfield (updated)", "language": "en", "date_range": null}],
            "coordinates": [39.7817, -89.6501],
            "enclosed_by": [],
            "external_ids": [{"system": "geonames", "value": "4250542"}]
        }))
        .send()
        .await
        .expect("update place");
    assert_eq!(update_resp.status(), StatusCode::OK);

    // GET again to verify update
    let get2_resp = client
        .get(format!("{base_url}/api/v1/places/{place_id}"))
        .send()
        .await
        .expect("get updated place");
    assert_eq!(get2_resp.status(), StatusCode::OK);
    let updated: serde_json::Value = get2_resp.json().await.expect("updated body");
    assert_eq!(updated["place_type"].as_str(), Some("city"));
    assert_eq!(
        updated["names"][0]["name"].as_str(),
        Some("Springfield (updated)")
    );
    assert_eq!(
        updated["external_ids"][0]["system"].as_str(),
        Some("geonames")
    );

    // DELETE the place
    let delete_resp = client
        .delete(format!("{base_url}/api/v1/places/{place_id}"))
        .send()
        .await
        .expect("delete place");
    assert_eq!(delete_resp.status(), StatusCode::NO_CONTENT);

    // GET after delete must return 404
    let gone_resp = client
        .get(format!("{base_url}/api/v1/places/{place_id}"))
        .send()
        .await
        .expect("get deleted place");
    common::assert_api_error(gone_resp, StatusCode::NOT_FOUND, "not_found").await;

    server.shutdown().await.expect("shutdown server");
}

#[tokio::test]
async fn create_place_requires_at_least_one_name() {
    let backend = in_memory_backend();
    let state = AppState::with_default_cors_sqlite(backend, 0).expect("build app state");
    let server = start_server(state, 0).await.expect("start server");

    let client = reqwest::Client::new();
    let base_url = format!("http://{}", server.local_addr);

    let resp = client
        .post(format!("{base_url}/api/v1/places"))
        .json(&serde_json::json!({
            "place_type": "town",
            "names": []
        }))
        .send()
        .await
        .expect("create place no names");
    common::assert_api_error(resp, StatusCode::BAD_REQUEST, "validation").await;

    server.shutdown().await.expect("shutdown server");
}
