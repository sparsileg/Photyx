// plugins/get_histogram.rs — GetHistogram built-in plugin
// Computes histogram statistics for the current frame's raw buffer.
// For mono images: single histogram. For RGB: per-channel histograms.

use crate::plugin::{PhotonPlugin, ArgMap, ParamSpec, PluginOutput, PluginError};
use crate::context::{AppContext, ColorSpace, PixelData};

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

        let is_rgb = buffer.channels == 3 && buffer.color_space == ColorSpace::RGB;
        let stats = compute_stats(pixels, is_rgb);
        let filename = buffer.filename.clone();

        ctx.last_histogram = Some(stats.clone());

        let output = if is_rgb {
            vec![
                format!("File:     {}", filename),
                format!("Median R/G/B: {:.0}/{:.0}/{:.0}",
                    stats.median * 65535.0,
                    stats.median_g.unwrap_or(0.0) * 65535.0,
                    stats.median_b.unwrap_or(0.0) * 65535.0),
                format!("Std Dev R/G/B: {:.0}/{:.0}/{:.0}",
                    stats.std_dev * 65535.0,
                    stats.std_dev_g.unwrap_or(0.0) * 65535.0,
                    stats.std_dev_b.unwrap_or(0.0) * 65535.0),
                format!("Clipping: {:.3}%", stats.clipping_pct),
            ]
        } else {
            vec![
                format!("File:     {}", filename),
                format!("Median:   {:.0}", stats.median * 65535.0),
                format!("Std Dev:  {:.0}", stats.std_dev * 65535.0),
                format!("Clipping: {:.3}%", stats.clipping_pct),
            ]
        };

        Ok(PluginOutput::Values(output))
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct HistogramStats {
    pub bins:         Vec<u32>,         // 256 bins for channel 0 (or mono)
    pub bins_g:       Option<Vec<u32>>, // 256 bins for green channel (RGB only)
    pub bins_b:       Option<Vec<u32>>, // 256 bins for blue channel (RGB only)
    pub median:       f64,              // channel 0 (R or mono)
    pub median_g:     Option<f64>,
    pub median_b:     Option<f64>,
    pub mean:         f64,
    pub std_dev:      f64,
    pub std_dev_g:    Option<f64>,
    pub std_dev_b:    Option<f64>,
    pub clipping_pct: f64,
}

pub fn compute_stats(pixels: &PixelData, is_rgb: bool) -> HistogramStats {
    if is_rgb {
        compute_rgb_stats(pixels)
    } else {
        compute_mono_stats(pixels)
    }
}

fn compute_mono_stats(pixels: &PixelData) -> HistogramStats {
    let mut bins = vec![0u32; 256];
    let mut sum = 0.0f64;
    let mut count = 0usize;
    let mut clipped = 0usize;
    let mut normalized: Vec<f64> = Vec::new();

    const STRIDE: usize = 16;
    match pixels {
        PixelData::U16(v) => {
            let clip_threshold = 65208u16;
            for &p in v.iter().step_by(STRIDE) {
                let n = p as f64 / 65535.0;
                normalized.push(n);
                bins[(n * 255.0) as usize] += 1;
                sum += n;
                count += 1;
                if p >= clip_threshold { clipped += 1; }
            }
        }
        PixelData::F32(v) => {
            for &p in v.iter().step_by(STRIDE) {
                if !p.is_finite() { continue; }
                let n = p.clamp(0.0, 1.0) as f64;
                normalized.push(n);
                bins[(n * 255.0) as usize] += 1;
                sum += n;
                count += 1;
                if n >= 0.995 { clipped += 1; }
            }
        }
        PixelData::U8(v) => {
            for &p in v.iter().step_by(STRIDE) {
                let n = p as f64 / 255.0;
                normalized.push(n);
                bins[p as usize] += 1;
                sum += n;
                count += 1;
                if p >= 253 { clipped += 1; }
            }
        }
    }

    let median = true_median(&mut normalized);
    let mean = if count > 0 { sum / count as f64 } else { 0.0 };
    let std_dev = normalized.iter().map(|&x| (x - mean).powi(2)).sum::<f64>() / count.max(1) as f64;
    let std_dev = std_dev.sqrt();
    let clipping_pct = if count > 0 { clipped as f64 / count as f64 * 100.0 } else { 0.0 };

    HistogramStats {
        bins, bins_g: None, bins_b: None,
        median, median_g: None, median_b: None,
        mean, std_dev, std_dev_g: None, std_dev_b: None,
        clipping_pct,
    }
}

