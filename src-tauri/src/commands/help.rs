// commands/help.rs

use tauri::{AppHandle, Manager};
use tauri_plugin_opener::OpenerExt;

#[tauri::command]
pub fn open_documentation(app: AppHandle) -> Result<(), String> {
    let index_path = app
        .path()
        .resolve("docs/index.html", tauri::path::BaseDirectory::Resource)
        .map_err(|e| format!("Failed to resolve docs path: {}", e))?;

    let path_str = index_path
        .to_str()
        .ok_or_else(|| "Docs path is not valid UTF-8".to_string())?;

    app.opener()
        .open_path(path_str, None::<&str>)
        .map_err(|e| format!("Failed to open documentation: {}", e))
}

// ----------------------------------------------------------------------
// ----------------------------------------------------------------------
// ----------------------------------------------------------------------
