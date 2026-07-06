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
            "flag":               if r.is_reference { "REF" } else { r.flag.as_ref().map(|f| f.as_str()).unwrap_or("PASS") },
            "triggered_by":       r.triggered_by,
            "rejection_category": r.rejection_category,
        }))
    }).collect();

    let outliers: Vec<String> = ctx.outlier_frame_paths.iter()
        .map(|p| p.rsplit(['/', '\\']).next().unwrap_or(p.as_str()).to_string())
        .collect();

    let session_stats = match &ctx.last_session_stats {
        Some(s) => serde_json::json!({
            "background_median": { "mean": s.background_median.mean, "stddev": s.background_median.stddev },
            "fwhm":              { "mean": s.fwhm.mean,              "stddev": s.fwhm.stddev },
            "eccentricity":      { "mean": s.eccentricity.mean,      "stddev": s.eccentricity.stddev },
            "star_count":        { "mean": s.star_count.mean,        "stddev": s.star_count.stddev },
        }),
        None => serde_json::json!({}),
    };

    let thresholds = {
        let t = &ctx.analysis_thresholds;
        serde_json::json!({
            "bg_median_reject_sigma":     t.background_median.reject,
            "fwhm_reject_sigma":          t.fwhm.reject,
            "star_count_reject_sigma":    t.star_count.reject,
            "eccentricity_reject_abs":    t.eccentricity.reject,
        })
    };

    serde_json::json!({
        "photyx": {
            "photyx_version": "1.0.0",
            "exported_at":    chrono::Utc::now().to_rfc3339(),
        },
        "statistics": {
            "session_stats": session_stats,
            "thresholds":    thresholds,
        },
        "frames":   frames,
        "outliers": outliers,
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

// ----------------------------------------------------------------------
