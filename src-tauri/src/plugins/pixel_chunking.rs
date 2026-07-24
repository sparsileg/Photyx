// plugins/pixel_chunking.rs — Shared chunked pixel-snapshot loading
//
// Issue 174: the pixel source is now the source file on disk, not
// ctx.image_buffers. Since Issue 173, image_buffers is a metadata-only
// registry with raw pixels resident for only the small viewing LRU set;
// analysis/stacking/caching therefore cannot read pixels from it (they
// would see only the handful of recently-viewed frames). Instead, callers
// process ctx.file_list in chunks sized to the Rayon thread count, and for
// each chunk this module reads+decodes that chunk's frames directly from
// disk into owned snapshots. Peak memory is bounded to one chunk's worth of
// raw buffers, and the viewing LRU is never touched or churned.
//
// FITS sequential-access discipline (cfitsio is not thread-safe): the
// disk read + decode for each path happens here, in a sequential loop —
// never inside a caller's Rayon closure. Callers run their parallel work
// over the already-decoded snapshots this returns.
//
// Consumers: AnalyzeFrames, StackFrames (Pass 2), CacheFrames. Each applies
// its own failure policy to LoadOutcome::Missing / ::Unreadable — see
// load_pixel_chunk's doc comment.

use crate::context::{AppContext, KeywordEntry, PixelData};
use crate::plugins::image_reader::read_image_file;
use std::collections::HashMap;
use std::path::Path;

/// One frame's pixel data plus the metadata needed to process it, read
/// fresh from disk for a single chunk. Carries keywords and color_space so
/// callers don't need a second image_buffers lookup for plate scale,
/// debayer decisions, or filter/object metadata.
pub struct FramePixelSnapshot {
    pub path:     String,
    pub width:    usize,
    pub height:   usize,
    pub channels: usize,
    pub keywords: HashMap<String, KeywordEntry>,
    pub pixels:   PixelData,
}

/// Per-path outcome of a chunk load, returned in the same order as the
/// input paths so callers can zip against their chunk and apply policy.
///
/// The Missing/Unreadable split lets each caller distinguish a source file
/// that is gone from one that is present but failed to decode, and word its
/// diagnostics accordingly:
///   - Missing    — the path does not exist on disk.
///   - Unreadable — the file exists but read_image_file failed (carries the
///                  decode error), or decoded to a buffer with no pixels.
///
/// Failure policy is the caller's, not this module's: AnalyzeFrames treats
/// either failure as a hard error (partial frame sets corrupt session
/// statistics); StackFrames and CacheFrames record the frame as an
/// exclusion and continue (a degraded stack/cache is still useful, and the
/// exclusion is surfaced loudly in their summaries).
pub enum LoadOutcome {
    Loaded(FramePixelSnapshot),
    Missing { path: String },
    Unreadable { path: String, error: String },
}

/// Read + decode a chunk of frames from disk, sequentially, into owned
/// snapshots. One LoadOutcome per input path, in order.
///
/// Sequential by contract: this is the single place FITS files are opened
/// for these pipelines, and cfitsio is not thread-safe. Do not parallelize
/// this loop. Callers parallelize over the returned Loaded snapshots.
pub fn load_pixel_chunk(paths: &[String]) -> Vec<LoadOutcome> {
    paths.iter().map(|path| {
        // Distinguish "gone" from "present but undecodable" before
        // attempting the decode, so the two produce different diagnostics.
        if !Path::new(path).exists() {
            return LoadOutcome::Missing { path: path.clone() };
        }

        let buf = match read_image_file(path) {
            Ok(b)  => b,
            Err(e) => return LoadOutcome::Unreadable { path: path.clone(), error: e },
        };

        let pixels = match buf.pixels {
            Some(p) => p,
            None    => return LoadOutcome::Unreadable {
                path:  path.clone(),
                error: "decoded buffer contains no pixel data".to_string(),
            },
        };

        LoadOutcome::Loaded(FramePixelSnapshot {
            path:     path.clone(),
            width:    buf.width as usize,
            height:   buf.height as usize,
            channels: buf.channels as usize,
            keywords: buf.keywords,
            pixels,
        })
    }).collect()
}

/// Resolve the effective chunk size from ctx.rayon_thread_count.
/// Used by AnalyzeFrames, StackFrames, and CacheFrames.
pub fn chunk_size(ctx: &AppContext) -> usize {
    (ctx.rayon_thread_count as usize).max(1)
}

// ----------------------------------------------------------------------
// ----------------------------------------------------------------------
// ----------------------------------------------------------------------
