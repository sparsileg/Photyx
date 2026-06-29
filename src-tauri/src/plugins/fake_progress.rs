// plugins/fake_progress.rs — FakeProgress plugin
// Simulates a long-running plugin for testing the progress feedback pipeline.
use crate::plugin::{ArgMap, ParamSpec, ParamType, PhotonPlugin, PluginError, PluginOutput};
use crate::context::AppContext;

pub struct FakeProgress;

impl PhotonPlugin for FakeProgress {
    fn name(&self) -> &str { "FakeProgress" }
    fn version(&self) -> &str { "1.0" }
    fn description(&self) -> &str { "Simulates a long-running plugin for progress pipeline testing" }

    fn parameters(&self) -> Vec<ParamSpec> {
        vec![
            ParamSpec {
                name:        "frames".to_string(),
                param_type:  ParamType::Integer,
                required:    false,
                description: "Number of simulated frames (default 128)".to_string(),
                default:     Some("128".to_string()),
            },
        ]
    }

    fn execute(&self, _ctx: &mut AppContext, args: &ArgMap) -> Result<PluginOutput, PluginError> {
        let total = args.get("frames")
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(128);

        crate::set_progress("Simulating", 0, total);

        for i in 1..=total {
            std::thread::sleep(std::time::Duration::from_millis(50));
            crate::set_progress("Simulating", i, total);
        }

        crate::set_progress("", 0, 0);

        Ok(PluginOutput::Data(serde_json::json!({
            "message": format!("FakeProgress complete ({} frames)", total),
            "frames_processed": total,
        })))
    }
}

// ----------------------------------------------------------------------
