// logging.rs — Structured application logging
// Spec §10

use std::path::PathBuf;
use tracing::info;
use tracing_appender::rolling;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Initialise logging. Call once at startup before anything else.
/// Returns the worker guard — must be kept alive for the duration of the app.
pub fn init_logging() -> tracing_appender::non_blocking::WorkerGuard {
    let log_dir = get_log_dir();
    std::fs::create_dir_all(&log_dir).expect("Failed to create log directory");

    // Rolling file appender — new file per session, prefix photyx, suffix .log
    let file_appender = rolling::never(&log_dir, session_log_filename());

    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    // Log level from environment or default to INFO in release, DEBUG in dev
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        if cfg!(debug_assertions) {
            EnvFilter::new("debug")
        } else {
            EnvFilter::new("info")
        }
    });

    // Two layers: file (structured) + stderr (dev only)
    let file_layer = fmt::layer()
        .with_writer(non_blocking)
        .with_ansi(false)
        .with_target(true)
        .with_thread_ids(false);

    #[cfg(debug_assertions)]
    let registry = tracing_subscriber::registry()
        .with(filter)
        .with(file_layer)
        .with(fmt::layer().with_writer(std::io::stderr).with_target(true));

    #[cfg(not(debug_assertions))]
    let registry = tracing_subscriber::registry()
        .with(filter)
        .with(file_layer);

    registry.init();

    // Prune old log files — keep last 10 per spec §10
    prune_old_logs(&log_dir, 10);

    info!("Photyx logging initialised. Log dir: {}", log_dir.display());

    guard
}

/// Returns the OS-appropriate log directory per spec §10
fn get_log_dir() -> PathBuf {
    crate::utils::get_log_dir()
}

/// Generate a log filename for this session: photyx_YYYY-MM-DD_HH-MM-SS.log
fn session_log_filename() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Simple timestamp without chrono dependency
    // Format: photyx_<unix_timestamp>.log
    format!("photyx_{}.log", now)
}

/// Remove oldest log files, keeping only the most recent `keep` files
fn prune_old_logs(log_dir: &PathBuf, keep: usize) {
    let Ok(entries) = std::fs::read_dir(log_dir) else { return };

    let mut files: Vec<(std::time::SystemTime, PathBuf)> = entries
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .map(|x| x == "log")
                .unwrap_or(false)
        })
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            let modified = meta.modified().ok()?;
            Some((modified, e.path()))
        })
        .collect();

    if files.len() <= keep {
        return;
    }

    // Sort oldest first
    files.sort_by_key(|(t, _)| *t);

    let to_delete = files.len() - keep;
    for (_, path) in files.iter().take(to_delete) {
        if let Err(e) = std::fs::remove_file(path) {
            eprintln!("Failed to prune log file {:?}: {}", path, e);
        }
    }
}
