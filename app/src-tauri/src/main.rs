mod commands;
mod state;

use crate::state::RuntimeState;
use rusqlite::Connection;
use rustygene_api::{AppState, ServerHandle};
use rustygene_storage::run_migrations;
use rustygene_storage::sqlite_impl::SqliteBackend;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::Manager;

pub(crate) fn resolve_data_dir() -> PathBuf {
    if let Ok(value) = std::env::var("RUSTYGENE_DATA_DIR") {
        return PathBuf::from(value);
    }

    if let Some(mut path) = dirs::data_local_dir() {
        path.push("rustygene");
        return path;
    }

    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".rustygene")
}

pub(crate) async fn bootstrap_embedded_api() -> Result<ServerHandle, String> {
    let data_dir = resolve_data_dir();
    fs::create_dir_all(&data_dir).map_err(|err| {
        format!(
            "failed to create data directory {}: {err}",
            data_dir.display()
        )
    })?;

    let db_path = data_dir.join("rustygene.db");
    let mut connection = Connection::open(&db_path).map_err(|err| {
        format!(
            "failed to open sqlite database {}: {err}",
            db_path.display()
        )
    })?;

    run_migrations(&mut connection).map_err(|err| format!("migration failure: {err}"))?;

    let backend = Arc::new(SqliteBackend::new(connection));
    let app_state = AppState::with_default_cors_sqlite(backend, 0)
        .map_err(|err| format!("failed to create API state: {}", err.message()))?;

    rustygene_api::start_server(app_state, 0)
        .await
        .map_err(|err| format!("failed to start embedded API: {}", err.message()))
}

fn main() {
    tauri::Builder::default()
        .manage(RuntimeState::default())
        .setup(|app| {
            let runtime_state = app.state::<RuntimeState>();
            let api_port = runtime_state.api_port.clone();
            let server_handle = runtime_state.server_handle.clone();

            tauri::async_runtime::spawn(async move {
                match bootstrap_embedded_api().await {
                    Ok(server) => {
                        let port = server.local_addr.port();
                        *api_port.write().await = Some(port);
                        *server_handle.lock().await = Some(server);
                    }
                    Err(error) => {
                        eprintln!("embedded API bootstrap failed: {error}");
                    }
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_api_port,
            commands::open_file_dialog,
            commands::save_file_dialog,
            commands::write_binary_file,
            commands::create_database_backup,
            commands::restore_database_backup
        ])
        .run(tauri::generate_context!())
        .expect("failed to run RustyGene desktop shell");
}
