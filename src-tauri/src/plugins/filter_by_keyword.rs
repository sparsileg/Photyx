// plugins/filter_by_keyword.rs — FilterByKeyword built-in plugin
// Filters the session file list down to frames whose keyword matches the
// given value. Non-matching frames are purged from image_buffers and the
// display/blink caches (not just file_list) so they don't linger in memory
// or produce stale UI state — mirrors the cleanup MoveFile already does.

use tracing::info;
use crate::plugin::{PhotyxPlugin, ArgMap, ParamSpec, ParamType, PluginOutput, PluginError};
use crate::context::AppContext;

pub struct FilterByKeyword;

impl PhotyxPlugin for FilterByKeyword {
    fn name(&self)        -> &str { "FilterByKeyword" }
    fn version(&self)     -> &str { "1.0" }
    fn description(&self) -> &str { "Filters the session file list to frames where the given keyword matches the given value" }

    fn parameters(&self) -> Vec<ParamSpec> {
        vec![
            ParamSpec {
                name:        "name".to_string(),
                param_type:  ParamType::String,
                required:    true,
                description: "Keyword name to filter on".to_string(),
                default:     None,
            },
            ParamSpec {
                name:        "value".to_string(),
                param_type:  ParamType::String,
                required:    true,
                description: "Value to match (case-insensitive)".to_string(),
                default:     None,
            },
        ]
    }

    fn execute(&self, ctx: &mut AppContext, args: &ArgMap) -> Result<PluginOutput, PluginError> {
        let name = args.get("name")
            .ok_or_else(|| PluginError::missing_arg("name"))?
            .trim()
            .to_uppercase();
        let value = args.get("value")
            .ok_or_else(|| PluginError::missing_arg("value"))?
            .trim()
            .to_lowercase();

        if name.is_empty() {
            return Err(PluginError::invalid_arg("name", "keyword name cannot be empty"));
        }

        if ctx.file_list.is_empty() {
            return Ok(PluginOutput::Message("No files loaded.".to_string()));
        }

        let total = ctx.file_list.len();
        let mut kept:    Vec<String> = Vec::new();
        let mut removed: Vec<String> = Vec::new();

        for path in ctx.file_list.iter() {
            let matches = ctx.image_buffers.get(path)
                .map(|buffer| {
                    buffer.keywords.iter()
                        .find(|(k, _)| k.to_uppercase() == name)
                        .map(|(_, kw)| kw.value.trim().to_lowercase() == value)
                        .unwrap_or(false)
                })
                .unwrap_or(false);

            if matches {
                kept.push(path.clone());
            } else {
                removed.push(path.clone());
            }
        }

        for path in &removed {
            ctx.remove_frame_data(path);
        }

        ctx.file_list = kept;

        if ctx.file_list.is_empty() {
            ctx.current_frame = 0;
        } else if ctx.current_frame >= ctx.file_list.len() {
            ctx.current_frame = ctx.file_list.len() - 1;
        }

        info!(
            "FilterByKeyword: {} = {} — kept {}/{} frame(s)",
            name, value, ctx.file_list.len(), total
        );
        Ok(PluginOutput::Message(format!(
            "FilterByKeyword: kept {}/{} frame(s) where {} = {}",
            ctx.file_list.len(), total, name, value
        )))
    }
}

// ----------------------------------------------------------------------
