use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::Router;

use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/proposals", get(not_implemented).post(not_implemented))
        .route("/proposals/:id/accept", post(not_implemented))
        .route("/proposals/:id/reject", post(not_implemented))
}

async fn not_implemented() -> StatusCode {
    StatusCode::NOT_IMPLEMENTED
}
