// plugins/read_images.rs   ReadImages built-in native plugin
// Loads a single image file or all supported images in a directory
// into the session. Skips duplicates already in ctx.file_list.

use std::path::Path;
use tracing::info;
use crate::plugin::{PhotonPlugin, ArgMap, ParamSpec, ParamType, PluginOutput, PluginError};
use crate::context::AppContext;
use crate::plugins::image_reader::read_image_file;

pub struct ReadImages;

impl PhotonPlugin for ReadImages {
    fn name(&self)        -> &str { "ReadImages" }
    fn version(&self)     -> &str { "1.0" }
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

        let mut loaded  = 0;
        let mut skipped = 0;
        let mut errors  = 0;

        for file_path in &candidates {
            if existing.contains(file_path) {
                skipped += 1;
                continue;
            }
            match read_image_file(file_path) {
                Ok(buffer) => {
                    info!("ReadImages: loaded {}", file_path);
                    ctx.image_buffers.insert(file_path.clone(), buffer);
                    ctx.file_list.push(file_path.clone());
                    loaded += 1;
                }
                Err(e) => {
                    tracing::warn!("ReadImages: failed to load {}: {}", file_path, e);
                    errors += 1;
                }
            }
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
