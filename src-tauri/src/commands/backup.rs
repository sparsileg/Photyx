// commands/backup.rs — Database backup and restore Tauri command handlers

use flate2::write::GzEncoder;
use flate2::read::GzDecoder;
use flate2::Compression;
use std::io::{Read, Write};
use std::path::PathBuf;
use tauri::State;
use crate::PhotoxState;
use crate::db;

#[tauri::command]
pub fn backup_database(state: State<PhotoxState>) -> Result<String, String> {
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

    // Write SQLite backup to a temp .db file first
    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let tmp_path  = backup_dir.join(format!("photyx_backup_{}.tmp.db", timestamp));
    let gz_path   = backup_dir.join(format!("photyx_backup_{}.db.gz", timestamp));

    {
        let db = state.db.lock().expect("db lock poisoned");
        // Checkpoint WAL to ensure all pending writes are in the main DB file
        db.execute_batch("PRAGMA wal_checkpoint(FULL);")
            .map_err(|e| format!("WAL checkpoint failed: {}", e))?;
        let mut dst = rusqlite::Connection::open(&tmp_path)
            .map_err(|e| format!("Failed to create temp backup file: {}", e))?;
        let backup = rusqlite::backup::Backup::new(&db, &mut dst)
            .map_err(|e| format!("Failed to initialize backup: {}", e))?;
        backup.run_to_completion(100, std::time::Duration::from_millis(5), None)
            .map_err(|e| format!("Backup failed: {}", e))?;
    }

    // Compress the temp file to .db.gz
    {
        let raw = std::fs::read(&tmp_path)
            .map_err(|e| format!("Failed to read temp backup: {}", e))?;
        let gz_file = std::fs::File::create(&gz_path)
            .map_err(|e| format!("Failed to create compressed backup: {}", e))?;
        let mut encoder = GzEncoder::new(gz_file, Compression::best());
        encoder.write_all(&raw)
            .map_err(|e| format!("Compression failed: {}", e))?;
        encoder.finish()
            .map_err(|e| format!("Compression finalisation failed: {}", e))?;
    }

    // Remove temp file
    let _ = std::fs::remove_file(&tmp_path);

    // Trim old backups — only count .db.gz files
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
                s.ends_with(".db.gz")
            })
            .collect();
        backups.sort_by_key(|e| std::cmp::Reverse(e.file_name()));
        for old in backups.iter().skip(max) {
            let _ = std::fs::remove_file(old.path());
        }
    }

    tracing::info!("Database backed up and compressed to {:?}", gz_path);
    Ok(gz_path.to_str().unwrap_or("").replace('\\', "/"))
}

#[tauri::command]
pub fn restore_database(backup_path: String, state: State<PhotoxState>) -> Result<(), String> {
    let src = PathBuf::from(&backup_path);
    if !src.exists() {
        return Err(format!("Backup file not found: {}", backup_path));
    }

    let db_path = dirs_next::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Photyx")
        .join("photyx.db");

    // Decompress if .gz, otherwise treat as raw .db
    let db_bytes = if backup_path.ends_with(".gz") {
        let gz_data = std::fs::read(&src)
            .map_err(|e| format!("Failed to read backup file: {}", e))?;
        let mut decoder = GzDecoder::new(gz_data.as_slice());
        let mut decompressed = Vec::new();
        decoder.read_to_end(&mut decompressed)
            .map_err(|e| format!("Decompression failed: {}", e))?;
        decompressed
    } else {
        std::fs::read(&src)
            .map_err(|e| format!("Failed to read backup file: {}", e))?
    };

    // Acquire DB lock for the entire restore operation
    let mut db = state.db.lock().expect("db lock poisoned");

    // Checkpoint WAL before replacing the file
    let _ = db.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");

    std::fs::write(&db_path, &db_bytes)
        .map_err(|e| format!("Restore failed: {}", e))?;

    // Remove WAL and SHM — leftover WAL would replay old data over the restore
    let _ = std::fs::remove_file(db_path.with_extension("db-wal"));
    let _ = std::fs::remove_file(db_path.with_extension("db-shm"));

    // Reopen the connection in-place so the running app sees the restored data
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
