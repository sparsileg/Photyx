// plugins/fake_progress.rs — FakeProgress plugin
// Simulates a long-running plugin for testing the progress feedback pipeline.
use crate::plugin::{ArgMap, ParamSpec, ParamType, PhotonPlugin, PluginError, PluginOutput};
use crate::context::AppContext;
use crate::{PROGRESS_CURRENT, PROGRESS_TOTAL};
use std::sync::atomic::Ordering;

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

        PROGRESS_CURRENT.store(0, Ordering::Relaxed);
        PROGRESS_TOTAL.store(total, Ordering::Relaxed);

        for i in 1..=total {
            std::thread::sleep(std::time::Duration::from_millis(50));
            PROGRESS_CURRENT.store(i, Ordering::Relaxed);
        }

        PROGRESS_CURRENT.store(0, Ordering::Relaxed);
        PROGRESS_TOTAL.store(0, Ordering::Relaxed);

        Ok(PluginOutput::Data(serde_json::json!({
            "message": format!("FakeProgress complete ({} frames)", total),
            "frames_processed": total,
        })))
    }
}

// ----------------------------------------------------------------------
