// plugins/select_directory.rs — SelectDirectory built-in native plugin
// Spec §7.8

use std::path::Path;
use tracing::info;
use crate::plugin::{PhotonPlugin, ArgMap, ParamSpec, ParamType, PluginOutput, PluginError};
use crate::context::AppContext;

pub struct SelectDirectory;

impl PhotonPlugin for SelectDirectory {
    fn name(&self) -> &str { "SelectDirectory" }
    fn version(&self) -> &str { "1.0" }
    fn description(&self) -> &str { "Sets the active working directory" }

    fn parameters(&self) -> Vec<ParamSpec> {
        vec![
            ParamSpec {
                name:        "path".to_string(),
                param_type:  ParamType::Path,
                required:    true,
                description: "Directory path to set as active".to_string(),
                default:     None,
            }
        ]
    }

    fn execute(&self, ctx: &mut AppContext, args: &ArgMap) -> Result<PluginOutput, PluginError> {
        let path = args.get("path").ok_or_else(|| PluginError::missing_arg("path"))?;

        // Resolve ~ to home directory
        let resolved = if path.starts_with('~') {
            let home = std::env::var("USERPROFILE")
                .or_else(|_| std::env::var("HOME"))
                .unwrap_or_else(|_| ".".to_string());
            path.replacen('~', &home, 1)
        } else {
            path.clone()
        };

        // Normalise to forward slashes per spec §7.11
        let normalised = resolved.replace('\\', "/");

        if !Path::new(&normalised).exists() {
            return Err(PluginError::new(
                "DIR_NOT_FOUND",
                &format!("Directory not found: '{}'", normalised),
            ));
        }

        ctx.active_directory = Some(normalised.clone());
        ctx.file_list.clear();
        ctx.image_buffers.clear();
        ctx.current_frame = 0;

        info!("Active directory set to: {}", normalised);
        Ok(PluginOutput::Message(format!("Active directory: {}", normalised)))
    }
}
