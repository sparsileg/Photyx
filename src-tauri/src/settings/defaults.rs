// settings/defaults.rs — All hard-coded default values and bounds for Photyx settings.
// This is the single source of truth. No magic numbers or default strings anywhere else.
// AppSettings is populated from these constants + the database at startup.

// ── UI / Viewer ───────────────────────────────────────────────────────────────

pub const DEFAULT_THEME:               &str = "matrix";
pub const DEFAULT_ZOOM_LEVEL:          &str = "fit";
pub const DEFAULT_BLINK_RATE_SECS:     f64  = 0.1;
pub const DEFAULT_CHANNEL_VIEW:        &str = "rgb";

// ── File & Path ───────────────────────────────────────────────────────────────

pub const DEFAULT_LAST_DIRECTORY:      &str = "";
pub const DEFAULT_JPEG_QUALITY:        i64  = 75;
pub const JPEG_QUALITY_MIN:            i64  = 1;
pub const JPEG_QUALITY_MAX:            i64  = 100;
pub const DEFAULT_RECENT_DIRS_MAX:     i64  = 10;
pub const RECENT_DIRS_MIN:             i64  = 1;
pub const RECENT_DIRS_MAX:             i64  = 50;

// ── pcode / Macro ─────────────────────────────────────────────────────────────

pub const DEFAULT_BACKUP_DIRECTORY:    &str = "";   // resolved to OS Downloads at runtime
pub const DEFAULT_CONSOLE_HISTORY_SIZE: i64 = 500;
pub const CONSOLE_HISTORY_MIN:         i64  = 100;
pub const CONSOLE_HISTORY_MAX:         i64  = 5000;
pub const DEFAULT_MACRO_EDITOR_FONT:   i64  = 13;
pub const MACRO_EDITOR_FONT_MIN:       i64  = 8;
pub const MACRO_EDITOR_FONT_MAX:       i64  = 24;
pub const DEFAULT_ERROR_BEHAVIOR:      &str = "halt";  // not persisted

// ── Performance ───────────────────────────────────────────────────────────────

pub const DEFAULT_BUFFER_POOL_BYTES:   i64  = 4 * 1024 * 1024 * 1024; // 4 GB
pub const BUFFER_POOL_MIN_BYTES:       i64  = 512 * 1024 * 1024;       // 512 MB
pub const BUFFER_POOL_MAX_BYTES:       i64  = 32 * 1024 * 1024 * 1024; // 32 GB

// ── AutoStretch ───────────────────────────────────────────────────────────────

pub const DEFAULT_AUTOSTRETCH_SHADOW_CLIP:  f64 = -2.8;
pub const AUTOSTRETCH_SHADOW_CLIP_MIN:      f64 = -5.0;
pub const AUTOSTRETCH_SHADOW_CLIP_MAX:      f64 =  0.0;
pub const DEFAULT_AUTOSTRETCH_TARGET_BG:    f64 = 0.15;
pub const AUTOSTRETCH_TARGET_BG_MIN:        f64 = 0.01;
pub const AUTOSTRETCH_TARGET_BG_MAX:        f64 = 0.50;

// ── Crash Recovery ────────────────────────────────────────────────────────────

pub const DEFAULT_CRASH_RECOVERY_INTERVAL_SECS: i64 = 60;
pub const CRASH_RECOVERY_INTERVAL_MIN:          i64 = 15;
pub const CRASH_RECOVERY_INTERVAL_MAX:          i64 = 300;

// ── Threshold Profiles (AnalyzeFrames defaults) ───────────────────────────────

pub const DEFAULT_PROFILE_NAME:              &str = "Default";
pub const DEFAULT_BG_MEDIAN_SIGMA:           f64  = 2.5;
pub const BG_MEDIAN_SIGMA_MIN:               f64  = 0.5;
pub const BG_MEDIAN_SIGMA_MAX:               f64  = 4.0;
pub const DEFAULT_SNR_SIGMA:                 f64  = -2.5;
pub const SNR_SIGMA_MIN:                     f64  = 0.5;
pub const SNR_SIGMA_MAX:                     f64  = 5.0;
pub const DEFAULT_FWHM_SIGMA:                f64  = 2.5;
pub const FWHM_SIGMA_MIN:                    f64  = 0.5;
pub const FWHM_SIGMA_MAX:                    f64  = 4.0;
pub const DEFAULT_STAR_COUNT_SIGMA:          f64  = -1.5;
pub const STAR_COUNT_SIGMA_MIN:              f64  = 0.5;
pub const STAR_COUNT_SIGMA_MAX:              f64  = 5.0;
pub const DEFAULT_ECCENTRICITY_ABS:          f64  = 0.85;
pub const ECCENTRICITY_ABS_MIN:              f64  = 0.10;
pub const ECCENTRICITY_ABS_MAX:              f64  = 1.00;
pub const OUTLIER_SIGMA_THRESHOLD:           f64  = 4.0;

// ── Non-persisted runtime constants ──────────────────────────────────────────
// These are never written to the DB. They live here so they are
// findable and not scattered as magic numbers through the codebase.

pub const DISPLAY_MAX_WIDTH_PX:        u32  = 1200;  // box-filter downsample ceiling
pub const BLINK_JPEG_QUALITY:          u8   = 85;
pub const DISPLAY_JPEG_QUALITY:        u8   = 92;
pub const ALGORITHM_SET_VERSION:       i64  = 1;     // bump when any analysis algorithm changes
