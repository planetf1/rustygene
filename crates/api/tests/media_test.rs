use std::io::Cursor;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;

use reqwest::StatusCode;
use rusqlite::Connection;
use rustygene_api::{start_server, AppState};
use rustygene_core::evidence::Media;
use rustygene_core::person::Person;
use rustygene_storage::run_migrations;
use rustygene_storage::sqlite_impl::SqliteBackend;
use rustygene_storage::Storage;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::time::{sleep, Duration};

fn in_memory_backend() -> Arc<SqliteBackend> {
    let mut conn = Connection::open_in_memory().expect("open in-memory sqlite connection");
    run_migrations(&mut conn).expect("run sqlite migrations");
    Arc::new(SqliteBackend::new(conn))
}

fn sample_jpeg_bytes() -> Vec<u8> {
    let image = image::DynamicImage::ImageRgb8(image::RgbImage::from_fn(8, 6, |_x, _y| {
        image::Rgb([220, 10, 10])
    }));
    let mut buffer = Vec::new();
    image
        .write_to(&mut Cursor::new(&mut buffer), image::ImageFormat::Jpeg)
        .expect("encode test jpeg");
    buffer
}

#[derive(Debug, Deserialize)]
struct LinkResponse {
    linked: bool,
}

async fn upload_sample_media(client: &reqwest::Client, base_url: &str, file_name: &str) -> Media {
    let jpeg = sample_jpeg_bytes();
    let part = reqwest::multipart::Part::bytes(jpeg).file_name(file_name.to_string());
    let form = reqwest::multipart::Form::new().part("file", part);

    let upload_response = client
        .post(format!("{base_url}/api/v1/media"))
        .multipart(form)
        .send()
        .await
        .expect("upload media");
    assert_eq!(upload_response.status(), StatusCode::CREATED);
    upload_response.json().await.expect("parse media response")
}

