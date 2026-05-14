// plugins/add_files.rs — AddFiles built-in native plugin
// Appends explicit file paths to the session; does not clear existing files.

use tracing::info;
use crate::plugin::{PhotonPlugin, ArgMap, ParamSpec, ParamType, PluginOutput, PluginError};
use crate::context::AppContext;
use crate::plugins::image_reader::{read_image_file, peek_fits_dimensions, peek_xisf_dimensions, peek_tiff_dimensions};

pub struct AddFiles;

impl PhotonPlugin for AddFiles {
    fn name(&self)        -> &str { "AddFiles" }
    fn version(&self)     -> &str { "1.0" }
    fn description(&self) -> &str { "Appends a list of explicit file paths to the session" }

    fn parameters(&self) -> Vec<ParamSpec> {
        vec![
            ParamSpec {
                name:        "paths".to_string(),
                param_type:  ParamType::String,
                required:    true,
                description: "Comma-separated list of file paths to load".to_string(),
                default:     None,
            }
        ]
    }

    fn execute(&self, ctx: &mut AppContext, args: &ArgMap) -> Result<PluginOutput, PluginError> {
        let raw = args.get("paths")
            .ok_or_else(|| PluginError::missing_arg("paths"))?;

        // Split on comma, trim whitespace and quotes
        let paths: Vec<String> = raw
            .split(',')
            .map(|s| s.trim().trim_matches('"').to_string())
            .filter(|s| !s.is_empty())
            .collect();

        if paths.is_empty() {
            return Err(PluginError::new("NO_FILES", "No file paths provided"));
        }

        // Validate all paths exist before clearing session
        for path in &paths {
            if !std::path::Path::new(path).exists() {
                return Err(PluginError::new(
                    "FILE_NOT_FOUND",
                    &format!("File not found: '{}'", path),
                ));
            }
        }

        // ── Memory estimate and limit check ───────────────────────────────────
        // Peek the first file to get dimensions, then extrapolate across all files.
        let first = paths.first().unwrap(); // safe — we checked is_empty above
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
            (w as i64) * (h as i64) * (c as i64) * (bpp as i64) * (paths.len() as i64)
        });

        if let Some(raw_bytes) = estimated_bytes {
            let peak_bytes = (raw_bytes as f64 * 2.1) as i64;
            if peak_bytes > ctx.buffer_pool_bytes {
                return Err(PluginError::new(
                    "MEMORY_LIMIT_EXCEEDED",
                    &format!(
                        "Load cancelled: {} files require ~{:.1} GB of memory. Preferences limit is set to {:.1} GB.",
                        paths.len(),
                        peak_bytes as f64 / (1024.0 * 1024.0 * 1024.0),
                        ctx.buffer_pool_bytes as f64 / (1024.0 * 1024.0 * 1024.0),
                    ),
                ));
            }
        }

        let raw_mb  = estimated_bytes.unwrap_or(0) / (1024 * 1024);
        let peak_mb = (estimated_bytes.unwrap_or(0) as f64 * 2.1) as i64 / (1024 * 1024);

        // Filter out files already in the session
        let paths: Vec<String> = paths.into_iter()
            .filter(|p| !ctx.file_list.contains(p))
            .collect();

        if paths.is_empty() {
            return Ok(PluginOutput::Message("All selected files are already loaded.".to_string()));
        }

        let mut loaded = 0usize;
        let mut errors: Vec<String> = Vec::new();

        for path in &paths {
            match read_image_file(path) {
                Ok(buffer) => {
                    ctx.image_buffers.insert(path.clone(), buffer);
                    ctx.file_list.push(path.clone());
                    loaded += 1;
                }
                Err(e) => {
                    errors.push(format!("{}: {}", path, e));
                }
            }
        }

        info!("SelectFiles: loaded {} of {} files", loaded, paths.len());

        let mut msg = format!(
            "Loaded {} file(s) (~{} MB raw, ~{} MB peak with analysis).",
            loaded, raw_mb, peak_mb
        );
        if !errors.is_empty() {
            msg.push_str(&format!(" {} error(s).", errors.len()));
            for e in &errors {
                tracing::warn!("SelectFiles: {}", e);
            }
        }

        Ok(PluginOutput::Message(msg))
    }
}

// ----------------------------------------------------------------------
