// plugins/export_analysis_report.rs — ExportAnalysisReport plugin
// Spec §11

use crate::context::AppContext;
use crate::plugin::{ArgMap, ParamSpec, ParamType, PhotyxPlugin, PluginError, PluginOutput};

pub struct ExportAnalysisReport;

impl PhotyxPlugin for ExportAnalysisReport {
    fn name(&self)        -> &str { "ExportAnalysisReport" }
    fn version(&self)     -> &str { "1.0.0" }
    fn description(&self) -> &str {
        "Exports the current analysis results as a Photyx session JSON file. \
         If path is omitted, a filename is derived from the first frame and \
         written to the system Downloads folder."
    }

    fn parameters(&self) -> Vec<ParamSpec> {
        vec![
            ParamSpec {
                name:        "path".to_string(),
                param_type:  ParamType::String,
                required:    false,
                description: "Full destination path for the JSON file \
                              (e.g. path=\"D:/projects/M64/report.json\"). \
                              If omitted, written to the Downloads folder with \
                              an auto-derived filename.".to_string(),
                default:     None,
            },
        ]
    }

    fn execute(&self, ctx: &mut AppContext, args: &ArgMap) -> Result<PluginOutput, PluginError> {
        if ctx.analysis_results.is_empty() {
            return Err(PluginError::new(
                "NO_RESULTS",
                "No analysis results to export. Run AnalyzeFrames first.",
            ));
        }

        // ── Resolve output path ───────────────────────────────────────────────
        let out_path: std::path::PathBuf = if let Some(p) = args.get("path") {
            std::path::PathBuf::from(p)
        } else {
            let filename = derive_filename(ctx);
            let downloads = dirs_next::download_dir()
                .ok_or_else(|| PluginError::new("NO_DOWNLOADS", "Cannot locate Downloads folder."))?;
            downloads.join(filename)
        };

        // ── Build JSON ────────────────────────────────────────────────────────
        let json = build_report_json(ctx);

        // ── Write file ────────────────────────────────────────────────────────
        let json_str = serde_json::to_string_pretty(&json)
            .map_err(|e| PluginError::new("SERIALIZE_FAILED", &e.to_string()))?;

        std::fs::write(&out_path, &json_str)
            .map_err(|e| PluginError::new("WRITE_FAILED", &e.to_string()))?;

        let out_str = out_path.to_string_lossy().replace('\\', "/");
        tracing::info!("ExportAnalysisReport: wrote {}", out_str);

        Ok(PluginOutput::Message(format!("Analysis report exported to {}", out_str)))
    }
}

// ── JSON assembly ─────────────────────────────────────────────────────────────

fn build_report_json(ctx: &AppContext) -> serde_json::Value {
    let frames: Vec<serde_json::Value> = ctx.file_list.iter().filter_map(|path| {
        let r = ctx.analysis_results.get(path)?;
        let short = path.rsplit(['/', '\\']).next().unwrap_or(path.as_str());
        Some(serde_json::json!({
            "filename":           short,
            "fwhm":               r.fwhm,
            "eccentricity":       r.eccentricity,
            "star_count":         r.star_count,
            "background_median":  r.background_median,
            "flag":               r.flag.as_ref().map(|f| f.as_str()).unwrap_or("PASS"),
            "is_reference":       r.is_reference,
            "triggered_by":       r.triggered_by,
            "rejection_category": r.rejection_category,
        }))
    }).collect();

    let outliers: Vec<String> = ctx.outlier_frame_paths.iter()
        .map(|p| p.rsplit(['/', '\\']).next().unwrap_or(p.as_str()).to_string())
        .collect();

    let zero_stats = || serde_json::json!({ "mean": 0.0_f32, "stddev": 0.0_f32 });
    let session_stats = match &ctx.last_session_stats {
        Some(s) => serde_json::json!({
            "background_median": { "mean": s.background_median.mean, "stddev": s.background_median.stddev },
            "fwhm":              { "mean": s.fwhm.mean,              "stddev": s.fwhm.stddev },
            "eccentricity":      { "mean": s.eccentricity.mean,      "stddev": s.eccentricity.stddev },
            "star_count":        { "mean": s.star_count.mean,        "stddev": s.star_count.stddev },
        }),
        // AnalysisJsonPayload's SessionStatsPayload requires all four metric
        // blocks — keep the shape import-valid even in the rare case
        // analysis_results is populated without a session-stats run behind
        // it, rather than emitting a bare "{}" that fails re-import.
        None => serde_json::json!({
            "background_median": zero_stats(),
            "fwhm":               zero_stats(),
            "eccentricity":       zero_stats(),
            "star_count":         zero_stats(),
        }),
    };

    // Use the thresholds actually applied by the run being exported, not
    // whatever profile happens to be active right now — these can differ
    // whenever AnalyzeFrames ran with an explicit profile= (including via
    // the Analyze Frames menu picker) and the active profile changed
    // afterward, before Export was clicked. Falls back to the active
    // profile only if nothing has been analyzed yet in this session.
    let thresholds = {
        let t = ctx.last_analysis_thresholds.as_ref().unwrap_or(&ctx.analysis_thresholds);
        serde_json::json!({
            "bg_median_reject_sigma":     t.background_median.reject,
            "fwhm_reject_sigma":          t.fwhm.reject,
            "star_count_reject_sigma":    t.star_count.reject,
            "eccentricity_reject_abs":    t.eccentricity.reject,
        })
    };

    // Top-level shape matches AnalysisJsonPayload exactly (thresholds,
    // session_stats, outliers, frames) so a file this plugin writes can be
    // fed straight back into load_analysis_json — see Issue 115.
    // "photyx" is extra metadata; unknown fields are ignored on import.
    serde_json::json!({
        "photyx": {
            "photyx_version": "1.0.0",
            "exported_at":    chrono::Utc::now().to_rfc3339(),
        },
        "thresholds":    thresholds,
        "session_stats": session_stats,
        "outliers":      outliers,
        "frames":        frames,
    })
}

