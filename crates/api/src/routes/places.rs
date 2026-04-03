use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use rustygene_core::place::{ExternalId, Place, PlaceName, PlaceRef, PlaceType};
use rustygene_core::types::EntityId;
use rustygene_storage::Pagination;
use serde::Deserialize;
use uuid::Uuid;

use crate::errors::ApiError;
use crate::AppState;

#[derive(Debug, Deserialize)]
struct PlacesQuery {
    #[serde(default)]
    limit: Option<u32>,
    #[serde(default)]
    offset: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct UpsertPlaceRequest {
    #[serde(default)]
    place_type: Option<PlaceType>,
    #[serde(default)]
    names: Vec<PlaceName>,
    #[serde(default)]
    coordinates: Option<(f64, f64)>,
    #[serde(default)]
    enclosed_by: Vec<PlaceRef>,
    #[serde(default)]
    external_ids: Vec<ExternalId>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_places).post(create_place))
        .route(
            "/:id",
            get(get_place).put(update_place).delete(delete_place),
        )
}

async fn list_places(
    State(state): State<AppState>,
    Query(query): Query<PlacesQuery>,
) -> Result<Json<Vec<Place>>, ApiError> {
    let places = state
        .storage
        .list_places(Pagination {
            limit: query.limit.unwrap_or(100),
            offset: query.offset.unwrap_or(0),
        })
        .await?;

    Ok(Json(places))
}

async fn create_place(
    State(state): State<AppState>,
    Json(request): Json<UpsertPlaceRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    if request.names.is_empty() {
        return Err(ApiError::BadRequest(
            "place must have at least one name".to_string(),
        ));
    }

    let place_id = EntityId::new();
    let place = Place {
        id: place_id,
        place_type: request.place_type.unwrap_or_default(),
        names: request.names,
        coordinates: request.coordinates,
        enclosed_by: request.enclosed_by,
        external_ids: request.external_ids,
    };

    state.storage.create_place(&place).await?;

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "id": place_id })),
    ))
}

async fn get_place(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Place>, ApiError> {
    let place_id = parse_entity_id(&id)?;
    let place = state.storage.get_place(place_id).await?;
    Ok(Json(place))
}

async fn update_place(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<UpsertPlaceRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if request.names.is_empty() {
        return Err(ApiError::BadRequest(
            "place must have at least one name".to_string(),
        ));
    }

    let place_id = parse_entity_id(&id)?;
    let mut place = state.storage.get_place(place_id).await?;

    place.place_type = request.place_type.unwrap_or_default();
    place.names = request.names;
    place.coordinates = request.coordinates;
    place.enclosed_by = request.enclosed_by;
    place.external_ids = request.external_ids;

    state.storage.update_place(&place).await?;

    Ok(Json(serde_json::json!({ "id": place_id })))
}

async fn delete_place(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let place_id = parse_entity_id(&id)?;
    let _ = state.storage.get_place(place_id).await?;
    state.storage.delete_place(place_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

fn parse_entity_id(raw: &str) -> Result<EntityId, ApiError> {
    Uuid::parse_str(raw)
        .map(EntityId)
        .map_err(|_| ApiError::BadRequest(format!("invalid entity id: {raw}")))
}
