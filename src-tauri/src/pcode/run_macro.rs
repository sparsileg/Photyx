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

        let resolved = crate::utils::resolve_path(filename, ctx.active_directory.as_deref());

        let script = std::fs::read_to_string(&resolved)
            .map_err(|e| PluginError::new("IO_ERROR",
                                          &format!("Cannot read macro file '{}': {}", resolved, e)))?;

        info!("RunMacro: executing '{}'", resolved);

        // We need access to the registry — it's stored in PhotoxState which we
        // can't access from here directly. Use a thread-local workaround via
        // a registry reference stored in AppContext.
        // For Phase 5 Step 1: execute via the global registry reference.
        let registry = crate::GLOBAL_REGISTRY.get()
            .ok_or_else(|| PluginError::new("INTERNAL", "Registry not initialized"))?;

        let results = crate::pcode::execute_script(&script, ctx, registry, true);

        let errors: Vec<_> = results.iter().filter(|r| !r.success).collect();
        let lines_run = results.len();

        if errors.is_empty() {
            Ok(PluginOutput::Message(format!(
                "Macro '{}' complete ({} commands)", resolved, lines_run
            )))
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
