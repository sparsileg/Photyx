// plugins/list_keywords.rs — ListKeywords built-in plugin
// Returns all keywords for the current frame as formatted text.

use crate::plugin::{PhotonPlugin, ArgMap, ParamSpec, PluginOutput, PluginError};
use crate::context::AppContext;

pub struct ListKeywords;

impl PhotonPlugin for ListKeywords {
    fn name(&self) -> &str { "ListKeywords" }
    fn version(&self) -> &str { "1.0" }
    fn description(&self) -> &str { "Lists all header keywords for the current frame" }
    fn parameters(&self) -> Vec<ParamSpec> { vec![] }

    fn execute(&self, ctx: &mut AppContext, _args: &ArgMap) -> Result<PluginOutput, PluginError> {
        let path = ctx.file_list.get(ctx.current_frame).cloned().ok_or_else(|| {
            PluginError::new("NO_IMAGE", "No image loaded.")
        })?;

        let buffer = ctx.image_buffers.get(&path).ok_or_else(|| {
            PluginError::new("NO_IMAGE", "Image buffer not found.")
        })?;

        if buffer.keywords.is_empty() {
            return Ok(PluginOutput::Message("No keywords found.".to_string()));
        }

        let mut sorted: Vec<_> = buffer.keywords.values().collect();
        sorted.sort_by(|a, b| a.name.cmp(&b.name));

        let lines: Vec<String> = sorted.iter().map(|kw| {
            let comment = kw.comment.as_deref().unwrap_or("");
            if comment.is_empty() {
                format!("{:<10} = {}", kw.name, kw.value)
            } else {
                format!("{:<10} = {}  / {}", kw.name, kw.value, comment)
            }
        }).collect();

        Ok(PluginOutput::Values(lines))
    }
}


// ----------------------------------------------------------------------
