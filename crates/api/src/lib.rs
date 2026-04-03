pub mod errors;
pub mod models;
pub mod routes;

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;

use axum::extract::State;
use axum::http::header::{HeaderName, HeaderValue};
use axum::http::Method;
use axum::response::Json;
use axum::routing::get;
use axum::Router;
use serde::Serialize;
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tokio::sync::oneshot;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::trace::TraceLayer;

use crate::errors::ApiError;
use rustygene_core::types::EntityId;
use rustygene_storage::sqlite_impl::SqliteBackend;
use rustygene_storage::Storage;
use uuid::Uuid;

const DEFAULT_CORS_ORIGINS: [&str; 3] = [
    "tauri://localhost",
    "https://tauri.localhost",
    "http://localhost",
];

pub const EVENT_BUS_CAPACITY: usize = 1024;
pub const SLOW_CONSUMER_DROP_THRESHOLD: u64 = 1000;

#[derive(Debug, Clone)]
pub enum DomainEvent {
    EntityCreated {
        entity_type: String,
        entity_id: Uuid,
        timestamp: String,
        actor: String,
    },
    EntityUpdated {
        entity_type: String,
        entity_id: Uuid,
        timestamp: String,
        actor: String,
    },
    EntityDeleted {
        entity_type: String,
        entity_id: Uuid,
        timestamp: String,
        actor: String,
    },
    ImportCompleted {
        job_id: Uuid,
        entities_imported: std::collections::BTreeMap<String, usize>,
        timestamp: String,
    },
    StagingApproved {
        id: Uuid,
        timestamp: String,
        actor: String,
    },
    StagingRejected {
        id: Uuid,
        timestamp: String,
        actor: String,
    },
}

