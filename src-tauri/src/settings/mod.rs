// settings/mod.rs — AppSettings global object.
// Populated at startup from defaults.rs (hard-coded values) and the
// preferences table (persisted user values). All reads come from this
// struct; writes go to both this struct and the DB simultaneously.
// Stored in PhotoxState as Mutex<AppSettings>.

pub mod defaults;
use defaults::*;
use rusqlite::Connection;

/// All application settings in one place.
/// Fields marked "not persisted" are always set from defaults.rs.
/// Fields marked "persisted" are loaded from the DB on startup and
/// written back via save_preference() on change.
#[derive(Debug, Clone)]
pub struct AppSettings {
    // ── UI / Viewer ───────────────────────────────────────────────────
    pub theme:                       String,  // persisted
    // zoom, blink rate, channel view: not persisted — always default
    pub zoom_level:                  String,
    pub blink_rate_secs:             f64,
    pub channel_view:                String,

    // ── File & Path ───────────────────────────────────────────────────
    pub last_directory:              String,  // persisted (last-used, not user pref)
    pub jpeg_quality:                i64,     // persisted, user pref
    pub recent_directories_max:      i64,     // persisted, user pref

    // ── pcode / Macro ─────────────────────────────────────────────────
    pub backup_directory:            String,  // persisted, user pref
    pub console_history_size:        i64,     // persisted, user pref
    pub macro_editor_font_size:      i64,     // persisted, user pref
    pub error_behavior:              String,  // not persisted — always "halt"

    // ── Performance ───────────────────────────────────────────────────
    pub buffer_pool_bytes:           i64,     // persisted, user pref

    // ── AutoStretch ───────────────────────────────────────────────────
    pub autostretch_shadow_clip:     f64,     // persisted, user pref
    pub autostretch_target_bg:       f64,     // persisted, user pref

    // ── Crash Recovery ────────────────────────────────────────────────
    pub crash_recovery_interval_secs: i64,   // persisted, internal

    // ── Active threshold profile ──────────────────────────────────────
    pub active_threshold_profile_id: Option<i64>, // persisted

    // ── Non-persisted runtime constants ──────────────────────────────
    pub display_max_width_px:        u32,
    pub blink_jpeg_quality:          u8,
    pub display_jpeg_quality:        u8,
    pub algorithm_set_version:       i64,
}

impl AppSettings {
    /// Construct with all defaults from defaults.rs.
    /// Call load_from_db() immediately after to apply persisted values.
    pub fn new() -> Self {
        Self {
            theme:                        DEFAULT_THEME.to_string(),
            zoom_level:                   DEFAULT_ZOOM_LEVEL.to_string(),
            blink_rate_secs:              DEFAULT_BLINK_RATE_SECS,
            channel_view:                 DEFAULT_CHANNEL_VIEW.to_string(),
            last_directory:               DEFAULT_LAST_DIRECTORY.to_string(),
            jpeg_quality:                 DEFAULT_JPEG_QUALITY,
            recent_directories_max:       DEFAULT_RECENT_DIRS_MAX,
            backup_directory:             DEFAULT_BACKUP_DIRECTORY.to_string(),
            console_history_size:         DEFAULT_CONSOLE_HISTORY_SIZE,
            macro_editor_font_size:       DEFAULT_MACRO_EDITOR_FONT,
            error_behavior:               DEFAULT_ERROR_BEHAVIOR.to_string(),
            buffer_pool_bytes:            DEFAULT_BUFFER_POOL_BYTES,
            autostretch_shadow_clip:      DEFAULT_AUTOSTRETCH_SHADOW_CLIP,
            autostretch_target_bg:        DEFAULT_AUTOSTRETCH_TARGET_BG,
            crash_recovery_interval_secs: DEFAULT_CRASH_RECOVERY_INTERVAL_SECS,
            active_threshold_profile_id:  None,
            display_max_width_px:         DISPLAY_MAX_WIDTH_PX,
            blink_jpeg_quality:           BLINK_JPEG_QUALITY,
            display_jpeg_quality:         DISPLAY_JPEG_QUALITY,
            algorithm_set_version:        ALGORITHM_SET_VERSION,
        }
    }

