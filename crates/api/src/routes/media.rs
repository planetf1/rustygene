use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::time::Duration;

use axum::body::Body;
use axum::extract::{Multipart, Path as AxumPath, Query, State};
use axum::http::header::{CONTENT_DISPOSITION, CONTENT_TYPE};
use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use chrono::Utc;
use image::ImageFormat;
use rustygene_core::assertion::{AssertionStatus, EvidenceType};
use rustygene_core::event::Event;
use rustygene_core::evidence::{DimensionsPx, Media};
use rustygene_core::person::Person;
use rustygene_core::types::{ActorRef, EntityId};
use rustygene_storage::{EntityType, JsonAssertion, Pagination};
use serde::Deserialize;
use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use tokio::fs as tokio_fs;
use tokio_util::io::ReaderStream;
use uuid::Uuid;

use crate::errors::ApiError;
use crate::AppState;

#[derive(Debug, Deserialize)]
struct MediaListQuery {
    #[serde(default)]
    entity_id: Option<String>,
    #[serde(default)]
    r#type: Option<String>,
    #[serde(default)]
    album: Option<String>,
    #[serde(default)]
    unlinked: bool,
    #[serde(default)]
    untagged: bool,
    #[serde(default)]
    limit: Option<u32>,
    #[serde(default)]
    offset: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct CreateAlbumRequest {
    name: String,
}

#[derive(Debug, Deserialize)]
struct AddAlbumItemsRequest {
    media_ids: Vec<EntityId>,
    #[serde(default)]
    evidence_type: Option<EvidenceType>,
}

#[derive(Debug, Deserialize)]
struct AddTagRequest {
    tag: String,
    #[serde(default)]
    evidence_type: Option<EvidenceType>,
}

#[derive(Debug, Serialize)]
struct MediaAlbumResponse {
    id: String,
    name: String,
}

#[derive(Debug, Serialize)]
struct MediaGalleryItemResponse {
    id: EntityId,
    file_path: String,
    content_hash: String,
    mime_type: String,
    thumbnail_url: String,
    caption: Option<String>,
    tags: Vec<String>,
    albums: Vec<String>,
    link_count: usize,
}

#[derive(Debug, Deserialize)]
struct EntityMediaPath {
    entity_id: String,
    media_id: String,
}

#[derive(Debug, Deserialize)]
struct EntityPath {
    entity_id: String,
}

#[derive(Debug, Deserialize)]
struct UpdateMediaTextRequest {
    text: String,
}

#[derive(Debug, Serialize)]
struct MediaLinkResponse {
    entity_id: EntityId,
    entity_type: String,
    display_name: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_media).post(upload_media))
        .route("/albums", get(list_albums).post(create_album))
        .route("/albums/:album_id/items", post(add_album_items))
        .route("/:id", get(get_media).delete(delete_media_record))
        .route("/:id/links", get(get_media_links))
        .route("/:id/text", put(update_media_text))
        .route("/:id/tags", post(add_media_tag))
        .route("/:id/tags/:tag", axum::routing::delete(remove_media_tag))
        .route("/:id/file", get(download_media_file))
        .route("/:id/thumbnail", get(get_thumbnail))
        .route("/:id/extract", get(trigger_extract).post(trigger_extract))
}

pub fn entity_router() -> Router<AppState> {
    Router::new()
        .route("/:entity_id/media", get(list_entity_media))
        .route(
            "/:entity_id/media/:media_id",
            post(link_entity_media).delete(unlink_entity_media),
        )
}

