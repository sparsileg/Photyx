// plugins/add_files.rs — AddFiles built-in native plugin
// Appends explicit file paths to the session; does not clear existing files.

use tracing::info;
use crate::plugin::{PhotyxPlugin, ArgMap, ParamSpec, ParamType, PluginOutput, PluginError};
use crate::context::AppContext;
use glob::glob;
use crate::plugins::image_reader::read_image_file;
use crate::plugins::load_common::{check_memory_limit, finalize_session_order};

pub struct AddFiles;

impl PhotyxPlugin for AddFiles {
    fn name(&self)        -> &str { "AddFiles" }
    fn version(&self)     -> &str { "1.1.0" }
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

        // Split on comma, trim whitespace and quotes; expand glob patterns
        let mut paths: Vec<String> = Vec::new();
        let mut glob_warnings: Vec<String> = Vec::new();

        for token in raw.split(',').map(|s| s.trim().trim_matches('"')).filter(|s| !s.is_empty()) {
            // Expand a leading ~ before glob matching — ~ isn't a glob-legal
            // character, so an unexpanded "~/lights/*.fit" would never match
            // anything and silently produce a "no files matched" warning.
            // No active-directory resolution here (relative, non-~ paths
            // are unaffected) — that's the separate, deferred AddFiles
            // relative-path question tracked in the `cd` command issue.
            let token = crate::utils::resolve_path(token, None);
            let token = token.as_str();

            let is_glob = token.contains('*') || token.contains('?') || token.contains('[');
            if is_glob {
                match glob(token) {
                    Ok(entries) => {
                        let mut matched = 0usize;
                        for entry in entries.flatten() {
                            if let Some(p) = entry.to_str() {
                                paths.push(p.to_string());
                                matched += 1;
                            }
                        }
                        if matched == 0 {
                            glob_warnings.push(format!("No files matched pattern: '{}'", token));
                        }
                    }
                    Err(e) => {
                        glob_warnings.push(format!("Invalid glob pattern '{}': {}", token, e));
                    }
                }
            } else {
                paths.push(token.to_string());
            }
        }

        if paths.is_empty() && glob_warnings.is_empty() {
            return Err(PluginError::new("NO_FILES", "No file paths provided"));
        }
        if paths.is_empty() {
            return Err(PluginError::new("NO_FILES", &glob_warnings.join("; ")));
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
        // Shared with ReadImages (Issue 91) so both load paths enforce the
        // same buffer-pool limit the same way. Checked against the full
        // glob-expanded path list, before the already-loaded filter below —
        // conservative on purpose (see load_common.rs).
        check_memory_limit(ctx, &paths)?;

        // Filter out files already in the session
        let paths: Vec<String> = paths.into_iter()
            .filter(|p| !ctx.file_list.contains(p))
            .collect();

        if paths.is_empty() {
            return Ok(PluginOutput::Message("All selected files are already loaded.".to_string()));
        }

        let mut loaded = 0usize;
        let mut errors: Vec<String> = Vec::new();
        let total_to_load = paths.len() as u32;

        crate::set_progress("Loading files", 0, total_to_load);

        for (i, path) in paths.iter().enumerate() {
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
            crate::set_progress("Loading files", (i + 1) as u32, total_to_load);
        }

        // Shared with ReadImages (Issue 91) so both load paths — and any
        // mix of the two — leave the session in identical order. See
        // load_common.rs for the full rationale (DTG-first capture order,
        // StackFrames grouping, current_frame reset).
        finalize_session_order(ctx);

        crate::set_progress("", 0, 0);

        info!("AddFiles: loaded {} of {} files", loaded, paths.len());

        // Cumulative totals across the whole session (not just this batch),
        // using actual loaded buffer sizes rather than the pre-load estimate
        // above (which only ever covered the new files being added, and was
        // extrapolated from peeking a single file's dimensions).
        let cumulative_raw_bytes = ctx.total_memory_used() as i64;
        let cumulative_raw_mb    = cumulative_raw_bytes / (1024 * 1024);
        let cumulative_peak_mb   = (cumulative_raw_bytes as f64 * 2.1) as i64 / (1024 * 1024);

        let mut msg = format!(
            "Loaded {} file(s) (~{} MB raw, ~{} MB peak with analysis).",
            loaded, cumulative_raw_mb, cumulative_peak_mb
        );
        if !glob_warnings.is_empty() {
            for w in &glob_warnings {
                tracing::warn!("AddFiles: {}", w);
            }
            msg.push_str(&format!(" {} glob warning(s): {}", glob_warnings.len(), glob_warnings.join("; ")));
        }
        if !errors.is_empty() {
            msg.push_str(&format!(" {} error(s).", errors.len()));
            for e in &errors {
                tracing::warn!("AddFiles: {}", e);
            }
        }

        Ok(PluginOutput::Message(msg))
    }
}

// ----------------------------------------------------------------------
