use std::path::PathBuf;
use std::sync::Arc;

use rustygene_api::{AppState, start_server};
use rustygene_storage::run_migrations;
use rustygene_storage::sqlite_impl::SqliteBackend;

#[tokio::main]
async fn main() {
    // 1. Load configuration
    let db_path_env = std::env::var("RUSTYGENE_DB_PATH").ok();
    let port: u16 = std::env::var("RUSTYGENE_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3000);

    let cors_origins = std::env::var("RUSTYGENE_CORS_ORIGINS")
        .ok()
        .map(|v| v.split(',').map(|s| s.trim().to_string()).collect::<Vec<_>>());

    // 2. Validate DB Path
    let Some(db_path_str) = db_path_env else {
        eprintln!("[FATAL] RUSTYGENE_DB_PATH is not set.");
        eprintln!("       Fix: set RUSTYGENE_DB_PATH to the desired SQLite file location.");
        std::process::exit(1);
    };

    let db_path = PathBuf::from(db_path_str);
    if let Some(parent) = db_path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            eprintln!("[FATAL] Database directory {:?} does not exist.", parent);
            eprintln!("       Fix: create the directory or point to an existing one.");
            std::process::exit(1);
        }
    }

    // 3. Open DB and run migrations
    let mut conn = match rusqlite::Connection::open(&db_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[FATAL] Cannot open database at {:?}: {}", db_path, e);
            eprintln!("       Fix: ensure the file is readable/writable and not locked.");
            std::process::exit(1);
        }
    };

    if let Err(e) = run_migrations(&mut conn) {
        eprintln!("[FATAL] Database migration failed: {}", e);
        eprintln!("       Fix: ensure DB schema is compatible and disk space is sufficient.");
        std::process::exit(1);
    }

    // 4. Build AppState
    let backend = Arc::new(SqliteBackend::new(conn));
    
    let state = if let Some(origins) = cors_origins {
        AppState::new(backend.clone(), Some(backend), port, origins)
    } else {
        AppState::with_default_cors_sqlite(backend, port)
    }.expect("failed to build app state");

    // 5. Start Server
    let server = match start_server(state, port).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[FATAL] Server bind failed on port {}: {:?}", port, e);
            std::process::exit(1);
        }
    };

    eprintln!("RustyGene API Server listening on {}", server.local_addr);

    // 6. Graceful Shutdown
    tokio::signal::ctrl_c()
        .await
        .expect("failed to listen for ctrl-c signal");

    eprintln!("Shutting down...");
    if let Err(e) = server.shutdown().await {
        eprintln!("[ERROR] Graceful shutdown failed: {}", e);
    }
}