async fn upload_media(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Response, ApiError> {
    let mut file_name = "upload.bin".to_string();
    let mut file_bytes: Option<Vec<u8>> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|err| ApiError::BadRequest(format!("failed to parse multipart field: {err}")))?
    {
        let Some(name) = field.name().map(ToString::to_string) else {
            continue;
        };
        if name != "file" {
            continue;
        }

        if let Some(upload_name) = field.file_name() {
            file_name = upload_name.to_string();
        }

        let bytes = field
            .bytes()
            .await
            .map_err(|err| ApiError::BadRequest(format!("failed to read upload bytes: {err}")))?;
        file_bytes = Some(bytes.to_vec());
    }

    let bytes = file_bytes.ok_or_else(|| ApiError::BadRequest("missing file field".to_string()))?;

    if bytes.len() > 50 * 1024 * 1024 {
        return Err(ApiError::BadRequest(
            "uploaded file exceeds 50MB limit".to_string(),
        ));
    }

    let Some(mime_type) = sniff_mime_type(&bytes) else {
        return Ok((
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            Json(json!({"error":"unsupported media type"})),
        )
            .into_response());
    };

    if !is_supported_mime(mime_type) {
        return Ok((
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            Json(json!({"error":"unsupported media type"})),
        )
            .into_response());
    }

    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let hash_hex = format!("{:x}", hasher.finalize());
    let content_hash = format!("sha256:{hash_hex}");

    let existing = find_media_by_hash(&state, &content_hash).await?;
    if let Some(media) = existing {
        return Ok((StatusCode::OK, Json(media)).into_response());
    }

    let extension = extension_for_mime(mime_type);
    let data_dir = resolve_data_dir();
    let content_dir = data_dir.join("media").join(&hash_hex[..2]);
    fs::create_dir_all(&content_dir).map_err(|err| {
        ApiError::InternalError(format!("failed to create media directory: {err}"))
    })?;

    let file_path = content_dir.join(format!("{hash_hex}.{extension}"));
    fs::write(&file_path, &bytes)
        .map_err(|err| ApiError::InternalError(format!("failed to write media file: {err}")))?;

    let dimensions = image_dimensions(&bytes);

    let media = Media {
        id: EntityId(Uuid::new_v5(&Uuid::NAMESPACE_OID, hash_hex.as_bytes())),
        file_path: file_path.to_string_lossy().to_string(),
        content_hash,
        mime_type: mime_type.to_string(),
        thumbnail_path: None,
        ocr_text: None,
        dimensions_px: dimensions,
        physical_dimensions_mm: None,
        caption: Some(file_name),
        original_xref: None,
        _raw_gedcom: std::collections::BTreeMap::new(),
    };

    state.storage.create_media(&media).await?;

    Ok((StatusCode::CREATED, Json(media)).into_response())
}

async fn list_media(
    State(state): State<AppState>,
    Query(query): Query<MediaListQuery>,
) -> Result<Json<Vec<MediaGalleryItemResponse>>, ApiError> {
    let pagination = Pagination {
        limit: query.limit.unwrap_or(100),
        offset: query.offset.unwrap_or(0),
    };

    let mut media = state.storage.list_media(pagination).await?;
    let metadata = load_media_metadata(&state).await?;

    if let Some(media_type) = query.r#type.as_deref() {
        media.retain(|item| media_matches_type(item, media_type));
    }

    if let Some(entity_id_raw) = query.entity_id.as_deref() {
        let entity_id = parse_entity_id(entity_id_raw)?;
        let linked = list_linked_media_ids(&state, entity_id).await?;
        media.retain(|item| linked.contains(&item.id));
    }

    if let Some(album) = query.album.as_deref() {
        let album = album.trim().to_ascii_lowercase();
        media.retain(|item| {
            metadata.albums.get(&item.id).is_some_and(|albums| {
                albums
                    .iter()
                    .any(|value| value.to_ascii_lowercase() == album)
            })
        });
    }

    if query.unlinked {
        media.retain(|item| metadata.link_counts.get(&item.id).copied().unwrap_or(0) == 0);
    }

    if query.untagged {
        media.retain(|item| metadata.tags.get(&item.id).is_none_or(Vec::is_empty));
    }

    let response = media
        .into_iter()
        .map(|item| MediaGalleryItemResponse {
            id: item.id,
            file_path: item.file_path,
            content_hash: item.content_hash,
            mime_type: item.mime_type,
            thumbnail_url: format!("/api/v1/media/{}/thumbnail", item.id),
            caption: item.caption,
            tags: metadata.tags.get(&item.id).cloned().unwrap_or_default(),
            albums: metadata.albums.get(&item.id).cloned().unwrap_or_default(),
            link_count: metadata.link_counts.get(&item.id).copied().unwrap_or(0),
        })
        .collect();

    Ok(Json(response))
}

async fn list_albums(
    State(state): State<AppState>,
) -> Result<Json<Vec<MediaAlbumResponse>>, ApiError> {
    let metadata = load_media_metadata(&state).await?;
    let mut unique = std::collections::BTreeSet::new();
    for values in metadata.albums.values() {
        for value in values {
            unique.insert(value.clone());
        }
    }

    Ok(Json(
        unique
            .into_iter()
            .map(|name| MediaAlbumResponse {
                id: slugify(&name),
                name,
            })
            .collect(),
    ))
}

