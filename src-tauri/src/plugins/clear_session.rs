// plugins/clear_session.rs — ClearSession built-in plugin
// Clears all loaded image buffers and resets session state.
// Active directory is preserved.

use crate::plugin::{PhotonPlugin, ArgMap, ParamSpec, PluginOutput, PluginError};
use crate::context::AppContext;

pub struct ClearSession;

impl PhotonPlugin for ClearSession {
    fn name(&self) -> &str { "ClearSession" }
    fn version(&self) -> &str { "1.0" }
    fn description(&self) -> &str { "Clears all loaded images and resets session state. Active directory is preserved." }
    fn parameters(&self) -> Vec<ParamSpec> { vec![] }

    fn execute(&self, ctx: &mut AppContext, _args: &ArgMap) -> Result<PluginOutput, PluginError> {
        ctx.file_list.clear();
        ctx.image_buffers.clear();
        ctx.display_cache.clear();
        ctx.current_frame = 0;
        Ok(PluginOutput::Message("Session cleared.".to_string()))
    }
}
