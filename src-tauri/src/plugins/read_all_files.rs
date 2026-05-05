// plugins/read_all_files.rs — ReadAll built-in native plugin
// Loads all supported image formats (FITS + XISF + TIFF) from the active directory.
// Spec §5.2, §6.3

use tracing::{info, warn};
use crate::plugin::{PhotonPlugin, ArgMap, ParamSpec, PluginOutput, PluginError};
use crate::context::AppContext;
use super::read_fits::{read_fits_file, peek_fits_dimensions};
use super::read_xisf::{read_xisf_file, peek_xisf_dimensions};
use super::read_tiff::{read_tiff_file, peek_tiff_dimensions};

pub struct ReadAll;

impl PhotonPlugin for ReadAll {
    fn name(&self)        -> &str { "ReadAll" }
    fn version(&self)     -> &str { "1.0" }
    fn description(&self) -> &str { "Reads all supported image files (FITS + XISF + TIFF) in the active directory" }
    fn parameters(&self)  -> Vec<ParamSpec> { vec![] }

    fn execute(&self, ctx: &mut AppContext, _args: &ArgMap) -> Result<PluginOutput, PluginError> {
        let dir = ctx.active_directory.clone().ok_or_else(|| {
            PluginError::new("NO_DIRECTORY", "No active directory. Use SelectDirectory first.")
        })?;

        let fits_extensions = ["fit", "fits", "fts"];
        let tiff_extensions = ["tif", "tiff"];

        let entries = std::fs::read_dir(&dir).map_err(|e| {
            PluginError::new("IO_ERROR", &format!("Cannot read directory '{}': {}", dir, e))
        })?;

        let mut fits_files: Vec<String> = Vec::new();
        let mut xisf_files: Vec<String> = Vec::new();
        let mut tiff_files: Vec<String> = Vec::new();

        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            let ext = path.extension()
                .and_then(|x| x.to_str())
                .unwrap_or("")
                .to_lowercase();
            let path_str = match path.to_str() {
                Some(s) => s.replace('\\', "/"),
                None => continue,
            };
            if fits_extensions.contains(&ext.as_str()) {
                fits_files.push(path_str);
            } else if ext == "xisf" {
                xisf_files.push(path_str);
            } else if tiff_extensions.contains(&ext.as_str()) {
                tiff_files.push(path_str);
            }
        }

        fits_files.sort();
        xisf_files.sort();
        tiff_files.sort();

        let total = fits_files.len() + xisf_files.len() + tiff_files.len();
        if total == 0 {
            return Ok(PluginOutput::Message(format!("No supported image files found in '{}'", dir)));
        }

        // ── Memory estimate and limit check ───────────────────────────────────
        // Peek the first file of whichever type appears first to get dimensions.
        let first_fits = fits_files.first();
        let first_xisf = xisf_files.first();
        let first_tiff = tiff_files.first();

        let estimated_bytes = if let Some(path) = first_fits {
            peek_fits_dimensions(path).map(|(w, h, c, bpp)| {
                (w as i64) * (h as i64) * (c as i64) * (bpp as i64) * (total as i64)
            })
        } else if let Some(path) = first_xisf {
            peek_xisf_dimensions(path).map(|(w, h, c, bpp)| {
                (w as i64) * (h as i64) * (c as i64) * (bpp as i64) * (total as i64)
            })
        } else if let Some(path) = first_tiff {
            peek_tiff_dimensions(path).map(|(w, h, c, bpp)| {
                (w as i64) * (h as i64) * (c as i64) * (bpp as i64) * (total as i64)
            })
        } else {
            None
        };

        if let Some(raw_bytes) = estimated_bytes {
            let peak_bytes = (raw_bytes as f64 * 2.1) as i64;
            if peak_bytes > ctx.buffer_pool_bytes {
                return Err(PluginError::new(
                    "MEMORY_LIMIT_EXCEEDED",
                    &format!(
                        "Load cancelled: {} files require ~{:.1} GB of memory. Preferences limit is set to {:.1} GB.",
                        total,
                        peak_bytes as f64 / (1024.0 * 1024.0 * 1024.0),
                        ctx.buffer_pool_bytes as f64 / (1024.0 * 1024.0 * 1024.0),
                    ),
                ));
            }
        }

        let raw_mb  = estimated_bytes.unwrap_or(0) / (1024 * 1024);
        let peak_mb = (estimated_bytes.unwrap_or(0) as f64 * 2.1) as i64 / (1024 * 1024);

        ctx.clear_session();

        let mut loaded = 0;
        let mut errors = 0;

        for path in fits_files.iter().chain(xisf_files.iter()).chain(tiff_files.iter()) {
            let ext = std::path::Path::new(path)
                .extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();

            let result = if ["fit","fits","fts"].contains(&ext.as_str()) {
                read_fits_file(path)
            } else if ext == "xisf" {
                read_xisf_file(path)
            } else {
                read_tiff_file(path)
            };

            match result {
                Ok(buffer) => {
                    info!("Loaded: {} ({}x{} {:?})", path, buffer.width, buffer.height, buffer.bit_depth);
                    ctx.file_list.push(path.clone());
                    ctx.image_buffers.insert(path.clone(), buffer);
                    loaded += 1;
                }
                Err(e) => {
                    warn!("Failed to load '{}': {}", path, e);
                    errors += 1;
                }
            }
        }

        ctx.current_frame = 0;

        let msg = if errors > 0 {
            format!(
                "Loaded {}/{} files ({} FITS, {} XISF, {} TIFF) (~{} MB raw, ~{} MB peak with analysis) ({} errors)",
                loaded, total, fits_files.len(), xisf_files.len(), tiff_files.len(), raw_mb, peak_mb, errors
            )
        } else {
            format!(
                "Loaded {} file(s) ({} FITS, {} XISF, {} TIFF) (~{} MB raw, ~{} MB peak with analysis)",
                loaded, fits_files.len(), xisf_files.len(), tiff_files.len(), raw_mb, peak_mb
            )
        };

        Ok(PluginOutput::Message(msg))
    }
}

// ----------------------------------------------------------------------
