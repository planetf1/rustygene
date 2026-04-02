use axum::http::StatusCode;
use axum::routing::post;
use axum::Router;

use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/import", post(not_implemented))
        .route("/export", post(not_implemented))
}

async fn not_implemented() -> StatusCode {
    StatusCode::NOT_IMPLEMENTED
}