// ── Filename derivation ───────────────────────────────────────────────────────

fn derive_filename(ctx: &AppContext) -> String {
    if let Some(first_path) = ctx.file_list.first() {
        let short = first_path.rsplit(['/', '\\']).next().unwrap_or("");
        let target_match = {
            let re_target = short.strip_prefix("Light_")
                .and_then(|s| s.split('_').next());
            re_target
        };
        let date_match = short.find(|c: char| c.is_ascii_digit())
            .and_then(|_| {
                // Find YYYYMMDD-HHMMSS pattern
                let mut i = 0;
                let chars: Vec<char> = short.chars().collect();
                while i + 15 <= chars.len() {
                    let segment: String = chars[i..i+15].iter().collect();
                    if segment.chars().take(8).all(|c| c.is_ascii_digit())
                        && segment.chars().nth(8) == Some('-')
                        && segment.chars().skip(9).take(6).all(|c| c.is_ascii_digit())
                    {
                        return Some(segment[..8].to_string());
                    }
                    i += 1;
                }
                None
            });

        match (target_match, date_match) {
            (Some(t), Some(d)) => return format!("{}_{}_{}.json", t, d, "analysis"),
            (Some(t), None)    => return format!("{}_{}.json", t, "analysis"),
            _                  => {}
        }
    }
"session.json".to_string()
}