fn compute_rgb_stats(pixels: &PixelData) -> HistogramStats {
    let mut bins_r = vec![0u32; 256];
    let mut bins_g = vec![0u32; 256];
    let mut bins_b = vec![0u32; 256];
    let mut norm_r: Vec<f64> = Vec::new();
    let mut norm_g: Vec<f64> = Vec::new();
    let mut norm_b: Vec<f64> = Vec::new();
    let mut sum_r = 0.0f64;
    let mut sum_g = 0.0f64;
    let mut sum_b = 0.0f64;
    let mut count = 0usize;
    let mut clipped = 0usize;

    const STRIDE: usize = 16;
    match pixels {
        PixelData::U16(v) => {
            let clip = 65208u16;
            for chunk in v.chunks_exact(3).step_by(STRIDE) {
                let (r, g, b) = (chunk[0], chunk[1], chunk[2]);
                let (nr, ng, nb) = (
                    r as f64 / 65535.0,
                    g as f64 / 65535.0,
                    b as f64 / 65535.0,
                );
                bins_r[(nr * 255.0) as usize] += 1;
                bins_g[(ng * 255.0) as usize] += 1;
                bins_b[(nb * 255.0) as usize] += 1;
                norm_r.push(nr); norm_g.push(ng); norm_b.push(nb);
                sum_r += nr; sum_g += ng; sum_b += nb;
                count += 1;
                if r >= clip || g >= clip || b >= clip { clipped += 1; }
            }
        }
        PixelData::F32(v) => {
            for chunk in v.chunks_exact(3).step_by(STRIDE) {
                let (r, g, b) = (
                    chunk[0].clamp(0.0, 1.0) as f64,
                    chunk[1].clamp(0.0, 1.0) as f64,
                    chunk[2].clamp(0.0, 1.0) as f64,
                );
                bins_r[(r * 255.0) as usize] += 1;
                bins_g[(g * 255.0) as usize] += 1;
                bins_b[(b * 255.0) as usize] += 1;
                norm_r.push(r); norm_g.push(g); norm_b.push(b);
                sum_r += r; sum_g += g; sum_b += b;
                count += 1;
                if r >= 0.995 || g >= 0.995 || b >= 0.995 { clipped += 1; }
            }
        }
        PixelData::U8(v) => {
            for chunk in v.chunks_exact(3).step_by(STRIDE) {
                let (r, g, b) = (chunk[0], chunk[1], chunk[2]);
                let (nr, ng, nb) = (r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0);
                bins_r[r as usize] += 1;
                bins_g[g as usize] += 1;
                bins_b[b as usize] += 1;
                norm_r.push(nr); norm_g.push(ng); norm_b.push(nb);
                sum_r += nr; sum_g += ng; sum_b += nb;
                count += 1;
                if r >= 253 || g >= 253 || b >= 253 { clipped += 1; }
            }
        }
    }

    let median_r = true_median(&mut norm_r);
    let median_g = true_median(&mut norm_g);
    let median_b = true_median(&mut norm_b);
    let mean_r = if count > 0 { sum_r / count as f64 } else { 0.0 };
    let mean_g = if count > 0 { sum_g / count as f64 } else { 0.0 };
    let mean_b = if count > 0 { sum_b / count as f64 } else { 0.0 };
    let std_dev_r = (norm_r.iter().map(|&x| (x - mean_r).powi(2)).sum::<f64>() / count.max(1) as f64).sqrt();
    let std_dev_g = (norm_g.iter().map(|&x| (x - mean_g).powi(2)).sum::<f64>() / count.max(1) as f64).sqrt();
    let std_dev_b = (norm_b.iter().map(|&x| (x - mean_b).powi(2)).sum::<f64>() / count.max(1) as f64).sqrt();
    let clipping_pct = if count > 0 { clipped as f64 / count as f64 * 100.0 } else { 0.0 };

    HistogramStats {
        bins: bins_r, bins_g: Some(bins_g), bins_b: Some(bins_b),
        median: median_r, median_g: Some(median_g), median_b: Some(median_b),
        mean: mean_r,
        std_dev: std_dev_r, std_dev_g: Some(std_dev_g), std_dev_b: Some(std_dev_b),
        clipping_pct,
    }
}

/// Compute the true median using partial sort — O(n) average, exact result.
fn true_median(values: &mut Vec<f64>) -> f64 {
    let n = values.len();
    if n == 0 { return 0.0; }
    let mid = n / 2;
    values.select_nth_unstable_by(mid, |a, b| a.partial_cmp(b).unwrap());
    if n % 2 == 0 {
        // For even-length, also need the element just before mid
        let lower = values[..mid].iter().cloned()
            .fold(f64::NEG_INFINITY, f64::max);
        (lower + values[mid]) / 2.0
    } else {
        values[mid]
    }
}


// ----------------------------------------------------------------------
