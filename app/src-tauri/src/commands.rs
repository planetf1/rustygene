use crate::state::RuntimeState;
use crate::{bootstrap_embedded_api, resolve_data_dir};
use rfd::FileDialog;
use tauri::State;

#[tauri::command]
pub fn write_binary_file(path: String, bytes: Vec<u8>) -> Result<(), String> {
    std::fs::write(&path, bytes).map_err(|err| format!("failed to write file '{}': {err}", path))
}

#[tauri::command]
pub fn read_binary_file(path: String) -> Result<Vec<u8>, String> {
    std::fs::read(&path).map_err(|err| format!("failed to read file '{}': {err}", path))
}

#[tauri::command]
pub fn create_database_backup(destination_path: String) -> Result<(), String> {
    let data_dir = resolve_data_dir();
    let db_path = data_dir.join("rustygene.db");

    if !db_path.exists() {
        return Err(format!(
            "database file does not exist at {}",
            db_path.display()
        ));
    }

    std::fs::copy(&db_path, &destination_path).map_err(|err| {
        format!(
            "failed to copy database from {} to {}: {err}",
            db_path.display(),
            destination_path
        )
    })?;

    Ok(())
}

#[tauri::command]
pub async fn restore_database_backup(
    state: State<'_, RuntimeState>,
    source_path: String,
) -> Result<(), String> {
    let source = std::path::PathBuf::from(&source_path);
    if !source.exists() {
        return Err(format!("backup file does not exist: {}", source.display()));
    }

    {
        let mut handle_guard = state.server_handle.lock().await;
        if let Some(handle) = handle_guard.take() {
            handle
                .shutdown()
                .await
                .map_err(|err| format!("failed to stop embedded API before restore: {err}"))?;
        }
    }

    let data_dir = resolve_data_dir();
    std::fs::create_dir_all(&data_dir).map_err(|err| {
        format!(
            "failed to ensure data directory {}: {err}",
            data_dir.display()
        )
    })?;

    let db_path = data_dir.join("rustygene.db");
    std::fs::copy(&source, &db_path).map_err(|err| {
        format!(
            "failed to restore backup from {} to {}: {err}",
            source.display(),
            db_path.display()
        )
    })?;

    let server = bootstrap_embedded_api().await?;
    let port = server.local_addr.port();

    *state.api_port.write().await = Some(port);
    *state.server_handle.lock().await = Some(server);

    Ok(())
}

#[tauri::command]
pub async fn get_api_port(state: State<'_, RuntimeState>) -> Result<u16, String> {
    state
        .api_port
        .read()
        .await
        .ok_or_else(|| "Embedded API is not ready yet".to_string())
}

#[tauri::command]
pub fn open_file_dialog(title: String, filters: Vec<String>) -> Option<String> {
    let mut dialog = FileDialog::new().set_title(&title);

    let extensions = filters
        .iter()
        .map(|value| value.trim_start_matches('.').to_string())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();

    if !extensions.is_empty() {
        let extension_refs = extensions.iter().map(String::as_str).collect::<Vec<_>>();
        dialog = dialog.add_filter("Allowed", &extension_refs);
    }

    dialog
        .pick_file()
        .map(|path| path.to_string_lossy().to_string())
}

#[tauri::command]
pub fn save_file_dialog(title: String, default_name: String) -> Option<String> {
    FileDialog::new()
        .set_title(&title)
        .set_file_name(&default_name)
        .save_file()
        .map(|path| path.to_string_lossy().to_string())
}
