// context/mod.rs — AppContext: session state passed to every plugin
// Spec §6.4

use std::collections::HashMap;
use serde::{Deserialize, Serialize};

// ── Image buffer (placeholder for Phase 2 buffer pool) ───────────────────────

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
    /// Normalize a pixel value to 0.0–1.0 range
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
    /// Currently active working directory (set by SelectDirectory)
    pub active_directory: Option<String>,

    /// Flat list of file paths in the active directory
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
    /// AutoStretch defaults — loaded from AppSettings at startup and on preference change
    pub autostretch_shadow_clip: f32,
    pub autostretch_target_bg:   f32,

    /// Active AnalyzeFrames thresholds — loaded from AppSettings at startup and on profile change
    pub analysis_thresholds: crate::analysis::session_stats::AnalysisThresholds,

    /// Thresholds actually used in the last AnalyzeFrames run — returned with analysis results
    pub last_analysis_thresholds: Option<crate::analysis::session_stats::AnalysisThresholds>,
    pub analysis_results: HashMap<String, crate::analysis::AnalysisResult>,

    /// Configurable log directory — if None, falls back to Tauri app data dir
    pub log_dir: Option<String>,

    /// Current session ID in session_history table — set by open_session, cleared by close_session
    pub current_session_id: Option<i64>,
}

impl AppContext {
    pub fn new() -> Self {
        let mut ctx = Self::default();
        ctx.autostretch_shadow_clip = crate::settings::defaults::DEFAULT_AUTOSTRETCH_SHADOW_CLIP as f32;
        ctx.autostretch_target_bg   = crate::settings::defaults::DEFAULT_AUTOSTRETCH_TARGET_BG as f32;
        ctx
    }

    /// Get a loaded image by index into the file list
    pub fn current_image(&self) -> Option<&ImageBuffer> {
        let filename = self.file_list.get(self.current_frame)?;
        self.image_buffers.get(filename)
    }

    /// Get a mutable reference to the current image
    pub fn current_image_mut(&mut self) -> Option<&mut ImageBuffer> {
        let filename = self.file_list.get(self.current_frame)?.clone();
        self.image_buffers.get_mut(&filename)
    }

    /// Total memory used by all image buffers (placeholder — Phase 2)
    pub fn total_memory_used(&self) -> usize {
        0 // Phase 2: sum actual buffer sizes
    }

    /// Get or create the AnalysisResult entry for a file path.
    pub fn analysis_result_for(&mut self, path: &str) -> &mut crate::analysis::AnalysisResult {
        self.analysis_results
            .entry(path.to_string())
            .or_insert_with(|| crate::analysis::AnalysisResult::new(path))
    }
}