#[tokio::test]
async fn media_upload_dedup_link_and_thumbnail_workflow() {
    let backend = in_memory_backend();
    let state = AppState::with_default_cors_sqlite(backend.clone(), 0).expect("build app state");
    let server = start_server(state, 0).await.expect("start server");
    assert_eq!(server.local_addr.ip(), IpAddr::V4(Ipv4Addr::LOCALHOST));

    let person = Person {
        id: rustygene_core::types::EntityId::new(),
        names: Vec::new(),
        gender: rustygene_core::types::Gender::Unknown,
        living: false,
        private: false,
        original_xref: None,
        _raw_gedcom: std::collections::BTreeMap::new(),
    };
    backend.create_person(&person).await.expect("create person");

    let jpeg = sample_jpeg_bytes();
    let part = reqwest::multipart::Part::bytes(jpeg.clone()).file_name("photo.jpg");
    let form = reqwest::multipart::Form::new().part("file", part);

    let client = reqwest::Client::new();
    let upload_response = client
        .post(format!("http://{}/api/v1/media", server.local_addr))
        .multipart(form)
        .send()
        .await
        .expect("upload media");

    assert_eq!(upload_response.status(), StatusCode::CREATED);
    let media: Media = upload_response.json().await.expect("parse media response");

    assert_eq!(
        media.dimensions_px.as_ref().map(|d| (d.width, d.height)),
        Some((8, 6))
    );

    let digest_hex = format!("{:x}", Sha256::digest(&jpeg));
    let expected_hash = format!("sha256:{digest_hex}");
    assert_eq!(media.content_hash, expected_hash);
    assert!(
        media
            .file_path
            .contains(&format!("/media/{}/{}", &digest_hex[..2], digest_hex)),
        "file path should follow content-addressed layout"
    );

    let duplicate_part = reqwest::multipart::Part::bytes(jpeg).file_name("photo-copy.jpg");
    let duplicate_form = reqwest::multipart::Form::new().part("file", duplicate_part);
    let duplicate_response = client
        .post(format!("http://{}/api/v1/media", server.local_addr))
        .multipart(duplicate_form)
        .send()
        .await
        .expect("upload duplicate media");

    assert_eq!(duplicate_response.status(), StatusCode::OK);
    let duplicate_media: Media = duplicate_response
        .json()
        .await
        .expect("parse duplicate response");
    assert_eq!(duplicate_media.id, media.id);

    let thumb_response = client
        .get(format!(
            "http://{}/api/v1/media/{}/thumbnail",
            server.local_addr, media.id
        ))
        .send()
        .await
        .expect("get thumbnail");
    assert_eq!(thumb_response.status(), StatusCode::OK);
    assert_eq!(
        thumb_response
            .headers()
            .get("content-type")
            .and_then(|h| h.to_str().ok()),
        Some("image/jpeg")
    );

    let link_response = client
        .post(format!(
            "http://{}/api/v1/entities/{}/media/{}",
            server.local_addr, person.id, media.id
        ))
        .send()
        .await
        .expect("link media to entity");
    assert_eq!(link_response.status(), StatusCode::CREATED);
    let linked: LinkResponse = link_response.json().await.expect("parse link response");
    assert!(linked.linked);

    let list_response = client
        .get(format!(
            "http://{}/api/v1/entities/{}/media",
            server.local_addr, person.id
        ))
        .send()
        .await
        .expect("list entity media");
    assert_eq!(list_response.status(), StatusCode::OK);
    let linked_media: Vec<Media> = list_response.json().await.expect("parse list media");
    assert_eq!(linked_media.len(), 1);
    assert_eq!(linked_media[0].id, media.id);

    let unlink_response = client
        .delete(format!(
            "http://{}/api/v1/entities/{}/media/{}",
            server.local_addr, person.id, media.id
        ))
        .send()
        .await
        .expect("unlink media");
    assert_eq!(unlink_response.status(), StatusCode::OK);
    let unlinked: LinkResponse = unlink_response.json().await.expect("parse unlink response");
    assert!(!unlinked.linked);

    let list_after_unlink = client
        .get(format!(
            "http://{}/api/v1/entities/{}/media",
            server.local_addr, person.id
        ))
        .send()
        .await
        .expect("list entity media after unlink");
    let linked_media_after: Vec<Media> = list_after_unlink
        .json()
        .await
        .expect("parse media list after unlink");
    assert!(linked_media_after.is_empty());

    server.shutdown().await.expect("shutdown server");
}

#[tokio::test]
async fn media_upload_unsupported_type_returns_415() {
    let backend = in_memory_backend();
    let state = AppState::with_default_cors_sqlite(backend, 0).expect("build app state");
    let server = start_server(state, 0).await.expect("start server");

    let part = reqwest::multipart::Part::bytes(b"plain-text".to_vec()).file_name("note.txt");
    let form = reqwest::multipart::Form::new().part("file", part);

    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://{}/api/v1/media", server.local_addr))
        .multipart(form)
        .send()
        .await
        .expect("upload unsupported media");

    assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);

    server.shutdown().await.expect("shutdown server");
}

