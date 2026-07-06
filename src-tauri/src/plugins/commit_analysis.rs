// plugins/commit_analysis.rs — CommitAnalysis plugin
// Spec §11.6

use crate::context::AppContext;
use crate::plugin::{ArgMap, ParamSpec, ParamType, PhotyxPlugin, PluginError, PluginOutput};

pub struct CommitAnalysis;

impl PhotyxPlugin for CommitAnalysis {
    fn name(&self)        -> &str { "CommitAnalysis" }
    fn version(&self)     -> &str { "1.0.0" }
    fn description(&self) -> &str {
        "Moves all REJECT frames to a rejected/ subfolder within each frame's source \
         directory and removes them from the session. Pass frames remain loaded. \
         Optionally appends a suffix to each moved filename."
    }

    fn parameters(&self) -> Vec<ParamSpec> {
        vec![
            ParamSpec {
                name:        "append".to_string(),
                param_type:  ParamType::String,
                required:    false,
                description: "Suffix appended to each moved filename after the original \
                              extension (e.g. append=.session → frame.fit.session). \
                              Leading dot is optional. Defaults to no suffix.".to_string(),
                default:     Some(String::new()),
            },
        ]
    }

    fn execute(&self, ctx: &mut AppContext, args: &ArgMap) -> Result<PluginOutput, PluginError> {
        let append = args.get("append").map(|s| s.as_str()).unwrap_or("");

        crate::commands::analysis::do_commit(ctx, append)
            .map(PluginOutput::Message)
            .map_err(|e| PluginError::new("COMMIT_FAILED", &e))
    }
}

// ----------------------------------------------------------------------
