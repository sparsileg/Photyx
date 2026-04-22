// plugins/read_all_files.rs — ReadAllFiles built-in native plugin
// Loads all supported image formats (FITS + XISF) from the active directory.
// Spec §5.2, §6.3

use tracing::{info, warn};
use crate::plugin::{PhotonPlugin, ArgMap, ParamSpec, PluginOutput, PluginError};
use crate::context::AppContext;
use super::read_fits::read_fits_file;
use super::read_xisf::read_xisf_file;
use super::read_tiff::read_tiff_file;

pub struct ReadAllFiles;

impl PhotonPlugin for ReadAllFiles {
    fn name(&self) -> &str { "ReadAllFiles" }
    fn version(&self) -> &str { "1.0" }
    fn description(&self) -> &str { "Reads all supported image files (FITS + XISF) in the active directory" }

    fn parameters(&self) -> Vec<ParamSpec> { vec![] }

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
            return Ok(PluginOutput::Message(
                format!("No supported image files found in '{}'", dir)
            ));
        }

        ctx.file_list.clear();
        ctx.image_buffers.clear();
        ctx.display_cache.clear();
        ctx.full_res_cache.clear();

        let mut loaded = 0;
        let mut errors = 0;

        for path in &fits_files {
            match read_fits_file(path) {
                Ok(buffer) => {
                    info!("Loaded FITS: {} ({}x{} {:?})", path, buffer.width, buffer.height, buffer.bit_depth);
                    ctx.file_list.push(path.clone());
                    ctx.image_buffers.insert(path.clone(), buffer);
                    loaded += 1;
                }
                Err(e) => {
                    warn!("Failed to load FITS '{}': {}", path, e);
                    errors += 1;
                }
            }
        }

        for path in &xisf_files {
            match read_xisf_file(path) {
                Ok(buffer) => {
                    info!("Loaded XISF: {} ({}x{} {:?})", path, buffer.width, buffer.height, buffer.bit_depth);
                    ctx.file_list.push(path.clone());
                    ctx.image_buffers.insert(path.clone(), buffer);
                    loaded += 1;
                }
                Err(e) => {
                    warn!("Failed to load XISF '{}': {}", path, e);
                    errors += 1;
                }
            }
        }

        for path in &tiff_files {
            match read_tiff_file(path) {
                Ok(buffer) => {
                    info!("Loaded TIFF: {} ({}x{} {:?})", path, buffer.width, buffer.height, buffer.bit_depth);
                    ctx.file_list.push(path.clone());
                    ctx.image_buffers.insert(path.clone(), buffer);
                    loaded += 1;
                }
                Err(e) => {
                    warn!("Failed to load TIFF '{}': {}", path, e);
                    errors += 1;
                }
            }
        }

        ctx.current_frame = 0;

        let msg = if errors > 0 {
            format!("Loaded {}/{} files ({} FITS, {} XISF, {} TIFF, {} errors)",
                loaded, total, fits_files.len(), xisf_files.len(), tiff_files.len(), errors)
        } else {
            format!("Loaded {} file(s) ({} FITS, {} XISF, {} TIFF)",
                loaded, fits_files.len(), xisf_files.len(), tiff_files.len())
        };

        Ok(PluginOutput::Message(msg))
    }
}
