// plugins/run_macro.rs — RunMacro built-in native plugin
// Executes a saved .phs macro file through the pcode interpreter.
// Spec §7.7, §6.3

use tracing::info;
use crate::plugin::{PhotonPlugin, ArgMap, ParamSpec, ParamType, PluginOutput, PluginError};
use crate::context::AppContext;

pub struct RunMacro;

impl PhotonPlugin for RunMacro {
    fn name(&self)        -> &str { "RunMacro" }
    fn version(&self)     -> &str { "1.0" }
    fn description(&self) -> &str { "Executes a saved .phs macro file" }

    fn parameters(&self) -> Vec<ParamSpec> {
        vec![
            ParamSpec {
                name:        "filename".to_string(),
                param_type:  ParamType::Path,
                required:    true,
                description: "Path to the .phs macro file".to_string(),
                default:     None,
            },
        ]
    }

    fn execute(&self, ctx: &mut AppContext, args: &ArgMap) -> Result<PluginOutput, PluginError> {
        let filename = args.get("filename")
            .ok_or_else(|| PluginError::missing_arg("filename"))?;

        // Resolve the macro path — if no directory separator and no .phs extension,
        // look in the macros directory automatically
        let resolved = {
            let p = std::path::Path::new(filename);
            if p.is_absolute() || p.components().count() > 1 {
                // Already a full or relative path
                crate::utils::resolve_path(filename, ctx.active_directory.as_deref())
            } else {
                // Bare name — look in macros directory, add .phs if needed
                let name = if filename.to_lowercase().ends_with(".phs") {
                    filename.to_string()
                } else {
                    format!("{}.phs", filename)
                };
                let macros_dir = crate::utils::get_macros_dir();
                macros_dir.join(name).to_str().unwrap_or(filename).to_string()
            }
        };

        let script = std::fs::read_to_string(&resolved)
            .map_err(|e| PluginError::new("IO_ERROR",
                &format!("Cannot read macro file '{}': {}", resolved, e)))?;

        info!("RunMacro: executing '{}'", resolved);

        let registry = crate::GLOBAL_REGISTRY.get()
            .ok_or_else(|| PluginError::new("INTERNAL", "Registry not initialized"))?;

        let results = crate::pcode::execute_script(&script, ctx, registry, true);

        let errors: Vec<_> = results.iter().filter(|r| !r.success).collect();
        let lines_run = results.len();

        if errors.is_empty() {
            let mut output = results.iter()
                .filter_map(|r| r.message.as_deref())
                .filter(|m| !m.is_empty())
                .collect::<Vec<_>>()
                .join("\n");
            if output.is_empty() {
                output = format!("Macro '{}' complete ({} commands)", resolved, lines_run);
            }
            Ok(PluginOutput::Message(output))
        } else {
            Ok(PluginOutput::Message(format!(
                "Macro '{}' halted after {} command(s): {}",
                resolved,
                lines_run,
                errors[0].message.as_deref().unwrap_or("unknown error")
            )))
        }
    }
}

// ----------------------------------------------------------------------
