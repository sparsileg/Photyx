// plugins/pixel_chunking.rs — Shared chunked pixel-snapshot loading
// Used by AnalyzeFrames and CacheFrames to bound peak memory during
// parallel per-frame processing. Rather than cloning every loaded frame's
// pixel buffer up front (doubling ctx.image_buffers for the run's
// duration), callers process ctx.file_list in chunks sized to the Rayon
// thread count: clone one chunk's pixel data, run the parallel pass over
// just that chunk, then move to the next chunk. Peak memory is bounded to
// one chunk's worth of raw buffers instead of the whole session — same
// pattern StackFrames' Pass 2 already uses.

use crate::context::{AppContext, PixelData};

/// One frame's pixel data plus the minimal metadata needed to process it,
/// snapshotted out of ctx.image_buffers for a single chunk.
pub struct FramePixelSnapshot {
    pub path:     String,
    pub width:    usize,
    pub height:   usize,
    pub channels: usize,
    pub pixels:   PixelData,
}

/// Clone pixel data for the given chunk of paths out of ctx.image_buffers.
/// Paths with no loaded buffer or no pixel data are silently skipped.
pub fn snapshot_pixel_chunk(ctx: &AppContext, paths: &[String]) -> Vec<FramePixelSnapshot> {
    paths.iter().filter_map(|path| {
        let buf    = ctx.image_buffers.get(path)?;
        let pixels = buf.pixels.as_ref()?.clone();
        Some(FramePixelSnapshot {
            path:     path.clone(),
            width:    buf.width as usize,
            height:   buf.height as usize,
            channels: buf.channels as usize,
            pixels,
        })
    }).collect()
}

/// Resolve the effective chunk size from ctx.rayon_thread_count.
/// Used by both AnalyzeFrames and CacheFrames.
pub fn chunk_size(ctx: &AppContext) -> usize {
    (ctx.rayon_thread_count as usize).max(1)
}

// ----------------------------------------------------------------------
