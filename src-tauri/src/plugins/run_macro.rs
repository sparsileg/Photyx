// plugins/run_macro.rs — RunMacro built-in native plugin
// Executes a macro stored in the SQLite database through the pcode interpreter.
// Spec §7.7, §6.3

use tracing::info;
use crate::plugin::{PhotyxPlugin, ArgMap, ParamSpec, ParamType, PluginOutput, PluginError};
use crate::context::AppContext;

pub struct RunMacro;

impl PhotyxPlugin for RunMacro {
    fn name(&self)        -> &str { "RunMacro" }
    fn version(&self)     -> &str { "1.1" }
    fn description(&self) -> &str { "Executes a macro stored in the database" }

    fn parameters(&self) -> Vec<ParamSpec> {
        vec![
            ParamSpec {
                name:        "name".to_string(),
                param_type:  ParamType::String,
                required:    true,
                description: "Name of the macro to execute".to_string(),
                default:     None,
            },
        ]
    }

    fn execute(&self, ctx: &mut AppContext, args: &ArgMap) -> Result<PluginOutput, PluginError> {
        let name = args.get("name")
            .ok_or_else(|| PluginError::missing_arg("name"))?;

        // Look up the macro script from the database.
        let db = crate::GLOBAL_DB.get()
            .ok_or_else(|| PluginError::new("INTERNAL", "Global DB not initialized"))?;
        let db = db.lock().expect("global db lock poisoned");

        let script: String = db
            .query_row(
                "SELECT script FROM macros WHERE name = ?1",
                rusqlite::params![name],
                |row| row.get(0),
            )
            .map_err(|_| PluginError::new(
                "NOT_FOUND",
                &format!("No macro named '{}' found in database", name),
            ))?;

        // Release the DB lock before executing — the script may call other
        // plugins that also need the DB lock (e.g. a nested RunMacro).
        drop(db);

        info!("RunMacro: executing '{}'", name);

        let registry = crate::GLOBAL_REGISTRY.get()
            .ok_or_else(|| PluginError::new("INTERNAL", "Registry not initialized"))?;

        let results = crate::pcode::execute_script(&script, ctx, registry, true);

        let errors: Vec<_> = results.iter().filter(|r| !r.success).collect();
        let lines_run = results.len();

        // Collect client actions from inner results so the frontend can
        // execute them after run_script returns. Merges both the new
        // client_actions field and the legacy client_command data field.
        let mut client_actions: Vec<String> = results.iter()
            .flat_map(|r| r.client_actions.iter().cloned())
            .collect();
        let legacy: Vec<String> = results.iter()
            .filter_map(|r| {
                r.data.as_ref()
                    .and_then(|d| d.get("client_command"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            })
            .collect();
        client_actions.extend(legacy);

        // Collect inner output messages (exclude assignments, include Print and command output)
        let inner_output: Vec<String> = results.iter()
            .filter(|r| r.success && !r.command.to_lowercase().starts_with("set "))
            .filter_map(|r| r.message.clone())
            .filter(|m| !m.is_empty())
            .collect();

        if errors.is_empty() {
            let summary = format!("Macro '{}' complete ({} commands)", name, lines_run);
            let mut full_msg = inner_output.join("\n");
            if !full_msg.is_empty() { full_msg.push('\n'); }
            full_msg.push_str(&summary);

            if client_actions.is_empty() {
                Ok(PluginOutput::Message(full_msg))
            } else {
                Ok(PluginOutput::Data(serde_json::json!({
                    "message":        full_msg,
                    "client_action":  client_actions.first(),
                    "client_actions": client_actions,
                })))
            }
        } else {
            let summary = format!(
                "Macro '{}' halted after {} command(s): {}",
                name,
                lines_run,
                errors[0].message.as_deref().unwrap_or("unknown error")
            );
            let mut full_msg = inner_output.join("\n");
            if !full_msg.is_empty() { full_msg.push('\n'); }
            full_msg.push_str(&summary);
            Ok(PluginOutput::Message(full_msg))
        }
    }
}

// ----------------------------------------------------------------------
