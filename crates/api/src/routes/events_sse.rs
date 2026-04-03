use std::collections::HashSet;
use std::convert::Infallible;
use std::sync::atomic::Ordering;
use std::time::Duration;

use axum::extract::{Query, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::get;
use axum::Router;
use serde::Deserialize;
use tokio_stream::Stream;

use crate::{AppState, DomainEvent};

const SSE_CONNECTION_WARN_THRESHOLD: usize = 50;

#[derive(Debug, Deserialize)]
struct StreamQuery {
    #[serde(default)]
    types: Option<String>,
}

pub fn router() -> Router<AppState> {
    Router::new().route("/stream", get(stream_events))
}

async fn stream_events(
    State(state): State<AppState>,
    Query(query): Query<StreamQuery>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let filters = parse_filters(query.types.as_deref());

    Sse::new(build_event_stream(state, filters))
        .keep_alive(KeepAlive::new().interval(Duration::from_secs(30)).text(""))
}

fn build_event_stream(
    state: AppState,
    filters: Option<HashSet<String>>,
) -> impl Stream<Item = Result<Event, Infallible>> {
    let slow_consumer_drop_threshold = state.slow_consumer_drop_threshold;

    let open_connections = state.sse_connections.fetch_add(1, Ordering::SeqCst) + 1;
    if open_connections > SSE_CONNECTION_WARN_THRESHOLD {
        tracing::warn!(
            "high SSE connection count: {} (threshold: {})",
            open_connections,
            SSE_CONNECTION_WARN_THRESHOLD
        );
    }

    let mut receiver = state.event_bus.subscribe();
    let connection_counter = state.sse_connections.clone();

    async_stream::stream! {
        let _guard = ConnectionGuard { counter: connection_counter };

        loop {
            match receiver.recv().await {
                Ok(event) => {
                    if !matches_filter(&filters, &event) {
                        continue;
                    }

                    let payload = event.payload();
                    let payload_json = serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string());
                    let timestamp = payload
                        .get("timestamp")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default();

                    yield Ok(Event::default()
                        .id(timestamp)
                        .event(event.event_name())
                        .data(payload_json));
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                    if skipped > slow_consumer_drop_threshold {
                        yield Ok(Event::default().comment("slow-consumer-dropped"));
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    break;
                }
            }
        }
    }
}

fn parse_filters(raw: Option<&str>) -> Option<HashSet<String>> {
    let raw = raw?;

    let filters = raw
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .collect::<HashSet<_>>();

    if filters.is_empty() {
        None
    } else {
        Some(filters)
    }
}

fn matches_filter(filters: &Option<HashSet<String>>, event: &DomainEvent) -> bool {
    filters
        .as_ref()
        .is_none_or(|values| values.contains(event.event_name()))
}

struct ConnectionGuard {
    counter: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        self.counter.fetch_sub(1, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use rusqlite::Connection;
    use rustygene_core::types::EntityId;
    use rustygene_storage::run_migrations;
    use rustygene_storage::sqlite_impl::SqliteBackend;
    use rustygene_storage::Storage;
    use tokio_stream::StreamExt;

    use super::build_event_stream;
    use crate::AppState;

    fn in_memory_backend() -> Arc<SqliteBackend> {
        let mut conn = Connection::open_in_memory().expect("open in-memory sqlite connection");
        run_migrations(&mut conn).expect("run sqlite migrations");
        Arc::new(SqliteBackend::new(conn))
    }

    #[tokio::test]
    async fn lagged_receiver_emits_slow_consumer_comment() {
        let backend = in_memory_backend();
        let storage: Arc<dyn Storage + Send + Sync> = backend.clone();
        let state = AppState::new_with_event_bus_capacity(
            storage,
            Some(backend),
            0,
            vec!["http://localhost".to_string()],
            8,
            1,
        )
        .expect("build configured app state");

        let publisher = state.clone();
        let mut stream = std::pin::pin!(build_event_stream(state, None));

        for _ in 0..32 {
            publisher.publish_entity_created("person", EntityId::new(), "load-test");
        }

        let event = stream
            .next()
            .await
            .expect("stream should yield lagged comment")
            .expect("stream item should be ok");
        let _ = event;

        assert!(
            stream.next().await.is_none(),
            "lagged receiver should emit one terminal item and then close"
        );
    }
}
