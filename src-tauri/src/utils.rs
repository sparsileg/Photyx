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

/// Returns the OS-appropriate Photyx macros directory per spec §7.10.
pub fn get_macros_dir() -> std::path::PathBuf {
    #[cfg(target_os = "windows")]
    {
        let appdata = std::env::var("APPDATA").unwrap_or_else(|_| ".".to_string());
        std::path::PathBuf::from(appdata).join("Photyx").join("Macros")
    }
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        std::path::PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join("Photyx")
            .join("Macros")
    }
    #[cfg(target_os = "linux")]
    {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        std::path::PathBuf::from(home).join(".config").join("Photyx").join("Macros")
    }
}

/// Returns the OS-appropriate Photyx log directory per spec §10.
/// Mirrors the logic in logging.rs — must stay in sync.
pub fn get_log_dir() -> std::path::PathBuf {
    #[cfg(target_os = "windows")]
    {
        let appdata = std::env::var("APPDATA").unwrap_or_else(|_| ".".to_string());
        std::path::PathBuf::from(appdata).join("Photyx").join("logs")
    }
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        std::path::PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join("Photyx")
            .join("logs")
    }
    #[cfg(target_os = "linux")]
    {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        std::path::PathBuf::from(home).join(".config").join("Photyx").join("logs")
    }
}

// ----------------------------------------------------------------------