#[tokio::test]
async fn media_album_and_tag_actions_persist_and_filter() {
    let backend = in_memory_backend();
    let state = AppState::with_default_cors_sqlite(backend.clone(), 0).expect("build app state");
    let server = start_server(state, 0).await.expect("start server");

    let jpeg = sample_jpeg_bytes();
    let part = reqwest::multipart::Part::bytes(jpeg).file_name("album.jpg");
    let form = reqwest::multipart::Form::new().part("file", part);
    let client = reqwest::Client::new();

    let upload_response = client
        .post(format!("http://{}/api/v1/media", server.local_addr))
        .multipart(form)
        .send()
        .await
        .expect("upload media");
    assert_eq!(upload_response.status(), StatusCode::CREATED);
    let media: Media = upload_response.json().await.expect("parse media response");

    let add_tag = client
        .post(format!(
            "http://{}/api/v1/media/{}/tags",
            server.local_addr, media.id
        ))
        .json(&serde_json::json!({ "tag": "passport" }))
        .send()
        .await
        .expect("add tag");
    assert_eq!(add_tag.status(), StatusCode::CREATED);

    let add_album = client
        .post(format!(
            "http://{}/api/v1/media/albums/family/items",
            server.local_addr
        ))
        .json(&serde_json::json!({ "media_ids": [media.id] }))
        .send()
        .await
        .expect("add media to album");
    assert_eq!(add_album.status(), StatusCode::OK);

    let filtered = client
        .get(format!(
            "http://{}/api/v1/media?album=family",
            server.local_addr
        ))
        .send()
        .await
        .expect("filter by album");
    assert_eq!(filtered.status(), StatusCode::OK);
    let payload: serde_json::Value = filtered.json().await.expect("parse filtered media");

    let rows = payload.as_array().expect("media response should be array");
    assert_eq!(rows.len(), 1);
    let media_id_text = media.id.to_string();
    assert_eq!(
        rows[0].get("id").and_then(|v| v.as_str()),
        Some(media_id_text.as_str())
    );

    let tags = rows[0]
        .get("tags")
        .and_then(|v| v.as_array())
        .expect("tags array should exist");
    assert!(tags.iter().any(|v| v.as_str() == Some("passport")));

    let albums = rows[0]
        .get("albums")
        .and_then(|v| v.as_array())
        .expect("albums array should exist");
    assert!(albums.iter().any(|v| v.as_str() == Some("family")));

    server.shutdown().await.expect("shutdown server");
}

#[tokio::test]
async fn media_text_update_and_links_endpoint_roundtrip() {
    let backend = in_memory_backend();
    let state = AppState::with_default_cors_sqlite(backend, 0).expect("build app state");
    let server = start_server(state, 0).await.expect("start server");

    let client = reqwest::Client::new();
    let base_url = format!("http://{}", server.local_addr);

    let person_response = client
        .post(format!("{base_url}/api/v1/persons"))
        .json(&serde_json::json!({
            "given_names": ["Mary"],
            "surnames": [{"value": "Johnson", "origin_type": "patrilineal", "connector": null}],
            "gender": "female"
        }))
        .send()
        .await
        .expect("create person");
    assert_eq!(person_response.status(), StatusCode::CREATED);
    let person_body: serde_json::Value = person_response.json().await.expect("person body");
    let person_id = person_body
        .get("id")
        .and_then(serde_json::Value::as_str)
        .expect("person id")
        .to_string();

    let media = upload_sample_media(&client, &base_url, "mary-johnson-passport.jpg").await;

    let link_response = client
        .post(format!(
            "{base_url}/api/v1/entities/{}/media/{}",
            person_id, media.id
        ))
        .send()
        .await
        .expect("link media to person");
    assert_eq!(link_response.status(), StatusCode::CREATED);

    let update_response = client
        .put(format!("{base_url}/api/v1/media/{}/text", media.id))
        .json(&serde_json::json!({
            "text": "Passport record for Mary Johnson"
        }))
        .send()
        .await
        .expect("update OCR text");
    assert_eq!(update_response.status(), StatusCode::OK);

    let media_detail = client
        .get(format!("{base_url}/api/v1/media/{}", media.id))
        .send()
        .await
        .expect("get media detail");
    assert_eq!(media_detail.status(), StatusCode::OK);
    let media_body: serde_json::Value = media_detail.json().await.expect("media detail body");
    assert_eq!(
        media_body
            .get("ocr_text")
            .and_then(serde_json::Value::as_str),
        Some("Passport record for Mary Johnson")
    );

    let links_response = client
        .get(format!("{base_url}/api/v1/media/{}/links", media.id))
        .send()
        .await
        .expect("get media links");
    assert_eq!(links_response.status(), StatusCode::OK);
    let links_body: serde_json::Value = links_response.json().await.expect("links body");
    let links = links_body.as_array().expect("links array");
    assert_eq!(links.len(), 1);
    assert_eq!(
        links[0]
            .get("entity_id")
            .and_then(serde_json::Value::as_str),
        Some(person_id.as_str())
    );
    assert_eq!(
        links[0]
            .get("entity_type")
            .and_then(serde_json::Value::as_str),
        Some("person")
    );
    assert_eq!(
        links[0]
            .get("display_name")
            .and_then(serde_json::Value::as_str),
        Some("Mary Johnson")
    );

    server.shutdown().await.expect("shutdown server");
}

