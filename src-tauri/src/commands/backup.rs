// commands/backup.rs — Database backup and restore Tauri command handlers

use std::sync::Arc;
use std::io::{Read, Write};
use std::path::PathBuf;
use tauri::State;
use crate::PhotoxState;
use crate::db;

#[tauri::command]
pub fn backup_database(state: State<Arc<PhotoxState>>) -> Result<String, String> {
    // Get backup directory from preferences, fall back to default
    let backup_dir: PathBuf = {
        let db = state.db.lock().expect("db lock poisoned");
        let dir: Option<String> = db.query_row(
            "SELECT value FROM preferences WHERE key = 'backup_directory'",
            [],
            |row| row.get(0),
        ).ok();
        match dir {
            Some(d) if !d.is_empty() => PathBuf::from(d),
            _ => dirs_next::data_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("Photyx")
                .join("backups"),
        }
    };

    std::fs::create_dir_all(&backup_dir)
        .map_err(|e| format!("Failed to create backup directory: {}", e))?;

    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S").to_string();
    let zip_path  = backup_dir.join(format!("photyx_backup_{}.zip", timestamp));
    let tmp_db    = backup_dir.join(format!("photyx_backup_{}.tmp.db", timestamp));

    // ── Step 1: SQLite backup to temp file ───────────────────────────────────
    {
        let db = state.db.lock().expect("db lock poisoned");
        db.execute_batch("PRAGMA wal_checkpoint(FULL);")
            .map_err(|e| format!("WAL checkpoint failed: {}", e))?;
        let mut dst = rusqlite::Connection::open(&tmp_db)
            .map_err(|e| format!("Failed to create temp backup file: {}", e))?;
        let backup = rusqlite::backup::Backup::new(&db, &mut dst)
            .map_err(|e| format!("Failed to initialize backup: {}", e))?;
        backup.run_to_completion(100, std::time::Duration::from_millis(5), None)
            .map_err(|e| format!("Backup failed: {}", e))?;
    }

    // ── Step 2: Read all macros from DB ───────────────────────────────────────
    let macros: Vec<(String, String)> = {
        let db = state.db.lock().expect("db lock poisoned");
        let mut stmt = db.prepare(
            "SELECT display_name, script FROM macros ORDER BY display_name COLLATE NOCASE"
        ).map_err(|e| e.to_string())?;
        let rows: Vec<(String, String)> = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
            .map_err(|e| e.to_string())?
            .filter_map(|r| r.ok())
            .collect();
        rows
    };

    // ── Step 3: Build zip archive ─────────────────────────────────────────────
    {
        let zip_file = std::fs::File::create(&zip_path)
            .map_err(|e| format!("Failed to create zip file: {}", e))?;
        let mut zip = zip::ZipWriter::new(zip_file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);

        // Add photyx.db
        let db_bytes = std::fs::read(&tmp_db)
            .map_err(|e| format!("Failed to read temp backup: {}", e))?;
        zip.start_file("photyx.db", options)
            .map_err(|e| format!("Failed to add DB to zip: {}", e))?;
        zip.write_all(&db_bytes)
            .map_err(|e| format!("Failed to write DB to zip: {}", e))?;

        // Add macros/ directory entries
        for (display_name, script) in &macros {
            // Sanitise display_name for use as a filename
            let safe_name: String = display_name
                .chars()
                .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' || c == ' ' { c } else { '_' })
                .collect();
            let entry_name = format!("macros/{}.phs", safe_name);
            zip.start_file(&entry_name, options)
                .map_err(|e| format!("Failed to add macro '{}' to zip: {}", display_name, e))?;
            zip.write_all(script.as_bytes())
                .map_err(|e| format!("Failed to write macro '{}' to zip: {}", display_name, e))?;
        }

        zip.finish()
            .map_err(|e| format!("Failed to finalise zip: {}", e))?;
    }

    // Remove temp DB file
    let _ = std::fs::remove_file(&tmp_db);

    // ── Step 4: Trim old backups ──────────────────────────────────────────────
    let max: usize = {
        let db = state.db.lock().expect("db lock poisoned");
        db.query_row(
            "SELECT CAST(value AS INTEGER) FROM preferences WHERE key = 'backup_max_count'",
            [],
            |row| row.get::<_, i64>(0),
        ).unwrap_or(10) as usize
    };

    if let Ok(entries) = std::fs::read_dir(&backup_dir) {
        let mut backups: Vec<_> = entries
            .filter_map(|e| e.ok())
            .filter(|e| {
                let name = e.file_name();
                let s = name.to_string_lossy();
                s.starts_with("photyx_backup_") && s.ends_with(".zip")
            })
            .collect();
        backups.sort_by_key(|e| std::cmp::Reverse(e.file_name()));
        for old in backups.iter().skip(max) {
            let _ = std::fs::remove_file(old.path());
        }
    }

    tracing::info!("Backup written to {:?} ({} macros included)", zip_path, macros.len());
    Ok(zip_path.to_str().unwrap_or("").replace('\\', "/"))
}

#[tauri::command]
pub fn restore_database(backup_path: String, state: State<Arc<PhotoxState>>) -> Result<(), String> {
    let src = PathBuf::from(&backup_path);
    if !src.exists() {
        return Err(format!("Backup file not found: {}", backup_path));
    }

    let db_path = dirs_next::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Photyx")
        .join("photyx.db");

    // Extract photyx.db from the zip
    let db_bytes = {
        let zip_file = std::fs::File::open(&src)
            .map_err(|e| format!("Failed to open backup zip: {}", e))?;
        let mut archive = zip::ZipArchive::new(zip_file)
            .map_err(|e| format!("Failed to read zip archive: {}", e))?;
        let mut db_entry = archive.by_name("photyx.db")
            .map_err(|_| "Backup zip does not contain photyx.db".to_string())?;
        let mut bytes = Vec::new();
        db_entry.read_to_end(&mut bytes)
            .map_err(|e| format!("Failed to extract photyx.db: {}", e))?;
        bytes
    };

    // Acquire DB lock for the entire restore operation
    let mut db = state.db.lock().expect("db lock poisoned");

    // Checkpoint WAL before replacing the file
    let _ = db.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");

    std::fs::write(&db_path, &db_bytes)
        .map_err(|e| format!("Restore failed: {}", e))?;

    // Remove WAL and SHM
    let _ = std::fs::remove_file(db_path.with_extension("db-wal"));
    let _ = std::fs::remove_file(db_path.with_extension("db-shm"));

    // Reopen the connection in-place
    let new_conn = db::open_db(
        dirs_next::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Photyx")
    ).map_err(|e| format!("Failed to reopen database after restore: {}", e))?;

    *db = new_conn;

    tracing::info!("Database restored from {:?} and connection reopened", src);
    Ok(())
}

// ----------------------------------------------------------------------