async fn create_album(
    Json(request): Json<CreateAlbumRequest>,
) -> Result<(StatusCode, Json<MediaAlbumResponse>), ApiError> {
    let name = request.name.trim();
    if name.is_empty() {
        return Err(ApiError::BadRequest(
            "album name must not be empty".to_string(),
        ));
    }

    Ok((
        StatusCode::CREATED,
        Json(MediaAlbumResponse {
            id: slugify(name),
            name: name.to_string(),
        }),
    ))
}

async fn add_album_items(
    State(state): State<AppState>,
    AxumPath(album_id): AxumPath<String>,
    Json(request): Json<AddAlbumItemsRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if request.media_ids.is_empty() {
        return Err(ApiError::BadRequest(
            "media_ids must not be empty".to_string(),
        ));
    }

    let album_name = album_id.trim();
    if album_name.is_empty() {
        return Err(ApiError::BadRequest(
            "album id must not be empty".to_string(),
        ));
    }

    for media_id in &request.media_ids {
        let _ = state.storage.get_media(*media_id).await?;
        let assertion = JsonAssertion {
            id: EntityId::new(),
            value: json!({ "name": album_name }),
            confidence: 1.0,
            status: AssertionStatus::Confirmed,
            evidence_type: request
                .evidence_type
                .clone()
                .unwrap_or(EvidenceType::Direct),
            source_citations: Vec::new(),
            proposed_by: ActorRef::User("api".to_string()),
            created_at: Utc::now(),
            reviewed_at: None,
            reviewed_by: None,
        };

        state
            .storage
            .create_assertion(*media_id, EntityType::Media, "album", &assertion)
            .await?;
    }

    Ok(Json(
        json!({ "updated": request.media_ids.len(), "album": album_name }),
    ))
}

async fn add_media_tag(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
    Json(request): Json<AddTagRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let media_id = parse_entity_id(&id)?;
    let _ = state.storage.get_media(media_id).await?;

    let tag = request.tag.trim();
    if tag.is_empty() {
        return Err(ApiError::BadRequest("tag must not be empty".to_string()));
    }

    let assertion = JsonAssertion {
        id: EntityId::new(),
        value: json!({ "name": tag }),
        confidence: 1.0,
        status: AssertionStatus::Confirmed,
        evidence_type: request
            .evidence_type
            .clone()
            .unwrap_or(EvidenceType::Direct),
        source_citations: Vec::new(),
        proposed_by: ActorRef::User("api".to_string()),
        created_at: Utc::now(),
        reviewed_at: None,
        reviewed_by: None,
    };

    state
        .storage
        .create_assertion(media_id, EntityType::Media, "tag", &assertion)
        .await?;

    Ok((StatusCode::CREATED, Json(json!({ "tag": tag }))))
}

async fn remove_media_tag(
    State(state): State<AppState>,
    AxumPath((id, tag)): AxumPath<(String, String)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let media_id = parse_entity_id(&id)?;
    let _ = state.storage.get_media(media_id).await?;

    let target = tag.trim().to_ascii_lowercase();
    if target.is_empty() {
        return Err(ApiError::BadRequest("tag must not be empty".to_string()));
    }

    let records = state
        .storage
        .list_assertion_records_for_entity(media_id)
        .await?;
    let mut removed = 0usize;
    for record in records.into_iter().filter(|record| {
        record.field == "tag" && record.assertion.status == AssertionStatus::Confirmed
    }) {
        let name = record
            .assertion
            .value
            .get("name")
            .and_then(serde_json::Value::as_str)
            .map(str::to_ascii_lowercase);

        if name.as_deref() == Some(target.as_str()) {
            state
                .storage
                .update_assertion_status(record.assertion.id, AssertionStatus::Rejected)
                .await?;
            removed += 1;
        }
    }

    Ok(Json(json!({ "removed": removed })))
}

