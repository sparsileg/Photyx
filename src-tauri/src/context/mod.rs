// context/mod.rs — AppContext: session state passed to every plugin
// Spec §6.4

use std::collections::HashMap;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};

// ── Image buffer ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ColorSpace {
    Mono,
    RGB,
    Bayer,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BitDepth {
    U8,
    U16,
    F32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PixelData {
    U8(Vec<u8>),
    U16(Vec<u16>),
    F32(Vec<f32>),
}

impl PixelData {
    pub fn normalize_value(val: f64, bit_depth: &BitDepth) -> f64 {
        match bit_depth {
            BitDepth::U8  => val / 255.0,
            BitDepth::U16 => val / 65535.0,
            BitDepth::F32 => val,
        }
    }

    pub fn len(&self) -> usize {
        match self {
            PixelData::U8(v)  => v.len(),
            PixelData::U16(v) => v.len(),
            PixelData::F32(v) => v.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageBuffer {
    pub filename:      String,
    pub width:         u32,
    pub height:        u32,
    pub display_width: u32,
    pub bit_depth:     BitDepth,
    pub color_space:   ColorSpace,
    pub channels:      u8,
    pub keywords:      HashMap<String, KeywordEntry>,
    pub pixels:        Option<PixelData>,
}

// ── Keyword entry ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeywordEntry {
    pub name:    String,
    pub value:   String,
    pub comment: Option<String>,
}

impl KeywordEntry {
    pub fn new(name: &str, value: &str, comment: Option<&str>) -> Self {
        Self {
            name:    name.to_uppercase(),
            value:   value.to_string(),
            comment: comment.map(|s| s.to_string()),
        }
    }
}

// ── Session state ─────────────────────────────────────────────────────────────

#[derive(Debug, Default, Clone, PartialEq)]
pub enum BlinkCacheStatus {
    #[default]
    Idle,
    Building,
    Ready,
}

#[derive(Debug, Default)]
pub struct AppContext {
    /// Flat list of file paths in the current session
    pub file_list: Vec<String>,

    /// Loaded image buffers (keyed by file path) — raw pixel data, never modified
    pub image_buffers: HashMap<String, ImageBuffer>,

    /// Display cache (keyed by file path) — pre-rendered display-resolution JPEG bytes
    pub display_cache: HashMap<String, Vec<u8>>,

    /// Full-resolution cache (keyed by file path) — built on demand for 100%/200% zoom
    pub full_res_cache: HashMap<String, Vec<u8>>,

    /// Blink cache at 12.5% resolution (keyed by file path)
    pub blink_cache_12: HashMap<String, Vec<u8>>,

    /// Blink cache at 25% resolution (keyed by file path)
    pub blink_cache_25: HashMap<String, Vec<u8>>,

    /// Background cache build status
    pub blink_cache_status: BlinkCacheStatus,

    /// Index of the currently displayed frame
    pub current_frame: usize,

    /// pcode variable store
    pub variables: HashMap<String, String>,

    /// Last computed histogram stats (for frontend retrieval)
    pub last_histogram: Option<crate::plugins::get_histogram::HistogramStats>,

    /// Last computed Auto-STF parameters (c0, m) — reused by get_full_frame
    pub last_stf_params: Option<(f32, f32)>,

    /// AutoStretch defaults — loaded from AppSettings at startup and on preference change
    pub autostretch_shadow_clip: f32,
    pub autostretch_target_bg:   f32,

    /// Active AnalyzeFrames thresholds — loaded from AppSettings at startup and on profile change
    pub analysis_thresholds: crate::analysis::session_stats::AnalysisThresholds,

    /// Thresholds actually used in the last AnalyzeFrames run — returned with analysis results
    pub last_analysis_thresholds: Option<crate::analysis::session_stats::AnalysisThresholds>,
    pub analysis_results: HashMap<String, crate::analysis::AnalysisResult>,

    /// Paths of frames excluded from session stat recomputation in the last AnalyzeFrames run
    pub outlier_frame_paths: std::collections::HashSet<String>,

    /// Clean session stats from the last AnalyzeFrames run (outliers excluded).
    pub last_session_stats: Option<crate::analysis::session_stats::SessionStats>,

    /// Configurable log directory — if None, falls back to Tauri app data dir
    pub log_dir: Option<String>,

    /// Buffer pool memory limit in bytes — copied from AppSettings at startup
    /// and on preference change. Used by read plugins to gate loading.
    pub buffer_pool_bytes: i64,

    /// Current session ID in session_history table — set by open_session, cleared by close_session
    pub current_session_id: Option<i64>,

    /// True when analysis results were loaded from a JSON import rather than a live
    /// AnalyzeFrames run. Disables Commit in the frontend and skips reclassification
    /// in get_analysis_results (imported classifications are authoritative).
    pub is_imported_session: bool,

    /// Transient stacked result — holds the output of StackFrames without a source
    /// file path. Cleared by ClearStack or a new StackFrames run.
    pub stack_result: Option<ImageBuffer>,

    /// Per-frame contribution metrics from the last StackFrames run.
    /// Cleared by ClearStack or a new StackFrames run.
    pub stack_contributions: Vec<crate::analysis::stack_metrics::FrameContribution>,

    /// Summary metrics from the last StackFrames run.
    pub stack_summary: Option<crate::analysis::stack_metrics::StackSummary>,
}

impl AppContext {
    /// Returns the unique parent directories of all loaded files.
    pub fn source_directories(&self) -> Vec<PathBuf> {
        let mut dirs: Vec<PathBuf> = self.file_list.iter()
            .filter_map(|f| std::path::Path::new(f).parent().map(|p| p.to_path_buf()))
            .collect();
        dirs.sort();
        dirs.dedup();
        dirs
    }

    /// Returns the common parent directory if all files share one, else None.
    pub fn common_parent(&self) -> Option<PathBuf> {
        let dirs = self.source_directories();
        if dirs.len() == 1 { dirs.into_iter().next() } else { None }
    }

    /// Sync all fields that mirror AppSettings into AppContext.
    /// Call this at startup and whenever any preference changes.
    pub fn sync_from_settings(&mut self, settings: &crate::settings::AppSettings) {
        self.autostretch_shadow_clip = settings.autostretch_shadow_clip as f32;
        self.autostretch_target_bg   = settings.autostretch_target_bg as f32;
        self.buffer_pool_bytes       = settings.buffer_pool_bytes;
    }

    pub fn new() -> Self {
        let mut ctx = Self::default();
        ctx.autostretch_shadow_clip = crate::settings::defaults::DEFAULT_AUTOSTRETCH_SHADOW_CLIP as f32;
        ctx.autostretch_target_bg   = crate::settings::defaults::DEFAULT_AUTOSTRETCH_TARGET_BG as f32;
        ctx.buffer_pool_bytes       = crate::settings::defaults::DEFAULT_BUFFER_POOL_BYTES;
        ctx
    }

    pub fn current_image(&self) -> Option<&ImageBuffer> {
        let filename = self.file_list.get(self.current_frame)?;
        self.image_buffers.get(filename)
    }

    pub fn current_image_mut(&mut self) -> Option<&mut ImageBuffer> {
        let filename = self.file_list.get(self.current_frame)?.clone();
        self.image_buffers.get_mut(&filename)
    }

    pub fn total_memory_used(&self) -> usize {
        self.image_buffers.values().map(|buf| {
            match &buf.pixels {
                Some(PixelData::U8(v))  => v.len(),
                Some(PixelData::U16(v)) => v.len() * 2,
                Some(PixelData::F32(v)) => v.len() * 4,
                None => 0,
            }
        }).sum()
    }

    /// Clear all session state — pixel buffers, caches, analysis results.
    pub fn clear_session(&mut self) {
        self.file_list.clear();
        self.image_buffers.clear();
        self.display_cache.clear();
        self.full_res_cache.clear();
        self.blink_cache_12.clear();
        self.blink_cache_25.clear();
        self.blink_cache_status = BlinkCacheStatus::Idle;
        self.current_frame = 0;
        self.analysis_results.clear();
        self.outlier_frame_paths.clear();
        self.last_session_stats = None;
        self.last_analysis_thresholds = None;
        self.last_stf_params = None;
        self.last_histogram = None;
        self.variables.clear();
        self.is_imported_session = false;
        self.stack_result = None;
        self.stack_contributions.clear();
        self.stack_summary = None;
    }

    /// Remove rejected files from the session after a commit.
    /// Clears analysis results but leaves pass frames loaded.
    pub fn remove_rejected_files(&mut self, rejected_paths: &[String]) {
        let reject_set: std::collections::HashSet<&str> =
            rejected_paths.iter().map(|s| s.as_str()).collect();
        self.file_list.retain(|p| !reject_set.contains(p.as_str()));
        for path in rejected_paths {
            self.image_buffers.remove(path);
            self.display_cache.remove(path);
            self.full_res_cache.remove(path);
            self.blink_cache_12.remove(path);
            self.blink_cache_25.remove(path);
        }
        self.analysis_results.clear();
        self.outlier_frame_paths.clear();
        self.last_session_stats = None;
        self.last_analysis_thresholds = None;
        self.current_frame = 0;
        self.is_imported_session = false;
    }

pub fn analysis_result_for(&mut self, path: &str) -> &mut crate::analysis::AnalysisResult {
        self.analysis_results
            .entry(path.to_string())
            .or_insert_with(|| crate::analysis::AnalysisResult::new(path))
    }

    /// Discard the transient stack result and per-frame contribution data.
    /// Called by ClearStack and at the start of a new StackFrames run.
    pub fn clear_stack(&mut self) {
        self.stack_result = None;
        self.stack_contributions.clear();
        self.stack_summary = None;
    }
}
