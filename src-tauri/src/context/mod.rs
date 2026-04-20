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
    pub filename:    String,
    pub width:       u32,
    pub height:      u32,
    pub bit_depth:   BitDepth,
    pub color_space: ColorSpace,
    pub channels:    u8,
    pub keywords:    HashMap<String, KeywordEntry>,
    pub pixels:      Option<PixelData>,
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

    /// Blink cache (keyed by file path) — pre-rendered blink-resolution JPEG bytes
    pub blink_cache: HashMap<String, Vec<u8>>,

    /// Index of the currently displayed frame
    pub current_frame: usize,

    /// pcode variable store
    pub variables: HashMap<String, String>,
}

impl AppContext {
    pub fn new() -> Self {
        Self::default()
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
}