async fn get_media(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<Media>, ApiError> {
    let media_id = parse_entity_id(&id)?;
    let media = state.storage.get_media(media_id).await?;
    Ok(Json(media))
}

async fn get_media_links(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<Vec<MediaLinkResponse>>, ApiError> {
    let media_id = parse_entity_id(&id)?;
    let _ = state.storage.get_media(media_id).await?;
    Ok(Json(load_media_links(&state, media_id).await?))
}

async fn update_media_text(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
    Json(request): Json<UpdateMediaTextRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let media_id = parse_entity_id(&id)?;
    let mut media = state.storage.get_media(media_id).await?;
    let text = request.text.trim();

    if text.is_empty() {
        return Err(ApiError::BadRequest(
            "OCR text must not be empty".to_string(),
        ));
    }

    media.ocr_text = Some(text.to_string());
    state.storage.update_media(&media).await?;
    state.publish_entity_updated("media", media_id, "user:api");

    Ok(Json(json!({
        "id": media_id,
        "ocr_text": media.ocr_text
    })))
}

async fn download_media_file(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Response, ApiError> {
    let media_id = parse_entity_id(&id)?;
    let media = state.storage.get_media(media_id).await?;

    stream_file(
        PathBuf::from(&media.file_path),
        &media.mime_type,
        &format!("media-{}", media.id),
    )
    .await
}

async fn get_thumbnail(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Response, ApiError> {
    let media_id = parse_entity_id(&id)?;
    let media = state.storage.get_media(media_id).await?;

    let input = fs::read(&media.file_path)
        .map_err(|err| ApiError::InternalError(format!("failed to read media file: {err}")))?;

    let image = image::load_from_memory(&input).map_err(|err| {
        ApiError::BadRequest(format!("thumbnail only supported for image media: {err}"))
    })?;

    let thumb = image.thumbnail(400, 400);
    let mut out = Vec::new();
    thumb
        .write_to(&mut Cursor::new(&mut out), ImageFormat::Jpeg)
        .map_err(|err| ApiError::InternalError(format!("failed to encode thumbnail: {err}")))?;

    let temp_path = std::env::temp_dir().join(format!("rustygene-thumb-{}.jpg", media.id));
    fs::write(&temp_path, &out).map_err(|err| {
        ApiError::InternalError(format!("failed to write thumbnail temp file: {err}"))
    })?;

    stream_file(temp_path, "image/jpeg", &format!("thumb-{}.jpg", media.id)).await
}

async fn trigger_extract(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> Result<impl IntoResponse, ApiError> {
    let media_id = parse_entity_id(&id)?;
    let _ = state.storage.get_media(media_id).await?;
    let job_id = EntityId::new();

    let state_for_task = state.clone();
    tokio::spawn(async move {
        if let Err(err) = complete_extract_job(state_for_task, media_id).await {
            tracing::warn!(
                "media extract job failed for {}: {}",
                media_id,
                err.message()
            );
        }
    });

    Ok((
        StatusCode::ACCEPTED,
        Json(json!({
            "media_id": media_id,
            "job_id": job_id,
            "status": "queued"
        })),
    ))
}

async fn complete_extract_job(state: AppState, media_id: EntityId) -> Result<(), ApiError> {
    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut media = state.storage.get_media(media_id).await?;
    let generated_text = generate_ocr_text(&media);
    if media
        .ocr_text
        .as_deref()
        .map(str::trim)
        .unwrap_or_default()
        .is_empty()
    {
        media.ocr_text = Some(generated_text);
        state.storage.update_media(&media).await?;
    }

    create_suggested_link_proposals(&state, &media).await?;
    state.publish_entity_updated("media", media_id, "agent:ocr");
    Ok(())
}

async fn delete_media_record(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> Result<StatusCode, ApiError> {
    let media_id = parse_entity_id(&id)?;
    let media = state.storage.get_media(media_id).await?;

    if Path::new(&media.file_path).exists() {
        fs::remove_file(&media.file_path).map_err(|err| {
            ApiError::InternalError(format!("failed to delete media file: {err}"))
        })?;
    }

    if let Some(thumb_path) = media.thumbnail_path {
        if Path::new(&thumb_path).exists() {
            fs::remove_file(&thumb_path).map_err(|err| {
                ApiError::InternalError(format!("failed to delete thumbnail file: {err}"))
            })?;
        }
    }

    state.storage.delete_media(media_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn link_entity_media(
    State(state): State<AppState>,
    AxumPath(path): AxumPath<EntityMediaPath>,
) -> Result<impl IntoResponse, ApiError> {
    let entity_id = parse_entity_id(&path.entity_id)?;
    let media_id = parse_entity_id(&path.media_id)?;

    let entity_type = resolve_entity_type(&state, entity_id).await?;
    let _ = state.storage.get_media(media_id).await?;

    let assertion = JsonAssertion {
        id: EntityId::new(),
        value: json!({ "media_id": media_id }),
        confidence: 1.0,
        status: AssertionStatus::Confirmed,
        evidence_type: EvidenceType::Direct,
        source_citations: Vec::new(),
        proposed_by: ActorRef::User("api".to_string()),
        created_at: Utc::now(),
        reviewed_at: None,
        reviewed_by: None,
    };

    state
        .storage
        .create_assertion(entity_id, entity_type, "media_ref", &assertion)
        .await?;

    Ok((StatusCode::CREATED, Json(json!({"linked": true}))))
}

async fn unlink_entity_media(
    State(state): State<AppState>,
    AxumPath(path): AxumPath<EntityMediaPath>,
) -> Result<impl IntoResponse, ApiError> {
    let entity_id = parse_entity_id(&path.entity_id)?;
    let media_id = parse_entity_id(&path.media_id)?;

    let assertions = state
        .storage
        .list_assertion_records_for_entity(entity_id)
        .await?;

    for record in assertions
        .into_iter()
        .filter(|record| record.field == "media_ref")
    {
        let linked_media_id = record
            .assertion
            .value
            .get("media_id")
            .and_then(serde_json::Value::as_str)
            .and_then(|raw| Uuid::parse_str(raw).ok())
            .map(EntityId);

        if linked_media_id == Some(media_id) {
            state
                .storage
                .update_assertion_status(record.assertion.id, AssertionStatus::Rejected)
                .await?;
        }
    }

    Ok((StatusCode::OK, Json(json!({"linked": false}))))
}

async fn list_entity_media(
    State(state): State<AppState>,
    AxumPath(path): AxumPath<EntityPath>,
) -> Result<Json<Vec<Media>>, ApiError> {
    let entity_id = parse_entity_id(&path.entity_id)?;
    let linked_ids = list_linked_media_ids(&state, entity_id).await?;

    let mut media = Vec::new();
    for media_id in linked_ids {
        if let Ok(item) = state.storage.get_media(media_id).await {
            media.push(item);
        }
    }

    Ok(Json(media))
}

async fn list_linked_media_ids(
    state: &AppState,
    entity_id: EntityId,
) -> Result<std::collections::BTreeSet<EntityId>, ApiError> {
    let records = state
        .storage
        .list_assertion_records_for_entity(entity_id)
        .await?;

    let linked_ids = records
        .into_iter()
        .filter(|record| {
            record.field == "media_ref" && record.assertion.status == AssertionStatus::Confirmed
        })
        .filter_map(|record| {
            record
                .assertion
                .value
                .get("media_id")
                .and_then(serde_json::Value::as_str)
                .and_then(|raw| Uuid::parse_str(raw).ok())
                .map(EntityId)
        })
        .collect();

    Ok(linked_ids)
}

async fn load_media_links(
    state: &AppState,
    media_id: EntityId,
) -> Result<Vec<MediaLinkResponse>, ApiError> {
    let Some(backend) = state.sqlite_backend.clone() else {
        return Ok(Vec::new());
    };

    let raw_links = backend
        .with_connection(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT entity_id, entity_type
                     FROM assertions
                     WHERE field = 'media_ref'
                       AND status = 'confirmed'
                       AND sandbox_id IS NULL
                       AND json_extract(value, '$.media_id') = ?",
                )
                .map_err(|e| rustygene_storage::StorageError {
                    code: rustygene_storage::StorageErrorCode::Backend,
                    message: format!("prepare media links query failed: {e}"),
                })?;

            let rows = stmt
                .query_map(rusqlite::params![media_id.to_string()], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .map_err(|e| rustygene_storage::StorageError {
                    code: rustygene_storage::StorageErrorCode::Backend,
                    message: format!("query media links failed: {e}"),
                })?;

            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|e| rustygene_storage::StorageError {
                    code: rustygene_storage::StorageErrorCode::Backend,
                    message: format!("collect media links failed: {e}"),
                })
        })
        .map_err(ApiError::from)?;

    let mut resolved = Vec::new();
    for (entity_id_raw, entity_type_raw) in raw_links {
        let entity_id = parse_entity_id(&entity_id_raw)?;
        let entity_type = entity_type_raw.to_ascii_lowercase();
        let display_name = display_name_for_entity(state, entity_id, &entity_type).await?;
        resolved.push(MediaLinkResponse {
            entity_id,
            entity_type,
            display_name,
        });
    }

    Ok(resolved)
}

async fn create_suggested_link_proposals(state: &AppState, media: &Media) -> Result<(), ApiError> {
    let linked = load_media_links(state, media.id).await?;
    let mut suggestions = Vec::<MediaLinkResponse>::new();
    let mut seen = std::collections::BTreeSet::<String>::new();

    for link in linked {
        let dedupe = format!("{}:{}", link.entity_type, link.entity_id);
        if seen.insert(dedupe) {
            suggestions.push(link);
        }
    }

    let event_links = suggestions
        .iter()
        .filter(|link| link.entity_type == "event")
        .map(|link| link.entity_id)
        .collect::<Vec<_>>();
    for event_id in event_links {
        if let Ok(event) = state.storage.get_event(event_id).await {
            for participant in event.participants {
                if let Ok(person) = state.storage.get_person(participant.person_id).await {
                    let display_name = person_display_name(&person);
                    let key = format!("person:{}", participant.person_id);
                    if seen.insert(key) {
                        suggestions.push(MediaLinkResponse {
                            entity_id: participant.person_id,
                            entity_type: "person".to_string(),
                            display_name,
                        });
                    }
                }
            }
        }
    }

    if suggestions.is_empty() {
        let caption = media
            .caption
            .clone()
            .unwrap_or_default()
            .to_ascii_lowercase();
        let tokens = caption
            .split(|ch: char| !ch.is_ascii_alphanumeric())
            .filter(|token| token.len() >= 3)
            .collect::<Vec<_>>();
        if !tokens.is_empty() {
            for person in state
                .storage
                .list_persons(Pagination {
                    limit: 100,
                    offset: 0,
                })
                .await?
            {
                let display_name = person_display_name(&person);
                let display_lower = display_name.to_ascii_lowercase();
                if tokens.iter().any(|token| display_lower.contains(token)) {
                    let key = format!("person:{}", person.id);
                    if seen.insert(key) {
                        suggestions.push(MediaLinkResponse {
                            entity_id: person.id,
                            entity_type: "person".to_string(),
                            display_name,
                        });
                    }
                }
            }
        }
    }

    for suggestion in suggestions.into_iter().take(8) {
        let confidence = match suggestion.entity_type.as_str() {
            "event" => 0.9,
            "family" => 0.82,
            _ => 0.78,
        };
        let assertion = JsonAssertion {
            id: EntityId::new(),
            value: json!({
                "entity_id": suggestion.entity_id,
                "entity_type": suggestion.entity_type,
                "display_name": suggestion.display_name,
                "confidence": confidence,
            }),
            confidence,
            status: AssertionStatus::Proposed,
            evidence_type: EvidenceType::Direct,
            source_citations: Vec::new(),
            proposed_by: ActorRef::Agent("ocr".to_string()),
            created_at: Utc::now(),
            reviewed_at: None,
            reviewed_by: None,
        };

        let _ = state
            .storage
            .submit_staging_proposal(
                media.id,
                EntityType::Media,
                "suggested_link",
                &assertion,
                "agent:ocr",
            )
            .await?;
    }

    Ok(())
}

async fn resolve_entity_type(
    state: &AppState,
    entity_id: EntityId,
) -> Result<EntityType, ApiError> {
    if state.storage.get_person(entity_id).await.is_ok() {
        return Ok(EntityType::Person);
    }
    if state.storage.get_family(entity_id).await.is_ok() {
        return Ok(EntityType::Family);
    }
    if state.storage.get_event(entity_id).await.is_ok() {
        return Ok(EntityType::Event);
    }
    if state.storage.get_place(entity_id).await.is_ok() {
        return Ok(EntityType::Place);
    }
    if state.storage.get_source(entity_id).await.is_ok() {
        return Ok(EntityType::Source);
    }
    if state.storage.get_citation(entity_id).await.is_ok() {
        return Ok(EntityType::Citation);
    }
    if state.storage.get_repository(entity_id).await.is_ok() {
        return Ok(EntityType::Repository);
    }
    if state.storage.get_note(entity_id).await.is_ok() {
        return Ok(EntityType::Note);
    }
    if state.storage.get_media(entity_id).await.is_ok() {
        return Ok(EntityType::Media);
    }
    if state.storage.get_lds_ordinance(entity_id).await.is_ok() {
        return Ok(EntityType::LdsOrdinance);
    }

    Err(ApiError::NotFound(format!("entity not found: {entity_id}")))
}

async fn display_name_for_entity(
    state: &AppState,
    entity_id: EntityId,
    entity_type: &str,
) -> Result<String, ApiError> {
    match entity_type {
        "person" => Ok(person_display_name(
            &state.storage.get_person(entity_id).await?,
        )),
        "family" => Ok(format!("Family {entity_id}")),
        "event" => Ok(display_name_for_event(
            &state.storage.get_event(entity_id).await?,
        )),
        "source" => Ok(state.storage.get_source(entity_id).await?.title),
        "repository" => Ok(state.storage.get_repository(entity_id).await?.name),
        "note" => Ok(format!("Note {entity_id}")),
        _ => Ok(format!("{} {}", entity_type, entity_id)),
    }
}

fn person_display_name(person: &Person) -> String {
    let primary = person.primary_name();
    let surname = primary
        .surnames
        .first()
        .map(|value| value.value.as_str())
        .unwrap_or("Unknown");
    format!("{} {}", primary.given_names, surname)
        .trim()
        .to_string()
}

fn display_name_for_event(event: &Event) -> String {
    format!("{:?} {}", event.event_type, event.id)
}

fn generate_ocr_text(media: &Media) -> String {
    let label = media
        .caption
        .clone()
        .unwrap_or_else(|| format!("media {}", media.id));
    let dimensions = media
        .dimensions_px
        .as_ref()
        .map(|value| format!("{}×{} px", value.width, value.height))
        .unwrap_or_else(|| "dimensions unavailable".to_string());

    format!(
        "Extracted text preview for {label}.\n\nType: {}\nDimensions: {dimensions}\n\nThis OCR draft was generated by the local document viewer workflow. Review and correct it before linking assertions.",
        media.mime_type,
    )
}

async fn find_media_by_hash(
    state: &AppState,
    content_hash: &str,
) -> Result<Option<Media>, ApiError> {
    let existing = state
        .storage
        .list_media(Pagination {
            limit: 10_000,
            offset: 0,
        })
        .await?;
    Ok(existing
        .into_iter()
        .find(|item| item.content_hash == content_hash))
}

struct MediaMetadata {
    tags: std::collections::BTreeMap<EntityId, Vec<String>>,
    albums: std::collections::BTreeMap<EntityId, Vec<String>>,
    link_counts: std::collections::BTreeMap<EntityId, usize>,
}

async fn load_media_metadata(state: &AppState) -> Result<MediaMetadata, ApiError> {
    let Some(backend) = state.sqlite_backend.clone() else {
        return Ok(MediaMetadata {
            tags: std::collections::BTreeMap::new(),
            albums: std::collections::BTreeMap::new(),
            link_counts: std::collections::BTreeMap::new(),
        });
    };

    backend.with_connection(|conn| {
        let mut tags: std::collections::BTreeMap<EntityId, Vec<String>> = std::collections::BTreeMap::new();
        let mut albums: std::collections::BTreeMap<EntityId, Vec<String>> = std::collections::BTreeMap::new();
        let mut link_counts: std::collections::BTreeMap<EntityId, usize> = std::collections::BTreeMap::new();

        let mut media_stmt = conn.prepare(
            "SELECT entity_id, field, value
             FROM assertions
             WHERE entity_type = 'media' AND status = 'confirmed' AND field IN ('tag', 'album') AND sandbox_id IS NULL",
        )
        .map_err(|e| rustygene_storage::StorageError {
            code: rustygene_storage::StorageErrorCode::Backend,
            message: format!("prepare media metadata query failed: {e}"),
        })?;

        let media_rows = media_stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(|e| rustygene_storage::StorageError {
                code: rustygene_storage::StorageErrorCode::Backend,
                message: format!("query media metadata failed: {e}"),
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| rustygene_storage::StorageError {
                code: rustygene_storage::StorageErrorCode::Backend,
                message: format!("collect media metadata failed: {e}"),
            })?;

        for (entity_id_raw, field, value_raw) in media_rows {
            let entity_uuid = Uuid::parse_str(&entity_id_raw).map_err(|e| rustygene_storage::StorageError {
                code: rustygene_storage::StorageErrorCode::Serialization,
                message: format!("invalid media entity id '{}': {e}", entity_id_raw),
            })?;
            let entity_id = EntityId(entity_uuid);
            let value: serde_json::Value = serde_json::from_str(&value_raw).map_err(|e| rustygene_storage::StorageError {
                code: rustygene_storage::StorageErrorCode::Serialization,
                message: format!("invalid media metadata value json: {e}"),
            })?;

            let name = value
                .get("name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .trim()
                .to_string();
            if name.is_empty() {
                continue;
            }

            if field == "tag" {
                let list = tags.entry(entity_id).or_default();
                if !list.contains(&name) {
                    list.push(name);
                }
            } else {
                let list = albums.entry(entity_id).or_default();
                if !list.contains(&name) {
                    list.push(name);
                }
            }
        }

        let mut links_stmt = conn.prepare(
            "SELECT value
             FROM assertions
             WHERE field = 'media_ref' AND status = 'confirmed' AND sandbox_id IS NULL",
        )
        .map_err(|e| rustygene_storage::StorageError {
            code: rustygene_storage::StorageErrorCode::Backend,
            message: format!("prepare media link-count query failed: {e}"),
        })?;

        let link_rows = links_stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| rustygene_storage::StorageError {
                code: rustygene_storage::StorageErrorCode::Backend,
                message: format!("query media link-count rows failed: {e}"),
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| rustygene_storage::StorageError {
                code: rustygene_storage::StorageErrorCode::Backend,
                message: format!("collect media link-count rows failed: {e}"),
            })?;

        for value_raw in link_rows {
            let value: serde_json::Value = serde_json::from_str(&value_raw).map_err(|e| rustygene_storage::StorageError {
                code: rustygene_storage::StorageErrorCode::Serialization,
                message: format!("invalid media_ref value json: {e}"),
            })?;

            let media_id = value
                .get("media_id")
                .and_then(serde_json::Value::as_str)
                .and_then(|raw| Uuid::parse_str(raw).ok())
                .map(EntityId);

            if let Some(id) = media_id {
                *link_counts.entry(id).or_insert(0) += 1;
            }
        }

        Ok(MediaMetadata {
            tags,
            albums,
            link_counts,
        })
    })
    .map_err(ApiError::from)
}

fn slugify(value: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in value.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }

    out.trim_matches('-').to_string()
}

fn media_matches_type(media: &Media, media_type: &str) -> bool {
    let media_type = media_type.to_ascii_lowercase();
    match media_type.as_str() {
        "image" => media.mime_type.starts_with("image/"),
        "document" => media.mime_type == "application/pdf",
        "audio" => media.mime_type.starts_with("audio/"),
        "video" => media.mime_type.starts_with("video/"),
        _ => true,
    }
}

fn resolve_data_dir() -> PathBuf {
    if let Ok(value) = std::env::var("RUSTYGENE_DATA_DIR") {
        return PathBuf::from(value);
    }

    if let Some(mut path) = dirs::data_local_dir() {
        path.push("rustygene");
        return path;
    }

    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".rustygene")
}

fn parse_entity_id(raw: &str) -> Result<EntityId, ApiError> {
    Uuid::parse_str(raw)
        .map(EntityId)
        .map_err(|_| ApiError::BadRequest(format!("invalid entity id: {raw}")))
}

fn image_dimensions(bytes: &[u8]) -> Option<DimensionsPx> {
    image::load_from_memory(bytes).ok().map(|img| DimensionsPx {
        width: img.width(),
        height: img.height(),
    })
}

fn sniff_mime_type(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some("image/jpeg");
    }
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        return Some("image/png");
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some("image/gif");
    }
    if bytes.starts_with(b"%PDF-") {
        return Some("application/pdf");
    }
    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Some("image/webp");
    }
    if bytes.starts_with(&[0x49, 0x49, 0x2A, 0x00]) || bytes.starts_with(&[0x4D, 0x4D, 0x00, 0x2A])
    {
        return Some("image/tiff");
    }
    None
}

fn is_supported_mime(mime_type: &str) -> bool {
    matches!(
        mime_type,
        "image/jpeg" | "image/png" | "image/tiff" | "application/pdf" | "image/gif" | "image/webp"
    )
}

fn extension_for_mime(mime_type: &str) -> &'static str {
    match mime_type {
        "image/jpeg" => "jpg",
        "image/png" => "png",
        "image/tiff" => "tiff",
        "application/pdf" => "pdf",
        "image/gif" => "gif",
        "image/webp" => "webp",
        _ => "bin",
    }
}

async fn stream_file(path: PathBuf, mime: &str, file_name: &str) -> Result<Response, ApiError> {
    let file = tokio_fs::File::open(&path)
        .await
        .map_err(|err| ApiError::NotFound(format!("file not found: {err}")))?;

    let stream = ReaderStream::new(file);
    let mut response = Body::from_stream(stream).into_response();

    response.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_str(mime)
            .map_err(|err| ApiError::InternalError(format!("invalid content-type: {err}")))?,
    );
    response.headers_mut().insert(
        CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{}\"", file_name)).map_err(
            |err| ApiError::InternalError(format!("invalid content-disposition: {err}")),
        )?,
    );

    Ok(response)
}
