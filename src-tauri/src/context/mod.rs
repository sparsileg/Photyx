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
pub struct ImageBuffer {
    pub filename:    String,
    pub width:       u32,
    pub height:      u32,
    pub bit_depth:   BitDepth,
    pub color_space: ColorSpace,
    pub channels:    u8,
    pub keywords:    HashMap<String, KeywordEntry>,
    // Phase 2: actual pixel data buffer goes here
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

    /// Loaded image buffers (keyed by filename)
    pub image_buffers: HashMap<String, ImageBuffer>,

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
