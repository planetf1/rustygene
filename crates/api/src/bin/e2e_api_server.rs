use std::path::PathBuf;
use std::sync::Arc;

use rustygene_api::{AppState, start_server};
use rustygene_gedcom::import_gedcom_to_sqlite;
use rustygene_storage::run_migrations;
use rustygene_storage::sqlite_impl::SqliteBackend;

#[tokio::main]
async fn main() {
    let api_port: u16 = std::env::var("RUSTYGENE_E2E_API_PORT")
        .ok()
        .and_then(|v| v.parse::<u16>().ok())
        .unwrap_or(3000);

    let db_path = std::env::var("RUSTYGENE_E2E_DB_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir().join("rustygene-e2e-seeded.sqlite"));

    if db_path.exists() {
        std::fs::remove_file(&db_path)
            .unwrap_or_else(|e| panic!("failed to remove existing db {:?}: {e}", db_path));
    }

    let mut conn = rusqlite::Connection::open(&db_path)
        .unwrap_or_else(|e| panic!("failed to open sqlite db {:?}: {e}", db_path));
    run_migrations(&mut conn).unwrap_or_else(|e| panic!("failed to run migrations: {e}"));

    let kennedy = include_str!("../../../../testdata/gedcom/kennedy.ged");
    import_gedcom_to_sqlite(&mut conn, "e2e-kennedy-seed", kennedy)
        .unwrap_or_else(|e| panic!("failed to import kennedy fixture into e2e db: {e}"));

    let backend = Arc::new(SqliteBackend::new(conn));
    let state =
        AppState::with_default_cors_sqlite(backend, 0).expect("build app state for e2e API server");

    let server = start_server(state, api_port)
        .await
        .unwrap_or_else(|e| panic!("failed to start e2e API server: {e:?}"));

    eprintln!(
        "rustygene e2e api server listening on {} (db: {:?})",
        server.local_addr, db_path
    );

    tokio::signal::ctrl_c()
        .await
        .expect("failed to listen for ctrl-c signal");

    server
        .shutdown()
        .await
        .expect("failed to shutdown e2e API server");
}
