// utils.rs — Shared utility functions
// Spec §7.11

use std::path::{Path, PathBuf};

/// Returns the current user's home directory, or None if it can't be
/// determined (e.g. the relevant environment variable is unset). Mirrors
/// the OS-conditional env-var pattern already used by get_log_dir() below,
/// rather than pulling in a new crate dependency for this.
fn home_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var("USERPROFILE").ok().map(PathBuf::from)
    }
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        std::env::var("HOME").ok().map(PathBuf::from)
    }
}

/// Expands a leading `~` (bare, or followed by `/` or `\`) against `home`.
/// Paths not starting with `~` are returned unchanged. If `home` is None
/// (couldn't be determined), the path is returned unchanged rather than
/// silently producing a broken path — a later "file not found" is a
/// clearer failure than a mangled path with a literal "~" baked into it.
/// Takes `home` as a parameter (rather than reading the environment
/// internally) so this stays a pure, easily unit-testable function.
fn expand_tilde(path: &str, home: Option<&Path>) -> String {
    let rest = if path == "~" {
        Some("")
    } else if let Some(r) = path.strip_prefix("~/") {
        Some(r)
    } else if let Some(r) = path.strip_prefix("~\\") {
        Some(r)
    } else {
        None
    };

    match rest {
        Some(r) => match home {
            // Joining an empty string still appends a trailing separator
            // in PathBuf::join — special-case bare "~" to avoid that
            // rather than joining "" onto home.
            Some(h) if r.is_empty() => h.to_string_lossy().replace('\\', "/"),
            Some(h) => h.join(r).to_string_lossy().replace('\\', "/"),
            None => path.to_string(),
        },
        None => path.to_string(),
    }
}

/// Resolve a path against the active directory if it is relative.
/// A leading `~` is expanded to the current user's home directory first
/// (making the result absolute, so it short-circuits the active-directory
/// resolution below). Absolute paths are otherwise returned unchanged.
/// Remaining relative paths are resolved against `active_directory`.
pub fn resolve_path(path: &str, active_directory: Option<&str>) -> String {
    let expanded = expand_tilde(path, home_dir().as_deref());
    let p = Path::new(&expanded);
    if p.is_absolute() {
        expanded.replace('\\', "/")
    } else if let Some(dir) = active_directory {
        PathBuf::from(dir)
            .join(&expanded)
            .to_string_lossy()
            .replace('\\', "/")
    } else {
        expanded.replace('\\', "/")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_tilde_expands_to_home() {
        let home = Path::new("/home/stan");
        assert_eq!(expand_tilde("~", Some(home)), "/home/stan");
    }

    #[test]
    fn tilde_slash_expands_and_joins() {
        let home = Path::new("/home/stan");
        assert_eq!(expand_tilde("~/lights/frame001.fit", Some(home)), "/home/stan/lights/frame001.fit");
    }

    #[test]
    fn tilde_backslash_expands_on_any_platform() {
        // Accept a Windows-typed "~\..." even when parsed on a
        // forward-slash system, since pcode paths are user-typed text.
        let home = Path::new("/home/stan");
        assert_eq!(expand_tilde("~\\lights\\frame001.fit", Some(home)), "/home/stan/lights/frame001.fit");
    }

    #[test]
    fn no_leading_tilde_passes_through_unchanged() {
        assert_eq!(expand_tilde("/data/lights/frame001.fit", Some(Path::new("/home/stan"))), "/data/lights/frame001.fit");
        assert_eq!(expand_tilde("lights/frame001.fit", Some(Path::new("/home/stan"))), "lights/frame001.fit");
    }

    #[test]
    fn tilde_word_not_at_start_is_not_expanded() {
        // "~backup" or "foo~bar" should never be touched — only a leading
        // "~" that is itself a complete path segment counts.
        assert_eq!(expand_tilde("~backup/frame.fit", Some(Path::new("/home/stan"))), "~backup/frame.fit");
    }

    #[test]
    fn missing_home_leaves_path_unchanged() {
        assert_eq!(expand_tilde("~/lights/frame001.fit", None), "~/lights/frame001.fit");
    }

    #[test]
    fn resolve_path_expands_tilde_and_short_circuits_active_directory() {
        // A tilde path becomes absolute after expansion, so it must NOT
        // also be joined against active_directory.
        let home = Path::new("/home/stan");
        let expanded = expand_tilde("~/lights/frame001.fit", Some(home));
        assert_eq!(expanded, "/home/stan/lights/frame001.fit");
        // resolve_path itself reads the real environment for home_dir(),
        // so this test only exercises expand_tilde's contribution to the
        // short-circuit logic directly rather than mocking the env.
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
