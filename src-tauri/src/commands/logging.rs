// commands/logging.rs — Log file Tauri command handlers

use tauri::State;
use crate::PhotoxState;

#[tauri::command]
pub fn list_log_files(state: State<PhotoxState>) -> Result<Vec<serde_json::Value>, String> {
    let log_dir = {
        let ctx = state.context.lock().expect("context lock poisoned");
        ctx.log_dir.clone()
    };

    let log_path = if let Some(dir) = log_dir {
        std::path::PathBuf::from(dir)
    } else {
        crate::utils::get_log_dir()
    };

    if !log_path.exists() {
        return Ok(vec![]);
    }

    let mut entries: Vec<serde_json::Value> = std::fs::read_dir(&log_path)
        .map_err(|e| e.to_string())?
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path  = entry.path();
            if path.extension()?.to_str()? != "log" { return None; }
            let meta     = std::fs::metadata(&path).ok()?;
            let modified = meta.modified().ok()?;
            let modified_secs = modified
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let size = meta.len();
            let filename = path.file_name()?.to_str()?.to_string();
            let full_path = path.to_str()?.to_string();
            Some(serde_json::json!({
                "filename":      filename,
                "path":          full_path,
                "size":          size,
                "modified_secs": modified_secs,
            }))
        })
        .collect();

    entries.sort_by(|a, b| {
        let ta = a["modified_secs"].as_u64().unwrap_or(0);
        let tb = b["modified_secs"].as_u64().unwrap_or(0);
        tb.cmp(&ta)
    });

    Ok(entries)
}

#[tauri::command]
pub fn read_log_file(path: String) -> Result<Vec<serde_json::Value>, String> {
    let contents = std::fs::read_to_string(&path)
        .map_err(|e| format!("Cannot read log file: {}", e))?;
    Ok(contents.lines().map(parse_log_line).collect())
}

pub fn parse_log_line(line: &str) -> serde_json::Value {
    if let Some(z_pos) = line.find("Z ") {
        let timestamp = &line[..z_pos + 1];
        let rest = line[z_pos + 1..].trim();

        let (level, remainder) = if rest.starts_with("ERROR") {
            ("ERROR", rest[5..].trim())
        } else if rest.starts_with("WARN") {
            ("WARN", rest[4..].trim())
        } else if rest.starts_with("INFO") {
            ("INFO", rest[4..].trim())
        } else if rest.starts_with("DEBUG") {
            ("DEBUG", rest[5..].trim())
        } else if rest.starts_with("TRACE") {
            ("TRACE", rest[5..].trim())
        } else {
            ("INFO", rest)
        };

        let (module, message) = if let Some(colon_pos) = remainder.find(": ") {
            (&remainder[..colon_pos], remainder[colon_pos + 2..].trim())
        } else {
            ("", remainder)
        };

        serde_json::json!({
            "timestamp": timestamp,
            "level":     level,
            "module":    module,
            "message":   message,
            "raw":       line,
        })
    } else {
        serde_json::json!({
            "timestamp": "",
            "level":     "RAW",
            "module":    "",
            "message":   line,
            "raw":       line,
        })
    }
}


// ----------------------------------------------------------------------
