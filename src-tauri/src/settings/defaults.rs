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

// Issue 171: no -1 sentinel. RAYON_THREAD_COUNT_DEFAULT is a static
// fallback only — used if num_cpus::get() were ever to return something
// degenerate (it can't in practice, but this keeps AppSettings::new()
// infallible without an unwrap). The real first-run default is computed
// in AppSettings::new() via num_cpus::get() - 1, the same call
// get_cpu_count (commands/display.rs) uses for the frontend's Auto
// button, so the built-in default and a manually-clicked Auto always
// agree on a given machine.
pub const RAYON_THREAD_COUNT_DEFAULT:  i64  = 1;
pub const RAYON_THREAD_COUNT_MIN:      i64  = 1;

// ── AutoStretch ───────────────────────────────────────────────────────────────

pub const DEFAULT_AUTOSTRETCH_SHADOW_CLIP:  f64 = -2.8;
pub const AUTOSTRETCH_SHADOW_CLIP_MIN:      f64 = -5.0;
pub const AUTOSTRETCH_SHADOW_CLIP_MAX:      f64 =  0.0;
pub const DEFAULT_AUTOSTRETCH_TARGET_BG:    f64 = 0.15;
pub const AUTOSTRETCH_TARGET_BG_MIN:        f64 = 0.01;
pub const AUTOSTRETCH_TARGET_BG_MAX:        f64 = 0.50;

// ── Threshold Profiles (AnalyzeFrames defaults) ───────────────────────────────

pub const DEFAULT_PROFILE_NAME:              &str = "Default";
pub const DEFAULT_BG_MEDIAN_SIGMA:           f64  = 2.5;
pub const BG_MEDIAN_SIGMA_MIN:               f64  = 0.5;
pub const BG_MEDIAN_SIGMA_MAX:               f64  = 4.0;
pub const DEFAULT_FWHM_SIGMA:                f64  = 2.5;
pub const FWHM_SIGMA_MIN:                    f64  = 0.5;
pub const FWHM_SIGMA_MAX:                    f64  = 4.0;
// Star count uses bimodal-anchored stats; 1.5σ is appropriate since the
// anchor is computed from the clean upper cluster, not the full population.
pub const DEFAULT_STAR_COUNT_SIGMA:          f64  = 1.5;
pub const STAR_COUNT_SIGMA_MIN:              f64  = 0.5;
pub const STAR_COUNT_SIGMA_MAX:              f64  = 4.0;
pub const DEFAULT_ECCENTRICITY_ABS:          f64  = 0.85;
pub const ECCENTRICITY_ABS_MIN:              f64  = 0.10;
pub const ECCENTRICITY_ABS_MAX:              f64  = 1.00;
pub const OUTLIER_SIGMA_THRESHOLD:           f64  = 4.0;

// Non-persisted runtime constants
// These are never written to the DB. They live here so they are
// findable and not scattered as magic numbers through the codebase.

pub const DISPLAY_MAX_WIDTH_PX:        u32  = 1200;  // box-filter downsample ceiling
pub const DETAIL_JPEG_QUALITY:         u8   = 90;    // full-res cache and display-resolution cache
pub const THUMBNAIL_JPEG_QUALITY:      u8   = 75;    // blink caches (12.5% / 25%)
pub const BLINK_WIDTH_12:              u32  = 376;   // blink thumbnail width, 12.5% resolution
pub const BLINK_WIDTH_25:              u32  = 752;   // blink thumbnail width, 25% resolution

// ── Stacking (StackFrames) ─────────────────────────────────────────────────
// Non-persisted algorithm thresholds — Issue 127 (reference candidacy gate),
// Issue 128 (cross-group M_cross validation), Issue 148 (constants
// consolidated from star_align.rs and stack_frames.rs — pure relocation,
// no values changed; each group below is labeled with its origin file).

pub const CROSS_GROUP_MAX_RESIDUAL_PX:    f32   = 2.0;  // max mean verification residual (px) before rejecting M_cross
pub const CROSS_GROUP_MIN_MATCHED:        usize = 10;   // min matched stars on M_cross verification before rejecting
pub const REF_MIN_STAR_FRACTION:          f64   = 0.5;  // min star count as a fraction of group median to be reference-eligible

// Frame grouping (from stack_frames.rs)
pub const MERIDIAN_FLIP_THRESHOLD:        f32   = 90.0;  // rotator delta (deg) that always starts a new group
pub const SESSION_GAP_MINUTES:            f32   = 120.0; // time gap (min) that, combined with ROTATOR_GROUP_TOLERANCE, starts a new group
pub const ROTATOR_GROUP_TOLERANCE:        f32   = 10.0;  // rotator delta (deg) required alongside a session gap to start a new group

// M_cross verification (from stack_frames.rs)
pub const CROSS_GROUP_VERIFY_MATCH_RADIUS_PX: f32 = 10.0; // search radius (px) matching a group-ref star to a master-ref star during M_cross verification; feeds CROSS_GROUP_MIN_MATCHED above

// Pass 2 combination (from stack_frames.rs)
pub const STACK_SIGMA_CLIP:               f32   = 2.5;   // sigma-clip threshold for Pass 2 accumulation

// RANSAC rigid alignment (from star_align.rs, estimate_rigid_transform)
pub const MATCH_TOLERANCE:                f32   = 15.0;
// Issue 146: raised from 4. Measured against a real sparse-star session
// (M104, two nights with meridian flips, per-frame star counts 27-274,
// worst-case RANSAC candidate-match count 50) — even the sparsest frame
// in that session cleared 8 candidate matches with wide margin. Must stay
// >= MIN_INLIERS to be meaningful.
pub const MIN_MATCHES:                    usize = 8;
pub const INLIER_TOLERANCE:               f32   = 2.0;
// Issue 146: raised from 4 to match TRI_MIN_INLIERS below — unifies the
// RANSAC and triangle paths on the same "how many agreeing correspondences
// constitute a valid transform" answer. Measured against the same M104
// session: worst-case accepted inlier count was 49 (frame 50 of 97),
// clearing this floor by ~8x.
pub const MIN_INLIERS:                    usize = 6;
// Issue 146: tightened from 0.52 (~30°). Measured within-group residual
// rotation across the same M104 session (97 frames, both nights) ranged
// essentially 0.000°-0.010° — the old bound was roughly three orders of
// magnitude looser than the real signal. Set with real margin above the
// observed maximum, not right at it, since this is one session's evidence,
// not a hard physical limit — a session with genuinely poor polar
// alignment or guiding could still exceed this and be correctly excluded,
// which is the intended behavior, not a bug.
pub const MAX_ROTATION_RAD:               f32   = 0.0349; // ~2 degrees
pub const MAX_TRANSLATION_DEVIATION:      f32   = 20.0;

// Triangle rigid alignment (from star_align.rs, estimate_rigid_transform_triangles)
pub const TRI_MAX_STARS:                  usize = 30;
pub const TRI_INLIER_TOLERANCE:           f32   = 3.0;
pub const TRI_MIN_INLIERS:                usize = 6;

// ----------------------------------------------------------------------
// ----------------------------------------------------------------------
// ----------------------------------------------------------------------
