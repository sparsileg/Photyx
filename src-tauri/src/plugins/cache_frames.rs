// plugins/cache_frames.rs — CacheFrames built-in plugin
// Pre-renders all loaded images to blink-resolution JPEGs.
// Stores results in AppContext::blink_cache, keyed by file path.
// Raw image_buffers are never modified.
// Uses Rayon for parallel processing across frames.

use tracing::info;
use image::RgbImage;
use std::io::Cursor;
use rayon::prelude::*;

use crate::plugin::{PhotyxPlugin, ArgMap, ParamSpec, ParamType, PluginOutput, PluginError};
use crate::context::AppContext;
use crate::settings::defaults::{THUMBNAIL_JPEG_QUALITY, BLINK_WIDTH_12, BLINK_WIDTH_25};

pub struct CacheFrames;

impl PhotyxPlugin for CacheFrames {
    fn name(&self) -> &str { "CacheFrames" }
    fn version(&self) -> &str { "1.1.0" }
    fn description(&self) -> &str { "Pre-renders all loaded images to blink-resolution JPEGs" }

    fn parameters(&self) -> Vec<ParamSpec> {
        vec![
            ParamSpec {
                name:        "resolution".to_string(),
                param_type:  ParamType::String,
                required:    false,
                description: "Blink resolution: 12 (12.5%) or 25 (25%). Default: both".to_string(),
                default:     Some("both".to_string()),
            },
        ]
    }

