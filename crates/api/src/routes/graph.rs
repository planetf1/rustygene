use axum::http::StatusCode;
use axum::routing::get;
use axum::Router;

use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/ancestors/:id", get(not_implemented))
        .route("/descendants/:id", get(not_implemented))
        .route("/pedigree/:id", get(not_implemented))
        .route("/path", get(not_implemented))
        .route("/network", get(not_implemented))
}

async fn not_implemented() -> StatusCode {
    StatusCode::NOT_IMPLEMENTED
}
