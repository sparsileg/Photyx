// plugins/run_macro.rs — RunMacro built-in native plugin
// Executes a macro stored in the SQLite database through the pcode interpreter.
// Spec §7.7, §6.3

use tracing::info;
use crate::plugin::{PhotonPlugin, ArgMap, ParamSpec, ParamType, PluginOutput, PluginError};
use crate::context::AppContext;

pub struct RunMacro;

impl PhotonPlugin for RunMacro {
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

        if errors.is_empty() {
            let msg = format!("Macro '{}' complete ({} commands)", name, lines_run);
            if client_actions.is_empty() {
                Ok(PluginOutput::Message(msg))
            } else {
                Ok(PluginOutput::Data(serde_json::json!({
                    "message":        msg,
                    "client_action":  client_actions.first(),
                    "client_actions": client_actions,
                })))
            }
        } else {
            Ok(PluginOutput::Message(format!(
                "Macro '{}' halted after {} command(s): {}",
                name,
                lines_run,
                errors[0].message.as_deref().unwrap_or("unknown error")
            )))
        }
    }
}

// ----------------------------------------------------------------------