    fn execute(&self, ctx: &mut AppContext, args: &ArgMap) -> Result<PluginOutput, PluginError> {
        let resolution = args.get("resolution").map(|s| s.as_str()).unwrap_or("both");

        let resolutions: &[(&str, usize)] = match resolution {
            "12"   => &[("12", BLINK_WIDTH_12 as usize)],
            "25"   => &[("25", BLINK_WIDTH_25 as usize)],
            _      => &[("12", BLINK_WIDTH_12 as usize), ("25", BLINK_WIDTH_25 as usize)], // "both" — default
        };

        if ctx.file_list.is_empty() {
            return Err(PluginError::new("NO_FILES", "No files loaded. Use AddFiles first."));
        }

        // Clear target caches up front — chunks are processed and inserted
        // incrementally below, so caches can't be bulk-replaced at the end
        // the way the old single-pass version did.
        for &(res_name, _) in resolutions {
            match res_name {
                "12" => ctx.blink_cache_12.clear(),
                _    => ctx.blink_cache_25.clear(),
            }
        }

        let total     = ctx.file_list.len();
        let chunk_len = crate::plugins::pixel_chunking::chunk_size(ctx);
        let file_list = ctx.file_list.clone();
        let mut cached_counts: std::collections::HashMap<&str, usize> =
            resolutions.iter().map(|&(name, _)| (name, 0)).collect();
        let mut failed_counts: std::collections::HashMap<&str, usize> =
            resolutions.iter().map(|&(name, _)| (name, 0)).collect();

        // One progress unit per (frame × requested resolution) — matches
        // the atomic-counter pattern AnalyzeFrames uses. Incremented at the
        // start of each frame's processing (below) so progress reflects
        // frames attempted, not just frames that succeeded.
        let progress_total   = (total * resolutions.len()) as u32;
        let progress_counter = std::sync::atomic::AtomicUsize::new(0);
        crate::set_progress("Caching frames", 0, progress_total);

        // Issue 175: the full ordered request list is known up front — one
        // Raw request per file (CacheFrames needs the full PixelData for
        // downsample_to_planes below, so LoadKind::Raw, not Luma/
        // ColorNormalized). Spawning here lets the reader thread start
        // decoding ahead of the render/encode loop below; by the time this
        // loop reaches its second chunk, disk read + decode for that chunk
        // has likely already overlapped with the previous chunk's
        // render/encode work instead of blocking in front of it.
        let requests: Vec<crate::plugins::pixel_chunking::LoadRequest> = file_list.iter()
            .map(|path| crate::plugins::pixel_chunking::LoadRequest {
                path: path.clone(),
                kind: crate::plugins::pixel_chunking::LoadKind::Raw,
            })
            .collect();
        let reader_capacity = crate::plugins::pixel_chunking::prefetch_capacity_chunked(ctx);
        let mut reader = crate::plugins::pixel_chunking::PixelReaderHandle::spawn_disk_reader(
            requests, reader_capacity,
        );

        for path_chunk in file_list.chunks(chunk_len) {
            // Sequential: drain this chunk's worth of outcomes from the
            // background reader (Issue 175) rather than reading them
            // synchronously here — the reader has been decoding ahead of
            // this loop on its own thread since spawn_disk_reader above.
            // Reused across every requested resolution below, same as
            // before: avoids re-reading pixels twice per chunk when
            // resolution=both (the default), while still bounding peak
            // memory to roughly one chunk (reader_capacity, Issue 175)
            // instead of the whole session.
            //
            // Split into decoded snapshots and disk-read failures. A frame
            // that could not be read is absent for every requested
            // resolution, so it is logged once here (distinguishing a
            // missing file from an unreadable one) and counted against each
            // resolution's failed tally below — exclude-and-continue: a
            // blink cache missing a few frames is degraded, not corrupt, and
            // the shortfall is surfaced in the summary line.
            let mut frames: Vec<crate::plugins::pixel_chunking::FramePixelSnapshot> = Vec::new();
            let mut chunk_read_failures = 0usize;
            let mut received = 0usize;
            for _ in 0..path_chunk.len() {
                let outcome = match reader.recv() {
                    Some(o) => o,
                    // Reader closed before delivering everything this chunk
                    // expected — shouldn't happen (the request list was
                    // built 1:1 from file_list), but handled as a shortfall
                    // below rather than left to silently under-count.
                    None => break,
                };
                received += 1;
                match outcome {
                    crate::plugins::pixel_chunking::LoadOutcome::Loaded(
                        crate::plugins::pixel_chunking::LoadedFrame::Raw(snap)
                    ) => frames.push(snap),
                    crate::plugins::pixel_chunking::LoadOutcome::Loaded(_) => {
                        // Unreachable in practice: this reader was spawned
                        // with LoadKind::Raw requests only, so every Loaded
                        // outcome is LoadedFrame::Raw. Guarded rather than
                        // left to panic if that ever changes.
                        chunk_read_failures += 1;
                        info!("CacheFrames: internal error — unexpected non-Raw LoadedFrame for a Raw request");
                    }
                    crate::plugins::pixel_chunking::LoadOutcome::Missing { path } => {
                        chunk_read_failures += 1;
                        info!("CacheFrames: source file missing, skipped — {}", path);
                    }
                    crate::plugins::pixel_chunking::LoadOutcome::Unreadable { path, error } => {
                        chunk_read_failures += 1;
                        info!("CacheFrames: source file unreadable, skipped — {} ({})", path, error);
                    }
                }
            }
            if received < path_chunk.len() {
                let shortfall = path_chunk.len() - received;
                chunk_read_failures += shortfall;
                info!("CacheFrames: background reader closed early — {} frame(s) in this chunk not received", shortfall);
            }

            for &(res_name, max_w) in resolutions {
                let results: Vec<(String, Vec<u8>)> = frames.par_iter().filter_map(|frame| {
                let done = progress_counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                crate::set_progress("Caching frames", done as u32, progress_total);

                // Forced mono (channels=1) regardless of source color — blink
                // thumbnails have never included color; this preserves that
                // existing behavior while sharing the same box-filter core
                // as the other display/cache paths (Issue 86).
                let (mut planes, disp_w, disp_h) = crate::render::downsample_to_planes(
                    &frame.pixels, frame.width, frame.height, 1, max_w,
                );
                let pixel_count = disp_w * disp_h;

                // Compute STF parameters and stretch
                let (c0, m) = compute_stf_params_pub(&planes[0]);
                let c0_range = (1.0 - c0).max(f32::EPSILON);
                for p in planes[0].iter_mut() {
                    let clipped = ((*p - c0) / c0_range).clamp(0.0, 1.0);
                    *p = mtf_pub(m, clipped);
                }

                let rgb = crate::render::planes_to_rgb8(&planes, pixel_count);

                let img = RgbImage::from_raw(disp_w as u32, disp_h as u32, rgb)?;
                let mut buf = Cursor::new(Vec::new());

                use image::codecs::jpeg::JpegEncoder;
                let mut encoder = JpegEncoder::new_with_quality(&mut buf, THUMBNAIL_JPEG_QUALITY);
                encoder.encode_image(&img).ok()?;

                info!("CacheFrames: cached {} ({}×{})", frame.path, disp_w, disp_h);
                Some((frame.path.clone(), buf.into_inner()))
            }).collect();

                // ── Store this chunk's results ─────────────────────────────────
                let n = results.len();
                match res_name {
                    "12" => { for (path, jpeg) in results { ctx.blink_cache_12.insert(path, jpeg); } }
                    _    => { for (path, jpeg) in results { ctx.blink_cache_25.insert(path, jpeg); } }
                }
                *cached_counts.get_mut(res_name).unwrap() += n;
                // Two failure sources, both counted per resolution: frames
                // that decoded but failed to render/encode in the par_iter
                // above (frames.len() - n), and frames that never decoded at
                // all because their source file was missing/unreadable
                // (chunk_read_failures). Progress was advanced only for the
                // decoded frames the par_iter iterated, so read failures do
                // not touch progress_counter — total attempted still reads
                // out correctly against progress_total.
                *failed_counts.get_mut(res_name).unwrap() += (frames.len() - n) + chunk_read_failures;
            } // end resolution loop
            // This chunk's cloned pixel buffers (`frames`) drop here,
            // before the next chunk is loaded.
        } // end chunk loop

        for &(res_name, _) in resolutions {
            info!("CacheFrames: {}% resolution — {}/{} frames cached ({} failed)",
                res_name, cached_counts[&res_name], total, failed_counts[&res_name]);
        }

        crate::set_progress("", 0, 0);
        ctx.blink_cache_status = crate::context::BlinkCacheStatus::Ready;

        let total_failed: usize = failed_counts.values().sum();
        let resolution_summary = resolutions.iter()
            .map(|&(res_name, _)| format!("{}/{} at {}%", cached_counts[&res_name], total, res_name))
            .collect::<Vec<_>>()
            .join(", ");

        let msg = if total_failed > 0 {
            format!("Cached {} ({} failure(s))", resolution_summary, total_failed)
        } else {
            format!("Cached {}", resolution_summary)
        };

        Ok(PluginOutput::Message(msg))
    }
}

