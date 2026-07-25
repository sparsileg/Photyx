// plugins/read_images.rs   ReadImages built-in native plugin
// Loads a single image file or all supported images in a directory
// into the session. Skips duplicates already in ctx.file_list.

use std::path::Path;
use tracing::info;
use crate::plugin::{PhotyxPlugin, ArgMap, ParamSpec, ParamType, PluginOutput, PluginError};
use crate::context::AppContext;
use crate::plugins::image_reader::read_image_file;
use crate::plugins::load_common::{build_blink_jpegs, finalize_session_order};

pub struct ReadImages;

impl PhotyxPlugin for ReadImages {
    fn name(&self)        -> &str { "ReadImages" }
    fn version(&self)     -> &str { "1.2.0" }
    fn description(&self) -> &str {
        "Loads a file or all supported images in a directory (FITS, XISF, TIFF) into the session"
    }

    fn parameters(&self) -> Vec<ParamSpec> {
        vec![
            ParamSpec {
                name:        "path".to_string(),
                param_type:  ParamType::String,
                required:    true,
                description: "Path to a single image file or a directory".to_string(),
                default:     None,
            },
        ]
    }

    fn execute(&self, ctx: &mut AppContext, args: &ArgMap) -> Result<PluginOutput, PluginError> {
        let raw_path = args.get("path")
            .ok_or_else(|| PluginError::missing_arg("path"))?;

        let path = crate::utils::resolve_path(
            raw_path,
            ctx.common_parent().as_ref().and_then(|p| p.to_str()),
        );

        let p = Path::new(&path);

        if !p.exists() {
            return Err(PluginError::new("NOT_FOUND", &format!("Path does not exist: {}", path)));
        }

        let existing: std::collections::HashSet<String> =
            ctx.file_list.iter().cloned().collect();

        let candidates: Vec<String> = if p.is_file() {
            vec![path.clone()]
        } else if p.is_dir() {
            let mut files: Vec<String> = std::fs::read_dir(&path)
                .map_err(|e| PluginError::new("IO_ERROR", &format!("Cannot read directory: {}", e)))?
                .filter_map(|entry| entry.ok())
                .filter_map(|entry| {
                    let fp = entry.path();
                    if !fp.is_file() { return None; }
                    let ext = fp.extension()?.to_str()?.to_lowercase();
                    if matches!(ext.as_str(), "fit" | "fits" | "fts" | "xisf" | "tif" | "tiff") {
                        fp.to_str().map(|s| s.to_string())
                    } else {
                        None
                    }
                })
                .collect();
            files.sort();
            files
        } else {
            return Err(PluginError::new("INVALID_PATH", &format!("Path is neither a file nor a directory: {}", path)));
        };

        if candidates.is_empty() {
            return Ok(PluginOutput::Message("No supported image files found.".to_string()));
        }

        // Memory gate retired (Issue 173): the load path no longer keeps
        // raw pixels resident, so session size is not RAM-bounded.

        // Pre-filter already-loaded duplicates before the loop, matching
        // AddFiles' structure — progress below is reported against the
        // actual loading work remaining, not the full directory listing.
        let to_load: Vec<String> = candidates.iter()
            .filter(|p| !existing.contains(*p))
            .cloned()
            .collect();
        let skipped = candidates.len() - to_load.len();

        let mut loaded = 0;
        let mut errors = 0;
        let total_to_load = to_load.len() as u32;

        crate::set_progress("Loading files", 0, total_to_load);

        for (i, file_path) in to_load.iter().enumerate() {
            match read_image_file(file_path) {
                Ok(mut buffer) => {
                    info!("ReadImages: loaded {}", file_path);
                    // Issue 173: build both blink thumbnails while the raw
                    // pixels are in hand, then discard the pixels — only
                    // metadata stays resident. Viewing reads from disk on
                    // demand (ensure_pixels_resident).
                    match build_blink_jpegs(&buffer) {
                        Ok((b12, b25)) => {
                            ctx.blink_cache_12.insert(file_path.clone(), b12);
                            ctx.blink_cache_25.insert(file_path.clone(), b25);
                        }
                        Err(e) => {
                            tracing::warn!("ReadImages: blink cache failed for {}: {}", file_path, e);
                        }
                    }
                    buffer.pixels = None;
                    ctx.image_buffers.insert(file_path.clone(), buffer);
                    ctx.file_list.push(file_path.clone());
                    loaded += 1;
                }
                Err(e) => {
                    tracing::warn!("ReadImages: failed to load {}: {}", file_path, e);
                    errors += 1;
                }
            }
            crate::set_progress("Loading files", (i + 1) as u32, total_to_load);
        }
        if loaded > 0 {
            ctx.blink_cache_status = crate::context::BlinkCacheStatus::Ready;
        }

        crate::set_progress("", 0, 0);

        // Matches AddFiles' ordering behavior (Issue 91) — only re-sort and
        // reset current_frame when at least one new path was actually
        // attempted; an all-duplicates no-op load leaves the session
        // untouched, same as AddFiles' early return in that case.
        if !to_load.is_empty() {
            finalize_session_order(ctx);
        }

        let msg = match (errors, skipped) {
            (0, 0) => format!("Loaded {} image(s)", loaded),
            (0, s) => format!("Loaded {} image(s), {} skipped (already in session)", loaded, s),
            (e, 0) => format!("Loaded {}/{} image(s) ({} errors)", loaded, candidates.len(), e),
            (e, s) => format!("Loaded {}/{} image(s), {} skipped, {} errors", loaded, candidates.len(), s, e),
        };

        Ok(PluginOutput::Message(msg))
    }
}
