// plugins/reject_current_frame.rs — RejectCurrentFrame built-in plugin
//
// Moves a single frame to a rejected/ subfolder within its own source
// directory, removing it from the session and all caches. Defaults to
// the current frame if no index is given — used both by pcode / standard
// view mode (omits index, acts on ctx.current_frame) and by the Blink
// panel's Reject button (passes an explicit index, since blink playback's
// local frame index can diverge from ctx.current_frame).

use crate::plugin::{PhotonPlugin, ArgMap, ParamSpec, ParamType, PluginOutput, PluginError};
use crate::context::AppContext;

pub struct RejectCurrentFrame;

impl PhotonPlugin for RejectCurrentFrame {
    fn name(&self) -> &str { "RejectCurrentFrame" }
    fn version(&self) -> &str { "1.0.0" }
    fn description(&self) -> &str {
        "Moves a single frame to a rejected/ subfolder within its own source \
         directory, removing it from the session and all caches. Defaults to \
         the current frame if index is not specified."
    }

    fn parameters(&self) -> Vec<ParamSpec> {
        vec![
            ParamSpec {
                name:        "index".to_string(),
                param_type:  ParamType::String,
                required:    false,
                description: "Zero-based frame index to reject. Defaults to the current frame.".to_string(),
                default:     None,
            },
            ParamSpec {
                name:        "append".to_string(),
                param_type:  ParamType::String,
                required:    false,
                description: format!(
                    "Suffix appended after the original filename extension (e.g. append=cloudy \
                     produces frame.fit.cloudy). Leading dot is optional. Defaults to \"{}\".",
                    crate::constants::REJECT_FILE_SUFFIX,
                ),
                default:     Some(crate::constants::REJECT_FILE_SUFFIX.to_string()),
            },
        ]
    }

    fn execute(&self, ctx: &mut AppContext, args: &ArgMap) -> Result<PluginOutput, PluginError> {
        let idx = args.get("index")
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(ctx.current_frame);

        let suffix = args.get("append")
            .map(|s| s.as_str())
            .unwrap_or(crate::constants::REJECT_FILE_SUFFIX);

        if ctx.file_list.is_empty() {
            return Err(PluginError::new("NO_FILES", "No files loaded."));
        }
        if idx >= ctx.file_list.len() {
            return Err(PluginError::new(
                "INDEX_OUT_OF_RANGE",
                &format!("Frame index {} out of range.", idx),
            ));
        }

        let path = ctx.file_list[idx].clone();
        let new_path = crate::commands::analysis::move_to_rejected(
            &path,
            suffix,
        ).map_err(|e| PluginError::new("MOVE_FAILED", &e))?;
        let new_index = ctx.remove_single_frame(idx).unwrap_or(0);

        tracing::info!("RejectCurrentFrame: rejected {} (index {})", new_path, idx);

        Ok(PluginOutput::Data(serde_json::json!({
            "message":       format!("Rejected: {}", new_path),
            "rejected_path": new_path,
            "new_index":     new_index,
            "frame_count":   ctx.file_list.len(),
        })))
    }
}