impl DomainEvent {
    pub fn event_name(&self) -> &'static str {
        match self {
            Self::EntityCreated { .. } => "entity.created",
            Self::EntityUpdated { .. } => "entity.updated",
            Self::EntityDeleted { .. } => "entity.deleted",
            Self::ImportCompleted { .. } => "import.completed",
            Self::StagingApproved { .. } => "staging.approved",
            Self::StagingRejected { .. } => "staging.rejected",
        }
    }

    pub fn payload(&self) -> serde_json::Value {
        match self {
            Self::EntityCreated {
                entity_type,
                entity_id,
                timestamp,
                actor,
            }
            | Self::EntityUpdated {
                entity_type,
                entity_id,
                timestamp,
                actor,
            }
            | Self::EntityDeleted {
                entity_type,
                entity_id,
                timestamp,
                actor,
            } => serde_json::json!({
                "event": self.event_name(),
                "entity_type": entity_type,
                "entity_id": entity_id,
                "timestamp": timestamp,
                "actor": actor,
            }),
            Self::ImportCompleted {
                job_id,
                entities_imported,
                timestamp,
            } => serde_json::json!({
                "event": self.event_name(),
                "job_id": job_id,
                "entities_imported": entities_imported,
                "timestamp": timestamp,
            }),
            Self::StagingApproved {
                id,
                timestamp,
                actor,
            }
            | Self::StagingRejected {
                id,
                timestamp,
                actor,
            } => serde_json::json!({
                "event": self.event_name(),
                "id": id,
                "timestamp": timestamp,
                "actor": actor,
            }),
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    pub storage: Arc<dyn Storage + Send + Sync>,
    pub sqlite_backend: Option<Arc<SqliteBackend>>,
    pub port: u16,
    pub cors_origins: Vec<String>,
    pub import_jobs: Arc<RwLock<HashMap<Uuid, routes::import_export::ImportJobStatus>>>,
    pub event_bus: broadcast::Sender<DomainEvent>,
    pub sse_connections: Arc<AtomicUsize>,
    pub slow_consumer_drop_threshold: u64,
}

impl AppState {
    pub fn new(
        storage: Arc<dyn Storage + Send + Sync>,
        sqlite_backend: Option<Arc<SqliteBackend>>,
        port: u16,
        cors_origins: Vec<String>,
    ) -> Result<Self, ApiError> {
        Self::new_with_event_bus_capacity(
            storage,
            sqlite_backend,
            port,
            cors_origins,
            EVENT_BUS_CAPACITY,
            SLOW_CONSUMER_DROP_THRESHOLD,
        )
    }

    pub fn new_with_event_bus_capacity(
        storage: Arc<dyn Storage + Send + Sync>,
        sqlite_backend: Option<Arc<SqliteBackend>>,
        port: u16,
        cors_origins: Vec<String>,
        event_bus_capacity: usize,
        slow_consumer_drop_threshold: u64,
    ) -> Result<Self, ApiError> {
        if cors_origins.is_empty() {
            return Err(ApiError::BadRequest(
                "cors_origins must not be empty".to_string(),
            ));
        }

        if event_bus_capacity == 0 {
            return Err(ApiError::BadRequest(
                "event_bus_capacity must not be zero".to_string(),
            ));
        }

        Ok(Self {
            storage,
            sqlite_backend,
            port,
            cors_origins,
            import_jobs: Arc::new(RwLock::new(HashMap::new())),
            event_bus: broadcast::channel(event_bus_capacity).0,
            sse_connections: Arc::new(AtomicUsize::new(0)),
            slow_consumer_drop_threshold,
        })
    }

    pub fn with_default_cors(
        storage: Arc<dyn Storage + Send + Sync>,
        port: u16,
    ) -> Result<Self, ApiError> {
        Self::new(
            storage,
            None,
            port,
            DEFAULT_CORS_ORIGINS
                .iter()
                .map(ToString::to_string)
                .collect(),
        )
    }

    pub fn with_default_cors_sqlite(
        backend: Arc<SqliteBackend>,
        port: u16,
    ) -> Result<Self, ApiError> {
        let storage: Arc<dyn Storage + Send + Sync> = backend.clone();
        Self::new(
            storage,
            Some(backend),
            port,
            DEFAULT_CORS_ORIGINS
                .iter()
                .map(ToString::to_string)
                .collect(),
        )
    }

    fn publish_event(&self, event: DomainEvent) {
        let _ = self.event_bus.send(event);
    }

    pub fn publish_entity_created(&self, entity_type: &str, entity_id: EntityId, actor: &str) {
        self.publish_event(DomainEvent::EntityCreated {
            entity_type: entity_type.to_string(),
            entity_id: entity_id.0,
            timestamp: chrono::Utc::now().to_rfc3339(),
            actor: actor.to_string(),
        });
    }

    pub fn publish_entity_updated(&self, entity_type: &str, entity_id: EntityId, actor: &str) {
        self.publish_event(DomainEvent::EntityUpdated {
            entity_type: entity_type.to_string(),
            entity_id: entity_id.0,
            timestamp: chrono::Utc::now().to_rfc3339(),
            actor: actor.to_string(),
        });
    }

    pub fn publish_entity_deleted(&self, entity_type: &str, entity_id: EntityId, actor: &str) {
        self.publish_event(DomainEvent::EntityDeleted {
            entity_type: entity_type.to_string(),
            entity_id: entity_id.0,
            timestamp: chrono::Utc::now().to_rfc3339(),
            actor: actor.to_string(),
        });
    }

    pub fn publish_import_completed(
        &self,
        job_id: Uuid,
        entities_imported: std::collections::BTreeMap<String, usize>,
    ) {
        self.publish_event(DomainEvent::ImportCompleted {
            job_id,
            entities_imported,
            timestamp: chrono::Utc::now().to_rfc3339(),
        });
    }

    pub fn publish_staging_approved(&self, proposal_id: EntityId, actor: &str) {
        self.publish_event(DomainEvent::StagingApproved {
            id: proposal_id.0,
            timestamp: chrono::Utc::now().to_rfc3339(),
            actor: actor.to_string(),
        });
    }

    pub fn publish_staging_rejected(&self, proposal_id: EntityId, actor: &str) {
        self.publish_event(DomainEvent::StagingRejected {
            id: proposal_id.0,
            timestamp: chrono::Utc::now().to_rfc3339(),
            actor: actor.to_string(),
        });
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
        .nest(
            "/api/v1/events",
            routes::events::router().merge(routes::events_sse::router()),
        )
        .nest("/api/v1/search", routes::search::router())
        .nest("/api/v1/graph", routes::graph::router())
        .nest("/api/v1/sources", routes::sources::router())
        .nest("/api/v1/citations", routes::citations::router())
        .nest("/api/v1/repositories", routes::repositories::router())
        .nest("/api/v1/notes", routes::notes::router())
        .nest("/api/v1/research-log", routes::research_log::router())
        .nest("/api/v1/media", routes::media::router())
        .nest("/api/v1/entities", routes::media::entity_router())
        .nest("/api/v1/assertions", routes::assertions::router())
        .nest("/api/v1/staging", routes::staging::router())
        .nest("/api/v1", routes::import_export::router())
        .nest(
            "/api/v1/import-export",
            routes::import_export::legacy_router(),
        )
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
