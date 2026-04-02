pub mod errors;
pub mod models;
pub mod routes;

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use axum::extract::State;
use axum::http::header::{HeaderName, HeaderValue};
use axum::http::Method;
use axum::response::Json;
use axum::routing::get;
use axum::Router;
use serde::Serialize;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::trace::TraceLayer;

use crate::errors::ApiError;
use rustygene_storage::Storage;

const DEFAULT_CORS_ORIGINS: [&str; 3] = [
    "tauri://localhost",
    "https://tauri.localhost",
    "http://localhost",
];

#[derive(Clone)]
pub struct AppState {
    pub storage: Arc<dyn Storage + Send + Sync>,
    pub port: u16,
    pub cors_origins: Vec<String>,
}

impl AppState {
    pub fn new(
        storage: Arc<dyn Storage + Send + Sync>,
        port: u16,
        cors_origins: Vec<String>,
    ) -> Result<Self, ApiError> {
        if cors_origins.is_empty() {
            return Err(ApiError::BadRequest(
                "cors_origins must not be empty".to_string(),
            ));
        }

        Ok(Self {
            storage,
            port,
            cors_origins,
        })
    }

    pub fn with_default_cors(
        storage: Arc<dyn Storage + Send + Sync>,
        port: u16,
    ) -> Result<Self, ApiError> {
        Self::new(
            storage,
            port,
            DEFAULT_CORS_ORIGINS.iter().map(ToString::to_string).collect(),
        )
    }
}

#[derive(Debug, Clone, Serialize)]
struct HealthResponse {
    status: &'static str,
    version: &'static str,
}

pub struct ServerHandle {
    pub local_addr: SocketAddr,
    shutdown_tx: Option<oneshot::Sender<()>>,
    task: JoinHandle<std::io::Result<()>>,
}

impl ServerHandle {
    pub async fn shutdown(mut self) -> std::io::Result<()> {
        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            let _ = shutdown_tx.send(());
        }

        self.task.await.map_err(std::io::Error::other)?
    }
}

pub fn build_router(state: AppState) -> Router {
    let allowed_origins = state
        .cors_origins
        .iter()
        .map(|origin| {
            HeaderValue::from_str(origin).map_err(|_| {
                ApiError::BadRequest(format!("invalid CORS origin configured: {origin}"))
            })
        })
        .collect::<Result<Vec<_>, _>>()
        .unwrap_or_else(|err| panic!("invalid AppState configuration: {}", err.message()));

    let cors_layer = CorsLayer::new()
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_origin(AllowOrigin::list(allowed_origins));

    Router::new()
        .route("/api/v1/health", get(health_handler))
        .nest("/api/v1/persons", routes::persons::router())
        .nest("/api/v1/families", routes::families::router())
        .nest("/api/v1/events", routes::events::router())
        .nest("/api/v1/search", routes::search::router())
        .nest("/api/v1/graph", routes::graph::router())
        .nest("/api/v1/media", routes::media::router())
        .nest("/api/v1/staging", routes::staging::router())
        .nest("/api/v1/import-export", routes::import_export::router())
        .with_state(state)
        .layer(TraceLayer::new_for_http())
        .layer(SetResponseHeaderLayer::if_not_present(
            HeaderName::from_static("x-content-type-options"),
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            HeaderName::from_static("x-frame-options"),
            HeaderValue::from_static("DENY"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            HeaderName::from_static("x-xss-protection"),
            HeaderValue::from_static("1; mode=block"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            HeaderName::from_static("referrer-policy"),
            HeaderValue::from_static("no-referrer"),
        ))
        .layer(RequestBodyLimitLayer::new(10 * 1024 * 1024))
        .layer(cors_layer)
}

pub async fn start_server(state: AppState, port: u16) -> Result<ServerHandle, ApiError> {
    let bind_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
    let listener = TcpListener::bind(bind_addr)
        .await
        .map_err(|err| ApiError::InternalError(format!("bind failed: {err}")))?;

    let local_addr = listener
        .local_addr()
        .map_err(|err| ApiError::InternalError(format!("read local address failed: {err}")))?;

    if local_addr.ip() != IpAddr::V4(Ipv4Addr::LOCALHOST) {
        return Err(ApiError::InternalError(format!(
            "server must bind to 127.0.0.1, got {}",
            local_addr.ip()
        )));
    }

    let router = build_router(state);
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    let task = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            })
            .await
    });

    Ok(ServerHandle {
        local_addr,
        shutdown_tx: Some(shutdown_tx),
        task,
    })
}

async fn health_handler(State(state): State<AppState>) -> Json<HealthResponse> {
    let _ = state;
    Json(HealthResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}