pub fn compute_stf_params_pub(pixels: &[f32]) -> (f32, f32) {
    let mut valid: Vec<f32> = pixels.iter().cloned().filter(|p| p.is_finite()).collect();
    if valid.is_empty() { return (0.0, 0.5); }
    valid.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap());

    let n = valid.len();
    let median = valid[n / 2];

    let mut deviations: Vec<f32> = valid.iter().map(|&p| (p - median).abs()).collect();
    deviations.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap());
    let mad = deviations[deviations.len() / 2];

    let c0 = (median + (-2.8_f32) * 1.4826 * mad).clamp(0.0, 1.0);

    let c0_range = (1.0 - c0).max(f32::EPSILON);
    let mut clipped: Vec<f32> = valid.iter()
        .filter(|&&p| p > c0)
        .map(|&p| (p - c0) / c0_range)
        .collect();
    if clipped.is_empty() { return (c0, 0.5); }
    clipped.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap());
    let clipped_median = clipped[clipped.len() / 2];

    let m = if clipped_median < f32::EPSILON { 0.5 } else { mtf_pub(0.25, clipped_median) };
    (c0, m)
}

#[inline(always)]
pub fn mtf_pub(m: f32, x: f32) -> f32 {
    if x <= 0.0 { return 0.0; }
    if x >= 1.0 { return 1.0; }
    if (m - 0.5).abs() < f32::EPSILON { return x; }
    (m - 1.0) * x / ((2.0 * m - 1.0) * x - m)
}


// ----------------------------------------------------------------------
