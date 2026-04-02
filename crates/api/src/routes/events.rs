use axum::http::StatusCode;
use axum::routing::get;
use axum::Router;

use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(not_implemented).post(not_implemented))
        .route("/:id", get(not_implemented).put(not_implemented).delete(not_implemented))
}

async fn not_implemented() -> StatusCode {
    StatusCode::NOT_IMPLEMENTED
}
