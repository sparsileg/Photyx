// commands/macros.rs — Macro file management Tauri command handlers

use tauri::State;
use crate::PhotoxState;

#[tauri::command]
pub fn get_macros_dir() -> String {
    crate::utils::get_macros_dir()
        .to_str()
        .unwrap_or("")
        .replace('\\', "/")
}

#[tauri::command]
pub fn list_macros(state: State<PhotoxState>) -> Result<Vec<serde_json::Value>, String> {
    let macros_path = {
        let _ctx = state.context.lock().expect("context lock poisoned");
        crate::utils::get_macros_dir()
    };

    if !macros_path.exists() {
        std::fs::create_dir_all(&macros_path)
            .map_err(|e| format!("Failed to create Macros directory: {}", e))?;
    }

    let mut entries: Vec<serde_json::Value> = std::fs::read_dir(&macros_path)
        .map_err(|e| e.to_string())?
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path  = entry.path();
            if path.extension()?.to_str()? != "phs" { return None; }
            let filename  = path.file_name()?.to_str()?.to_string();
            let stem      = path.file_stem()?.to_str()?.to_string();
            let full_path = path.to_str()?.to_string();
            let tooltip   = extract_macro_tooltip(&full_path);
            let lines     = std::fs::read_to_string(&full_path)
                .map(|s| s.lines().count())
                .unwrap_or(0);
            Some(serde_json::json!({
                "name":     stem,
                "filename": filename,
                "path":     full_path,
                "lines":    lines,
                "tooltip":  tooltip,
            }))
        })
        .collect();

    entries.sort_by(|a, b| {
        let na = a["name"].as_str().unwrap_or("");
        let nb = b["name"].as_str().unwrap_or("");
        na.cmp(nb)
    });

    Ok(entries)
}

pub fn extract_macro_tooltip(path: &str) -> String {
    let Ok(contents) = std::fs::read_to_string(path) else { return String::new() };
    let lines: Vec<&str> = contents.lines()
        .take_while(|l| l.trim().starts_with('#') || l.trim().is_empty())
        .filter(|l| l.trim().starts_with('#'))
        .map(|l| l.trim().trim_start_matches('#').trim())
        .collect();
    lines.join("\n")
}

#[tauri::command]
pub fn rename_macro(old_path: String, new_name: String) -> Result<String, String> {
    let old = std::path::PathBuf::from(&old_path);
    let dir = old.parent()
        .ok_or_else(|| "Cannot determine macro directory".to_string())?;
    let safe = new_name
        .replace(|c: char| !c.is_alphanumeric() && c != '_' && c != '-' && c != ' ', "")
        .trim()
        .to_string();
    if safe.is_empty() {
        return Err("Invalid macro name".to_string());
    }
    let new_path = dir.join(format!("{}.phs", safe));
    if new_path.exists() {
        return Err(format!("A macro named '{}' already exists", safe));
    }
    std::fs::rename(&old, &new_path)
        .map_err(|e| format!("Rename failed: {}", e))?;
    Ok(new_path.to_str().unwrap_or("").replace('\\', "/"))
}

#[tauri::command]
pub fn delete_macro(path: String) -> Result<(), String> {
    std::fs::remove_file(&path)
        .map_err(|e| format!("Failed to delete macro: {}", e))
}

// ----------------------------------------------------------------------
