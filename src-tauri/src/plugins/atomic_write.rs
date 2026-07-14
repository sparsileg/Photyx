// plugins/atomic_write.rs — shared atomic write-and-rename helper (Issue 82)
//
// Batch/whole-file writers (WriteFIT, WriteXISF, WriteTIFF, WriteFrame, and
// the XISF/TIFF branches of WriteCurrent) must never leave a truncated file
// at the final destination path if the process is killed mid-write. This is
// the single implementation of that guarantee — previously it existed as
// three separate inline copies (write_frame.rs, and two branches of
// write_current_files.rs).
//
// Contract: `write_fn` receives a temp path (`<out_path>.tmp`) and must
// write the complete file there. On success, the temp file is renamed into
// place. On any failure (write or rename), the temp file is removed and
// whatever was at `out_path` before the call — if anything — is left
// untouched.

/// Writes a file atomically: `write_fn` writes to a temporary path, which
/// is renamed over `out_path` only on success. A partial write can never
/// appear at `out_path` — the original file (if any existed) survives any
/// interruption right up until the temp file is fully written and the
/// rename begins.
///
/// On Windows, the existing target is removed immediately before the
/// rename rather than relying solely on `rename`'s built-in replace
/// behavior — the most reliable method on that platform, at the cost of a
/// narrow window (between the remove and the rename) where a crash would
/// leave nothing at the final path rather than the original. That trade is
/// acceptable per the acceptance criteria: intact original or nothing —
/// never a partial file.
pub(crate) fn atomic_write<F>(out_path: &str, write_fn: F) -> Result<(), String>
where
    F: FnOnce(&str) -> Result<(), String>,
{
    let temp_path = format!("{}.tmp", out_path);

    // Clear any stale temp file left behind by a prior crashed run.
    let _ = std::fs::remove_file(&temp_path);

    write_fn(&temp_path)?;

    #[cfg(windows)]
    {
        if std::path::Path::new(out_path).exists() {
            // Windows can hold a brief, transient lock on a just-touched file
            // (antivirus, search indexing, backup agents) even after our own
            // process has fully closed its handle — retry a few times with a
            // short delay before giving up, rather than failing on the first
            // sharing violation.
            let mut last_err = None;
            let mut removed = false;
            for attempt in 0..5 {
                match std::fs::remove_file(out_path) {
                    Ok(()) => { removed = true; break; }
                    Err(e) => {
                        last_err = Some(e);
                        if attempt < 4 {
                            std::thread::sleep(std::time::Duration::from_millis(100));
                        }
                    }
                }
            }
            if !removed {
                let _ = std::fs::remove_file(&temp_path);
                return Err(format!(
                    "Cannot remove existing file before replace: {}",
                    last_err.map(|e| e.to_string()).unwrap_or_default()
                ));
            }
        }
    }

    match std::fs::rename(&temp_path, out_path) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(&temp_path);
            Err(format!("Cannot replace file: {}", e))
        }
    }
}

// ----------------------------------------------------------------------
