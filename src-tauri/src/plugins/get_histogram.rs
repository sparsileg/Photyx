// plugins/get_histogram.rs — GetHistogram built-in plugin
// Computes histogram statistics for the current frame's raw buffer.
// Returns median, std dev, clipping % as pcode output.
// Also stores results in AppContext for frontend retrieval.

use crate::plugin::{PhotonPlugin, ArgMap, ParamSpec, PluginOutput, PluginError};
use crate::context::{AppContext, PixelData};

pub struct GetHistogram;

impl PhotonPlugin for GetHistogram {
    fn name(&self) -> &str { "GetHistogram" }
    fn version(&self) -> &str { "1.0" }
    fn description(&self) -> &str { "Computes histogram statistics for the current frame" }
    fn parameters(&self) -> Vec<ParamSpec> { vec![] }

    fn execute(&self, ctx: &mut AppContext, _args: &ArgMap) -> Result<PluginOutput, PluginError> {
        let path = ctx.file_list.get(ctx.current_frame).cloned().ok_or_else(|| {
            PluginError::new("NO_IMAGE", "No image loaded.")
        })?;

        let buffer = ctx.image_buffers.get(&path).ok_or_else(|| {
            PluginError::new("NO_IMAGE", "Image buffer not found.")
        })?;

        let pixels = buffer.pixels.as_ref().ok_or_else(|| {
            PluginError::new("NO_PIXELS", "No pixel data.")
        })?;

        let stats = compute_stats(pixels);
        let filename = buffer.filename.clone();

        // Store in context for frontend retrieval
        ctx.last_histogram = Some(stats.clone());

        Ok(PluginOutput::Values(vec![
            format!("File:     {}", filename),
            format!("Median:   {:.0}", stats.median * 65535.0),
            format!("Std Dev:  {:.0}", stats.std_dev * 65535.0),
            format!("Clipping: {:.3}%", stats.clipping_pct),
        ]))
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct HistogramStats {
    pub bins:         Vec<u32>,   // 256 bins, normalized to 0-1 range
    pub median:       f64,
    pub mean:         f64,
    pub std_dev:      f64,
    pub clipping_pct: f64,
}

pub fn compute_stats(pixels: &PixelData) -> HistogramStats {
    let mut bins = vec![0u32; 256];
    let mut sum = 0.0f64;
    let mut count = 0usize;
    let mut clipped = 0usize;

    // Normalize all pixels to 0.0-1.0 and bin them
    match pixels {
        PixelData::U16(v) => {
            let clip_threshold = 65208u16; // 99.5% of 65535
            for &p in v {
                let norm = p as f64 / 65535.0;
                let bin = (norm * 255.0) as usize;
                bins[bin.min(255)] += 1;
                sum += norm;
                count += 1;
                if p >= clip_threshold { clipped += 1; }
            }
        }
        PixelData::F32(v) => {
            for &p in v {
                if !p.is_finite() { continue; }
                let norm = p.clamp(0.0, 1.0) as f64;
                let bin = (norm * 255.0) as usize;
                bins[bin.min(255)] += 1;
                sum += norm;
                count += 1;
                if norm >= 0.995 { clipped += 1; }
            }
        }
        PixelData::U8(v) => {
            let clip_threshold = 253u8;
            for &p in v {
                bins[p as usize] += 1;
                sum += p as f64 / 255.0;
                count += 1;
                if p >= clip_threshold { clipped += 1; }
            }
        }
    }

    let mean = if count > 0 { sum / count as f64 } else { 0.0 };

    // Compute median from bins
    let half = count / 2;
    let mut cumulative = 0usize;
    let mut median_bin = 0usize;
    for (i, &b) in bins.iter().enumerate() {
        cumulative += b as usize;
        if cumulative >= half {
            median_bin = i;
            break;
        }
    }
    let median = median_bin as f64 / 255.0;

    // Compute std dev
    let variance = match pixels {
        PixelData::U16(v) => {
            v.iter().map(|&p| {
                let norm = p as f64 / 65535.0;
                (norm - mean).powi(2)
            }).sum::<f64>() / count as f64
        }
        PixelData::F32(v) => {
            v.iter().filter(|p| p.is_finite()).map(|&p| {
                let norm = p.clamp(0.0, 1.0) as f64;
                (norm - mean).powi(2)
            }).sum::<f64>() / count as f64
        }
        PixelData::U8(v) => {
            v.iter().map(|&p| {
                let norm = p as f64 / 255.0;
                (norm - mean).powi(2)
            }).sum::<f64>() / count as f64
        }
    };

    let std_dev = variance.sqrt();
    let clipping_pct = if count > 0 { clipped as f64 / count as f64 * 100.0 } else { 0.0 };

    HistogramStats { bins, median, mean, std_dev, clipping_pct }
}



// ----------------------------------------------------------------------
