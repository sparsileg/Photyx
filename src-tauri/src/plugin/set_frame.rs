// plugins/set_frame.rs — SetFrame built-in plugin
// Sets the current frame index. Used by UI navigation.

use crate::plugin::{PhotonPlugin, ArgMap, ParamSpec, ParamType, PluginOutput, PluginError};
use crate::context::AppContext;

pub struct SetFrame;

impl PhotonPlugin for SetFrame {
    fn name(&self) -> &str { "SetFrame" }
    fn version(&self) -> &str { "1.0" }
    fn description(&self) -> &str { "Sets the current frame index" }

    fn parameters(&self) -> Vec<ParamSpec> {
        vec![
            ParamSpec {
                name:        "index".to_string(),
                param_type:  ParamType::Integer,
                required:    true,
                description: "Zero-based frame index".to_string(),
                default:     None,
            },
        ]
    }

    fn execute(&self, ctx: &mut AppContext, args: &ArgMap) -> Result<PluginOutput, PluginError> {
        let index = args.get("index")
            .and_then(|v| v.parse::<usize>().ok())
            .ok_or_else(|| PluginError::new("BAD_ARG", "index must be a non-negative integer"))?;

        if index >= ctx.file_list.len() {
            return Err(PluginError::new(
                "OUT_OF_RANGE",
                &format!("index {} out of range (0–{})", index, ctx.file_list.len().saturating_sub(1)),
            ));
        }

        ctx.current_frame = index;
        Ok(PluginOutput::Message(format!("Frame set to {}", index)))
    }
}
