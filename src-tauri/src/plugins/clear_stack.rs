// plugins/clear_stack.rs — ClearStack built-in plugin
// Discards the transient stack result and per-frame contribution data.
// Stacking document §3.2

use crate::plugin::{PhotonPlugin, ArgMap, ParamSpec, PluginOutput, PluginError};
use crate::context::AppContext;

pub struct ClearStack;

impl PhotonPlugin for ClearStack {
    fn name(&self) -> &str { "ClearStack" }
    fn version(&self) -> &str { "1.0" }
    fn description(&self) -> &str { "Discards the transient stacked result and per-frame contribution data, returning the viewer to the normal session image." }
    fn parameters(&self) -> Vec<ParamSpec> { vec![] }

    fn execute(&self, ctx: &mut AppContext, _args: &ArgMap) -> Result<PluginOutput, PluginError> {
        if ctx.stack_result.is_none() {
            return Ok(PluginOutput::Message("ClearStack: no stack result to clear.".to_string()));
        }
        ctx.clear_stack();
        Ok(PluginOutput::Message("Stack result cleared.".to_string()))
    }
}
