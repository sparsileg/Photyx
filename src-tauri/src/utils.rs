// utils.rs — Shared utility functions
// Spec §7.11

use std::path::{Path, PathBuf};

/// Resolve a path against the active directory if it is relative.
/// Absolute paths are returned unchanged.
/// Relative paths are resolved against `active_directory`.
pub fn resolve_path(path: &str, active_directory: Option<&str>) -> String {
    let p = Path::new(path);
    if p.is_absolute() {
        path.replace('\\', "/")
    } else if let Some(dir) = active_directory {
        PathBuf::from(dir)
            .join(path)
            .to_string_lossy()
            .replace('\\', "/")
    } else {
        path.replace('\\', "/")
    }
}

// ----------------------------------------------------------------------