    /// Overwrite persisted fields from the preferences table.
    /// Missing keys fall back silently to the defaults already set by new().
    /// Bounds are clamped on read — DB stores raw values.
    pub fn load_from_db(&mut self, db: &Connection) {
        let mut stmt = match db.prepare("SELECT key, value FROM preferences") {
            Ok(s)  => s,
            Err(e) => { tracing::warn!("AppSettings: failed to read preferences: {}", e); return; }
        };
        let rows: Vec<(String, String)> = match stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?))) {
            Ok(mapped) => mapped.filter_map(|r| r.ok()).collect(),
            Err(e) => {
                tracing::warn!("AppSettings: failed to query preferences: {}", e);
                vec![]
            }
        };

        for (key, value) in rows {
            match key.as_str() {
                "theme" => self.theme = value,
                "last_directory" => self.last_directory = value,
                "jpeg_quality" => {
                    if let Ok(v) = value.parse::<i64>() {
                        self.jpeg_quality = v.clamp(JPEG_QUALITY_MIN, JPEG_QUALITY_MAX);
                    }
                }
                "recent_directories_max" => {
                    if let Ok(v) = value.parse::<i64>() {
                        self.recent_directories_max = v.clamp(RECENT_DIRS_MIN, RECENT_DIRS_MAX);
                    }
                }
                "backup_directory" => self.backup_directory = value,
                "console_history_size" => {
                    if let Ok(v) = value.parse::<i64>() {
                        self.console_history_size = v.clamp(CONSOLE_HISTORY_MIN, CONSOLE_HISTORY_MAX);
                    }
                }
                "macro_editor_font_size" => {
                    if let Ok(v) = value.parse::<i64>() {
                        self.macro_editor_font_size = v.clamp(MACRO_EDITOR_FONT_MIN, MACRO_EDITOR_FONT_MAX);
                    }
                }
                "buffer_pool_memory_limit" => {
                    if let Ok(v) = value.parse::<i64>() {
                        self.buffer_pool_bytes = v.clamp(BUFFER_POOL_MIN_BYTES, BUFFER_POOL_MAX_BYTES);
                    }
                }
                "autostretch_shadow_clip" => {
                    if let Ok(v) = value.parse::<f64>() {
                        self.autostretch_shadow_clip = v.clamp(AUTOSTRETCH_SHADOW_CLIP_MIN, AUTOSTRETCH_SHADOW_CLIP_MAX);
                    }
                }
                "autostretch_target_bg" => {
                    if let Ok(v) = value.parse::<f64>() {
                        self.autostretch_target_bg = v.clamp(AUTOSTRETCH_TARGET_BG_MIN, AUTOSTRETCH_TARGET_BG_MAX);
                    }
                }
                "crash_recovery_interval_secs" => {
                    if let Ok(v) = value.parse::<i64>() {
                        self.crash_recovery_interval_secs = v.clamp(CRASH_RECOVERY_INTERVAL_MIN, CRASH_RECOVERY_INTERVAL_MAX);
                    }
                }
                "active_threshold_profile_id" => {
                    self.active_threshold_profile_id = value.parse::<i64>().ok();
                }
                _ => {} // unknown keys ignored — forward-compatible
            }
        }
    }

    /// Write a single persisted preference to the DB and update self.
    /// Bounds are clamped before writing.
    pub fn save_preference(&mut self, key: &str, value: &str, db: &Connection) -> Result<(), String> {
        // Apply to self first (with clamping), then persist the clamped value
        self.apply(key, value);
        let clamped = self.get_as_string(key).unwrap_or_else(|| value.to_string());
        db.execute(
            "INSERT INTO preferences (key, value, updated_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            rusqlite::params![key, clamped, crate::db::now_unix()],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Apply a key/value pair to self (used by load_from_db and save_preference).
    fn apply(&mut self, key: &str, value: &str) {
        match key {
            "theme"                        => self.theme = value.to_string(),
            "last_directory"               => self.last_directory = value.to_string(),
            "jpeg_quality"                 => {
                if let Ok(v) = value.parse::<i64>() {
                    self.jpeg_quality = v.clamp(JPEG_QUALITY_MIN, JPEG_QUALITY_MAX);
                }
            }
            "recent_directories_max"       => {
                if let Ok(v) = value.parse::<i64>() {
                    self.recent_directories_max = v.clamp(RECENT_DIRS_MIN, RECENT_DIRS_MAX);
                }
            }
            "backup_directory"             => self.backup_directory = value.to_string(),
            "console_history_size"         => {
                if let Ok(v) = value.parse::<i64>() {
                    self.console_history_size = v.clamp(CONSOLE_HISTORY_MIN, CONSOLE_HISTORY_MAX);
                }
            }
            "macro_editor_font_size"       => {
                if let Ok(v) = value.parse::<i64>() {
                    self.macro_editor_font_size = v.clamp(MACRO_EDITOR_FONT_MIN, MACRO_EDITOR_FONT_MAX);
                }
            }
            "buffer_pool_memory_limit"     => {
                if let Ok(v) = value.parse::<i64>() {
                    self.buffer_pool_bytes = v.clamp(BUFFER_POOL_MIN_BYTES, BUFFER_POOL_MAX_BYTES);
                }
            }
            "autostretch_shadow_clip"      => {
                if let Ok(v) = value.parse::<f64>() {
                    self.autostretch_shadow_clip = v.clamp(AUTOSTRETCH_SHADOW_CLIP_MIN, AUTOSTRETCH_SHADOW_CLIP_MAX);
                }
            }
            "autostretch_target_bg"        => {
                if let Ok(v) = value.parse::<f64>() {
                    self.autostretch_target_bg = v.clamp(AUTOSTRETCH_TARGET_BG_MIN, AUTOSTRETCH_TARGET_BG_MAX);
                }
            }
            "crash_recovery_interval_secs" => {
                if let Ok(v) = value.parse::<i64>() {
                    self.crash_recovery_interval_secs = v.clamp(CRASH_RECOVERY_INTERVAL_MIN, CRASH_RECOVERY_INTERVAL_MAX);
                }
            }
            "active_threshold_profile_id"  => {
                self.active_threshold_profile_id = value.parse::<i64>().ok();
            }
            _ => {}
        }
    }

    /// Return a persisted field as a String for round-trip after clamping.
    fn get_as_string(&self, key: &str) -> Option<String> {
        match key {
            "theme"                        => Some(self.theme.clone()),
            "last_directory"               => Some(self.last_directory.clone()),
            "jpeg_quality"                 => Some(self.jpeg_quality.to_string()),
            "recent_directories_max"       => Some(self.recent_directories_max.to_string()),
            "backup_directory"             => Some(self.backup_directory.clone()),
            "console_history_size"         => Some(self.console_history_size.to_string()),
            "macro_editor_font_size"       => Some(self.macro_editor_font_size.to_string()),
            "buffer_pool_memory_limit"     => Some(self.buffer_pool_bytes.to_string()),
            "autostretch_shadow_clip"      => Some(self.autostretch_shadow_clip.to_string()),
            "autostretch_target_bg"        => Some(self.autostretch_target_bg.to_string()),
            "crash_recovery_interval_secs" => Some(self.crash_recovery_interval_secs.to_string()),
            "active_threshold_profile_id"  => self.active_threshold_profile_id.map(|v| v.to_string()),
            _ => None,
        }
    }
}
