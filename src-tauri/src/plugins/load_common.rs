// plugins/load_common.rs — shared post-glob load machinery (Issue 91)
//
// AddFiles and ReadImages both load images into the session but were built
// at different times, so only AddFiles got the memory-limit gate. This
// module is the first piece of consolidating the two onto shared logic —
// phase 1 covers just the memory gate; progress reporting and the basename
// re-sort/current_frame reset follow in later phases.

use crate::context::AppContext;
use crate::plugin::PluginError;
use crate::plugins::image_reader::{peek_fits_dimensions, peek_xisf_dimensions, peek_tiff_dimensions};

/// Estimate peak memory for loading `new_paths` and refuse the load if it
/// would breach `ctx.buffer_pool_bytes`.
///
/// Uses the first path's dimensions, extrapolated across the full list, at
/// the existing 2.1x peak multiplier (transient overhead during loading and
/// analysis), combined with the actual settled size of whatever is already
/// loaded. Checked against `new_paths` as given — deliberately *before* any
/// already-loaded/duplicate filtering the caller may do afterward, matching
/// AddFiles' original (conservative) behavior: it's better to occasionally
/// refuse a load that would have actually fit than to underestimate and
/// blow past the limit.
pub(crate) fn check_memory_limit(ctx: &AppContext, new_paths: &[String]) -> Result<(), PluginError> {
    let first = match new_paths.first() {
        Some(p) => p,
        None => return Ok(()),
    };

    let ext = std::path::Path::new(first)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    let estimated_bytes = match ext.as_str() {
        "fit" | "fits" | "fts" => peek_fits_dimensions(first),
        "xisf"                 => peek_xisf_dimensions(first),
        "tif" | "tiff"         => peek_tiff_dimensions(first),
        _                      => None,
    }.map(|(w, h, c, bpp)| {
        (w as i64) * (h as i64) * (c as i64) * (bpp as i64) * (new_paths.len() as i64)
    });

    if let Some(raw_bytes) = estimated_bytes {
        let already_loaded_bytes = ctx.total_memory_used() as i64;
        let new_peak_bytes       = (raw_bytes as f64 * 2.1) as i64;
        let projected_peak_bytes = already_loaded_bytes + new_peak_bytes;

        if projected_peak_bytes > ctx.buffer_pool_bytes {
            return Err(PluginError::new(
                "MEMORY_LIMIT_EXCEEDED",
                &format!(
                    "Load cancelled: {} new file(s) would bring total memory to ~{:.1} GB \
                     (~{:.1} GB already loaded + ~{:.1} GB for these files). \
                     Preferences limit is set to {:.1} GB.",
                    new_paths.len(),
                    projected_peak_bytes as f64 / (1024.0 * 1024.0 * 1024.0),
                    already_loaded_bytes as f64 / (1024.0 * 1024.0 * 1024.0),
                    new_peak_bytes as f64 / (1024.0 * 1024.0 * 1024.0),
                    ctx.buffer_pool_bytes as f64 / (1024.0 * 1024.0 * 1024.0),
                ),
            ));
        }
    }

    Ok(())
}

/// Re-sort the whole session by filename (not full path) and reset
/// current_frame to 0. Shared by AddFiles and ReadImages so both load
/// paths — and any mix of the two — leave the session in identical order.
/// Filenames are DTG-first, so this keeps capture order intact for
/// StackFrames' rotational grouping (Technical Reference §7.1), and a
/// rejected-then-re-added frame slots back into its original chronological
/// position instead of landing at the end.
///
/// Callers should only invoke this when at least one new path was actually
/// attempted (i.e. skip it on an all-duplicates no-op load) — matching the
/// existing behavior where a load that adds nothing leaves current_frame
/// untouched rather than resetting it on every call.
pub(crate) fn finalize_session_order(ctx: &mut AppContext) {
    ctx.file_list.sort_by(|a, b| {
        let a_name = a.rsplit(['/', '\\']).next().unwrap_or(a.as_str());
        let b_name = b.rsplit(['/', '\\']).next().unwrap_or(b.as_str());
        a_name.cmp(b_name)
    });
    ctx.current_frame = 0;
}

// ----------------------------------------------------------------------
// ----------------------------------------------------------------------
// ----------------------------------------------------------------------