#[tokio::test]
async fn media_extract_creates_filtered_suggested_link_proposals() {
    let backend = in_memory_backend();
    let state = AppState::with_default_cors_sqlite(backend, 0).expect("build app state");
    let server = start_server(state, 0).await.expect("start server");

    let client = reqwest::Client::new();
    let base_url = format!("http://{}", server.local_addr);

    let person_response = client
        .post(format!("{base_url}/api/v1/persons"))
        .json(&serde_json::json!({
            "given_names": ["Mary"],
            "surnames": [{"value": "Johnson", "origin_type": "patrilineal", "connector": null}],
            "gender": "female"
        }))
        .send()
        .await
        .expect("create person");
    assert_eq!(person_response.status(), StatusCode::CREATED);

    let media = upload_sample_media(&client, &base_url, "mary-johnson-passport.jpg").await;

    let extract_response = client
        .post(format!("{base_url}/api/v1/media/{}/extract", media.id))
        .send()
        .await
        .expect("trigger extract");
    assert_eq!(extract_response.status(), StatusCode::ACCEPTED);

    let mut media_detail_body = serde_json::Value::Null;
    let mut staging_body = serde_json::Value::Null;
    for _ in 0..200 {
        let media_detail = client
            .get(format!("{base_url}/api/v1/media/{}", media.id))
            .send()
            .await
            .expect("get media detail during extract");
        assert_eq!(media_detail.status(), StatusCode::OK);
        media_detail_body = media_detail.json().await.expect("media detail body");

        let staging_response = client
            .get(format!(
                "{base_url}/api/v1/staging?entity_id={}&entity_type=media&status=pending",
                media.id
            ))
            .send()
            .await
            .expect("list filtered staging proposals");
        assert_eq!(staging_response.status(), StatusCode::OK);
        staging_body = staging_response.json().await.expect("staging body");

        let has_text = media_detail_body
            .get("ocr_text")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|text| !text.trim().is_empty());
        let has_suggestion = staging_body.as_array().is_some_and(|rows| !rows.is_empty());

        if has_text && has_suggestion {
            break;
        }

        sleep(Duration::from_millis(20)).await;
    }

    assert!(
        media_detail_body
            .get("ocr_text")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|text| text.contains("Extracted text preview")),
        "expected generated OCR text, got: {media_detail_body:?}"
    );

    let proposals = staging_body.as_array().expect("staging array");
    assert!(
        proposals.iter().any(|proposal| {
            proposal
                .get("entity_type")
                .and_then(serde_json::Value::as_str)
                == Some("media")
                && proposal
                    .get("proposed_field")
                    .and_then(serde_json::Value::as_str)
                    == Some("suggested_link")
                && proposal
                    .get("proposed_value")
                    .and_then(|value| value.get("entity_type"))
                    .and_then(serde_json::Value::as_str)
                    == Some("person")
                && proposal
                    .get("proposed_value")
                    .and_then(|value| value.get("display_name"))
                    .and_then(serde_json::Value::as_str)
                    == Some("Mary Johnson")
        }),
        "expected suggested person link in filtered staging response: {staging_body:?}"
    );

    server.shutdown().await.expect("shutdown server");
}