// ── Round-trip test ─────────────────────────────────────────────────────────
// Guards Issue 115: asserts that whatever build_report_json emits actually
// deserializes as AnalysisJsonPayload (commands/analysis.rs), and that the
// values that matter — flag, is_reference, outliers, and thresholds sourced
// from the run that was actually exported — survive the round trip intact.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::{AnalysisResult, PxFlag};
    use crate::analysis::session_stats::{
        AnalysisThresholds, MetricThresholds, SessionStats, MetricStats,
    };
    use crate::commands::analysis::AnalysisJsonPayload;

    fn sample_context() -> AppContext {
        let mut ctx = AppContext::default();

        ctx.file_list = vec![
            "/data/M31/lights/frame001.fit".to_string(),
            "/data/M31/lights/frame002.fit".to_string(),
            "/data/M31/lights/frame003.fit".to_string(),
        ];

        ctx.analysis_results.insert("/data/M31/lights/frame001.fit".to_string(), AnalysisResult {
            filename:           "frame001.fit".to_string(),
            background_median:  Some(120.5),
            fwhm:                Some(2.8),
            eccentricity:        Some(0.15),
            star_count:          Some(340),
            flag:                Some(PxFlag::Pass),
            triggered_by:        vec![],
            rejection_category:  None,
            is_reference:        true,
        });
        ctx.analysis_results.insert("/data/M31/lights/frame002.fit".to_string(), AnalysisResult {
            filename:           "frame002.fit".to_string(),
            background_median:  Some(340.2),
            fwhm:                Some(5.1),
            eccentricity:        Some(0.42),
            star_count:          Some(90),
            flag:                Some(PxFlag::Reject),
            triggered_by:        vec!["fwhm".to_string(), "background_median".to_string()],
            rejection_category:  Some("OB".to_string()),
            is_reference:        false,
        });
        ctx.analysis_results.insert("/data/M31/lights/frame003.fit".to_string(), AnalysisResult {
            filename:           "frame003.fit".to_string(),
            background_median:  Some(118.9),
            fwhm:                Some(2.9),
            eccentricity:        Some(0.18),
            star_count:          Some(310),
            flag:                Some(PxFlag::Pass),
            triggered_by:        vec![],
            rejection_category:  None,
            is_reference:        false,
        });

        ctx.outlier_frame_paths =
            std::collections::HashSet::from(["/data/M31/lights/frame002.fit".to_string()]);

        ctx.last_session_stats = Some(SessionStats {
            background_median: MetricStats { mean: 190.0, stddev: 45.0 },
            fwhm:               MetricStats { mean: 3.2,   stddev: 0.6 },
            eccentricity:       MetricStats { mean: 0.22,  stddev: 0.08 },
            star_count:         MetricStats { mean: 250.0, stddev: 60.0 },
        });

        // Active profile deliberately differs from the run's actual
        // thresholds, so this test fails loudly if export ever regresses
        // to reading the wrong source.
        ctx.analysis_thresholds = AnalysisThresholds {
            background_median: MetricThresholds { reject: 9.9 },
            fwhm:               MetricThresholds { reject: 9.9 },
            star_count:         MetricThresholds { reject: 9.9 },
            eccentricity:       MetricThresholds { reject: 9.9 },
        };
        ctx.last_analysis_thresholds = Some(AnalysisThresholds {
            background_median: MetricThresholds { reject: 2.5 },
            fwhm:               MetricThresholds { reject: 2.5 },
            star_count:         MetricThresholds { reject: 1.75 },
            eccentricity:       MetricThresholds { reject: 0.85 },
        });

        ctx
    }

    #[test]
    fn export_round_trips_through_import_payload() {
        let ctx = sample_context();

        let json = build_report_json(&ctx);
        let json_str = serde_json::to_string(&json).expect("serialize");

        let payload: AnalysisJsonPayload = serde_json::from_str(&json_str)
            .expect("exported JSON must deserialize as AnalysisJsonPayload — Issue 115");

        assert_eq!(payload.frames.len(), 3, "all three frames must survive export");

        let by_name = |name: &str| payload.frames.iter()
            .find(|f| f.filename == name)
            .unwrap_or_else(|| panic!("frame {} missing from exported payload", name));

        let f1 = by_name("frame001.fit");
        assert_eq!(f1.flag, "PASS");
        assert!(f1.is_reference, "reference frame must round-trip as is_reference=true");

        let f2 = by_name("frame002.fit");
        assert_eq!(f2.flag, "REJECT", "a REJECT frame must keep its true flag, not collapse to REF");
        assert_eq!(f2.rejection_category.as_deref(), Some("OB"));
        assert!(!f2.is_reference);

        let f3 = by_name("frame003.fit");
        assert_eq!(f3.flag, "PASS");
        assert!(!f3.is_reference);

        assert_eq!(
            payload.outliers,
            vec!["frame002.fit".to_string()],
            "outliers must survive as basenames, matching frame filenames' convention"
        );

        // Thresholds must come from last_analysis_thresholds (the run that
        // was actually exported), not the active profile.
        assert_eq!(payload.thresholds.fwhm_reject_sigma, 2.5);
        assert_eq!(payload.thresholds.star_count_reject_sigma, 1.75);
        assert_ne!(
            payload.thresholds.fwhm_reject_sigma, 9.9,
            "export must not fall back to the active profile when a run has actually happened"
        );

        assert_eq!(payload.session_stats.fwhm.mean, 3.2);
        assert_eq!(payload.session_stats.star_count.stddev, 60.0);
    }

    #[test]
    fn export_falls_back_to_active_profile_when_nothing_analyzed_yet() {
        let mut ctx = AppContext::default();
        ctx.analysis_thresholds = AnalysisThresholds {
            background_median: MetricThresholds { reject: 3.0 },
            fwhm:               MetricThresholds { reject: 3.0 },
            star_count:         MetricThresholds { reject: 1.5 },
            eccentricity:       MetricThresholds { reject: 0.9 },
        };
        // last_analysis_thresholds intentionally left None — nothing has
        // been analyzed in this context.

        // build_report_json itself doesn't gate on analysis_results being
        // non-empty (that check lives in execute()) — call it directly to
        // test the threshold fallback and session_stats shape in isolation.
        let json = build_report_json(&ctx);
        let payload: AnalysisJsonPayload = serde_json::from_value(json)
            .expect("empty-session export must still deserialize");

        assert_eq!(payload.thresholds.fwhm_reject_sigma, 3.0);
        assert_eq!(payload.thresholds.star_count_reject_sigma, 1.5);

        // session_stats must be shape-valid (zeroed, not "{}") even with no
        // last_session_stats set.
        assert_eq!(payload.session_stats.fwhm.mean, 0.0);
    }
}

// ----------------------------------------------------------------------
// ----------------------------------------------------------------------
// ----------------------------------------------------------------------
