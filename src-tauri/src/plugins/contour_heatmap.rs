// plugins/contour_heatmap.rs — ContourHeatmap plugin
// Spec §photyx_spec_contour_heatmap.md
//
// Generates a spatial FWHM heatmap across the current frame.
//
// Pipeline:
//   1. Detect stars (reuses analysis::stars::detect_stars)
//   2. Compute per-star FWHM (reuses analysis::fwhm::star_fwhm)
//   3. Validate star count against thresholds
//   4. Assign stars to adaptive grid cells
//   5. Compute mean FWHM per cell
//   6. Bilinear interpolation across full image resolution
//   7. Map interpolated values to RGB via selected palette
//   8. Render contour lines (dashed, auto-contrasted, black-outlined)
//   9. Build ImageBuffer, write as XISF, inject into session

use crate::analysis::{self, fwhm::star_fwhm, stars::detect_stars, StarDetectionConfig};
use crate::context::{AppContext, BitDepth, ColorSpace, ImageBuffer, KeywordEntry, PixelData};
use crate::plugin::{ArgMap, ParamSpec, ParamType, PhotonPlugin, PluginError, PluginOutput};
use serde_json::json;
use std::collections::HashMap;
use tracing::info;

// ── Constants ─────────────────────────────────────────────────────────────────

const STAR_ABORT_THRESHOLD:   usize = 25;
const STAR_WARN_THRESHOLD:    usize = 75;
const CONTOUR_DASH_LEN:       usize = 12;  // pixels on
const CONTOUR_GAP_LEN:        usize = 6;   // pixels off
// CONTOUR_BAND_HALF reserved for future multi-pixel contour width — not yet used
const OUTLINE_RADIUS:         usize = 1;   // black outline thickness around contour

// ── Palette ───────────────────────────────────────────────────────────────────
// t = 0.0 → best focus (small FWHM), t = 1.0 → worst focus (large FWHM)

#[derive(Debug, Clone, Copy, PartialEq)]
enum Palette {
    Viridis,
    Plasma,
    CoolWarm,
}

impl Palette {
    fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "viridis"  => Some(Self::Viridis),
            "plasma"   => Some(Self::Plasma),
            "coolwarm" => Some(Self::CoolWarm),
            _          => None,
        }
    }

    fn sample(&self, t: f32) -> [u8; 3] {
        let t = t.clamp(0.0, 1.0);
        let stops: &[(f32, [u8; 3])] = match self {
            // Viridis: blue-purple → teal → green → yellow (colorblind-safe)
            Self::Viridis => &[
                (0.000, [68,   1,  84]),
                (0.125, [71,  44, 122]),
                (0.250, [59,  81, 139]),
                (0.375, [44, 113, 142]),
                (0.500, [33, 145, 140]),
                (0.625, [39, 173, 129]),
                (0.750, [92, 200,  99]),
                (0.875, [170, 220, 50]),
                (1.000, [253, 231,  37]),
            ],
            // Plasma: blue → purple → orange → yellow (colorblind-safe)
            Self::Plasma => &[
                (0.000, [13,   8, 135]),
                (0.125, [75,   3, 161]),
                (0.250, [125,  3, 168]),
                (0.375, [168, 34, 150]),
                (0.500, [203, 70, 121]),
                (0.625, [229, 107, 93]),
                (0.750, [248, 148,  65]),
                (0.875, [253, 195,  40]),
                (1.000, [240, 249,  33]),
            ],
            // Cool-to-warm: blue → white → red (intuitive for focus quality)
            Self::CoolWarm => &[
                (0.000, [59,  76, 192]),
                (0.250, [124, 159, 234]),
                (0.500, [221, 221, 221]),
                (0.750, [229, 134, 107]),
                (1.000, [180,   4,  38]),
            ],
        };

        for i in 0..stops.len() - 1 {
            let (t0, c0) = stops[i];
            let (t1, c1) = stops[i + 1];
            if t >= t0 && t <= t1 {
                let u = (t - t0) / (t1 - t0);
                return [
                    lerp_u8(c0[0], c1[0], u),
                    lerp_u8(c0[1], c1[1], u),
                    lerp_u8(c0[2], c1[2], u),
                ];
            }
        }
        stops.last().unwrap().1
    }
}

#[inline]
fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t).round() as u8
}

#[inline]
fn lerp_f32(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

// ── Grid sizing ───────────────────────────────────────────────────────────────

fn grid_size(star_count: usize) -> usize {
    match star_count {
        0..=399    => 5,
        400..=899  => 7,
        900..=1599 => 9,
        1600..=2499 => 11,
        2500..=3599 => 13,
        _           => 15,
    }
}

// ── Contour auto-contrast ─────────────────────────────────────────────────────
// Choose green or yellow based on luminance of the underlying heatmap color.
// Black outline is always applied regardless.

fn contour_dash_color(bg: [u8; 3]) -> [u8; 3] {
    // Perceived luminance (BT.709)
    let luma = 0.2126 * bg[0] as f32 + 0.7152 * bg[1] as f32 + 0.0722 * bg[2] as f32;
    if luma < 128.0 {
        [0, 255, 128]   // bright green on dark backgrounds
    } else {
        [255, 220, 0]   // yellow on light backgrounds
    }
}

// ── Plugin ────────────────────────────────────────────────────────────────────

pub struct ContourHeatmap;

impl PhotonPlugin for ContourHeatmap {
    fn name(&self)        -> &str { "ContourHeatmap" }
    fn version(&self)     -> &str { "1.0.0" }
    fn description(&self) -> &str {
        "Generates a spatial FWHM heatmap across the current frame. Stars are \
         detected, per-star FWHM is measured, values are interpolated across an \
         adaptive grid, and the result is rendered as a colour heatmap image with \
         contour lines. Output is written as an XISF file to the source directory."
    }

    fn parameters(&self) -> Vec<ParamSpec> {
        vec![
            ParamSpec {
                name:        "palette".to_string(),
                param_type:  ParamType::String,
                required:    false,
                description: "Colour palette: viridis (default), plasma, coolwarm".to_string(),
                default:     Some("viridis".to_string()),
            },
            ParamSpec {
                name:        "contour_levels".to_string(),
                param_type:  ParamType::Integer,
                required:    false,
                description: "Number of contour lines across the FWHM range (default: 10)".to_string(),
                default:     Some("10".to_string()),
            },
            ParamSpec {
                name:        "threshold".to_string(),
                param_type:  ParamType::Float,
                required:    false,
                description: "Star detection threshold in units of background std dev (default: 5.0)".to_string(),
                default:     Some("5.0".to_string()),
            },
            ParamSpec {
                name:        "saturation".to_string(),
                param_type:  ParamType::Float,
                required:    false,
                description: "Saturation threshold — stars at or above this are rejected (default: 0.98)".to_string(),
                default:     Some("0.98".to_string()),
            },
        ]
    }

    fn execute(&self, ctx: &mut AppContext, args: &ArgMap) -> Result<PluginOutput, PluginError> {

        // ── Parse args ────────────────────────────────────────────────────────

        let palette_name = args.get("palette").map(|s| s.as_str()).unwrap_or("viridis");
        let palette = Palette::from_str(palette_name).ok_or_else(|| {
            PluginError::invalid_arg("palette", "must be one of: viridis, plasma, coolwarm")
        })?;

        let contour_levels = args.get("contour_levels")
            .map(|s| s.parse::<usize>())
            .transpose()
            .map_err(|_| PluginError::invalid_arg("contour_levels", "must be a positive integer"))?
            .unwrap_or(10)
            .max(2);

        let mut det_config = StarDetectionConfig::default();
        if let Some(s) = args.get("threshold") {
            det_config.detection_threshold = s.parse::<f32>().map_err(|_| {
                PluginError::invalid_arg("threshold", "must be a positive float")
            })?;
        }
        if let Some(s) = args.get("saturation") {
            det_config.saturation_threshold = s.parse::<f32>().map_err(|_| {
                PluginError::invalid_arg("saturation", "must be a float between 0.0 and 1.0")
            })?;
        }

        // ── Load current image ────────────────────────────────────────────────

        let img = ctx.current_image().ok_or_else(|| {
            PluginError::new("NO_IMAGE", "No image loaded.")
        })?;

        let pixels = img.pixels.as_ref().ok_or_else(|| {
            PluginError::new("NO_PIXELS", "Image buffer contains no pixel data.")
        })?;

        let width    = img.width  as usize;
        let height   = img.height as usize;
        let channels = img.channels as usize;
        let source_path = img.filename.clone();

        // ── Derive output path ────────────────────────────────────────────────

        let source_stem = std::path::Path::new(&source_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("image");
        let source_dir = std::path::Path::new(&source_path)
            .parent()
            .and_then(|p| p.to_str())
            .unwrap_or(".");
        let out_path = format!("{}/{}_heatmap.xisf", source_dir.trim_end_matches('/').trim_end_matches('\\'), source_stem);
        let heatmap_key = out_path.clone();

        // ── Detect stars ──────────────────────────────────────────────────────

        let normalized = analysis::to_f32_normalized(pixels);
        let luma = analysis::extract_luminance(&normalized, width, height, channels);
        let stars = detect_stars(&luma, width, height, &det_config);

        // ── Compute per-star FWHM ─────────────────────────────────────────────

        struct StarPoint { x: f32, y: f32, fwhm: f32 }

        let star_points: Vec<StarPoint> = stars.iter()
            .filter_map(|s| {
                let fwhm = star_fwhm(s)?;
                if fwhm < 0.5 || fwhm > 50.0 { return None; }
                Some(StarPoint { x: s.cx, y: s.cy, fwhm })
            })
            .collect();

        let n = star_points.len();

        // ── Validate star count ───────────────────────────────────────────────

        if n < STAR_ABORT_THRESHOLD {
            return Err(PluginError::new(
                "TOO_FEW_STARS",
                &format!(
                    "Only {} stars detected (minimum {} required). \
                     Try lowering the threshold parameter.",
                    n, STAR_ABORT_THRESHOLD
                ),
            ));
        }

        let warn = n < STAR_WARN_THRESHOLD;

        // ── Build adaptive grid ───────────────────────────────────────────────

        let grid = grid_size(n);
        let cell_w = width  as f32 / grid as f32;
        let cell_h = height as f32 / grid as f32;

        // Accumulate FWHM values per cell
        let mut cell_sum   = vec![0.0f64; grid * grid];
        let mut cell_count = vec![0usize; grid * grid];

        for sp in &star_points {
            let col = ((sp.x / cell_w) as usize).min(grid - 1);
            let row = ((sp.y / cell_h) as usize).min(grid - 1);
            cell_sum[row * grid + col]   += sp.fwhm as f64;
            cell_count[row * grid + col] += 1;
        }

        // Cell means — cells with no stars get interpolated later
        let mut cell_fwhm: Vec<Option<f32>> = cell_sum.iter().zip(cell_count.iter())
            .map(|(&sum, &count)| {
                if count > 0 { Some((sum / count as f64) as f32) } else { None }
            })
            .collect();

        // Fill empty cells with the global mean as a fallback so interpolation
        // doesn't produce NaN in sparse areas
        let global_mean = {
            let (sum, cnt) = star_points.iter()
                .fold((0.0f64, 0usize), |(s, c), sp| (s + sp.fwhm as f64, c + 1));
            if cnt > 0 { (sum / cnt as f64) as f32 } else { 3.0 }
        };
        for v in cell_fwhm.iter_mut() {
            if v.is_none() { *v = Some(global_mean); }
        }
        let cell_fwhm: Vec<f32> = cell_fwhm.into_iter().map(|v| v.unwrap()).collect();

        // ── FWHM range ────────────────────────────────────────────────────────

        let fwhm_min = cell_fwhm.iter().cloned().fold(f32::MAX, f32::min);
        let fwhm_max = cell_fwhm.iter().cloned().fold(f32::MIN, f32::max);
        let fwhm_range = (fwhm_max - fwhm_min).max(0.01); // guard zero range

        // ── Bilinear interpolation across full image ───────────────────────────
        // Sample at each pixel's normalised grid position.

        let interp_fwhm = |px: usize, py: usize| -> f32 {
            // Convert pixel coords to grid-space coords (cell centres)
            let gx = (px as f32 + 0.5) / cell_w - 0.5;
            let gy = (py as f32 + 0.5) / cell_h - 0.5;

            let col0 = (gx.floor() as isize).clamp(0, grid as isize - 1) as usize;
            let row0 = (gy.floor() as isize).clamp(0, grid as isize - 1) as usize;
            let col1 = (col0 + 1).min(grid - 1);
            let row1 = (row0 + 1).min(grid - 1);

            let tx = (gx - col0 as f32).clamp(0.0, 1.0);
            let ty = (gy - row0 as f32).clamp(0.0, 1.0);

            let f00 = cell_fwhm[row0 * grid + col0];
            let f10 = cell_fwhm[row0 * grid + col1];
            let f01 = cell_fwhm[row1 * grid + col0];
            let f11 = cell_fwhm[row1 * grid + col1];

            lerp_f32(
                lerp_f32(f00, f10, tx),
                lerp_f32(f01, f11, tx),
                ty,
            )
        };

        // ── Render heatmap pixels ─────────────────────────────────────────────

        let pixel_count = width * height;
        let mut rgb = vec![0u8; pixel_count * 3];

        // Build float FWHM surface (needed for contour pass)
        let mut fwhm_surface = vec![0.0f32; pixel_count];
        for py in 0..height {
            for px in 0..width {
                let fwhm = interp_fwhm(px, py);
                fwhm_surface[py * width + px] = fwhm;
                let t = (fwhm - fwhm_min) / fwhm_range;
                let color = palette.sample(t);
                let idx = (py * width + px) * 3;
                rgb[idx]     = color[0];
                rgb[idx + 1] = color[1];
                rgb[idx + 2] = color[2];
            }
        }

        // ── Render contour lines ──────────────────────────────────────────────
        // Contour levels are evenly spaced across [fwhm_min, fwhm_max].
        // Each contour is detected via sign change of (fwhm_surface - level).
        // Dashed lines: auto-contrasted colour (green or yellow) with black outline.

        let contour_values: Vec<f32> = (1..=contour_levels)
            .map(|i| fwhm_min + fwhm_range * i as f32 / (contour_levels + 1) as f32)
            .collect();

        // For each contour level, mark pixels that straddle the level boundary
        // (4-connected neighbour check). Then render dashes + outline.

        // We need a reproducible dash phase per contour line segment.
        // Simple approach: use pixel index modulo dash period.
        let dash_period = CONTOUR_DASH_LEN + CONTOUR_GAP_LEN;

        for &level in &contour_values {
            // First pass: collect all contour pixels for this level
            let mut on_contour = vec![false; pixel_count];

            for py in 1..height - 1 {
                for px in 1..width - 1 {
                    let v = fwhm_surface[py * width + px];
                    // Check 4-connected neighbours for sign change
                    let neighbours = [
                        fwhm_surface[py * width + px + 1],
                        fwhm_surface[py * width + px - 1],
                        fwhm_surface[(py + 1) * width + px],
                        fwhm_surface[(py - 1) * width + px],
                    ];
                    let crosses = neighbours.iter().any(|&n| {
                        (v - level) * (n - level) < 0.0
                    });
                    if crosses {
                        on_contour[py * width + px] = true;
                    }
                }
            }

            // Second pass: black outline (write black to all neighbours of contour pixels)
            for py in OUTLINE_RADIUS..height - OUTLINE_RADIUS {
                for px in OUTLINE_RADIUS..width - OUTLINE_RADIUS {
                    if !on_contour[py * width + px] { continue; }
                    for dy in 0..=(OUTLINE_RADIUS * 2) {
                        for dx in 0..=(OUTLINE_RADIUS * 2) {
                            let ny = py + dy - OUTLINE_RADIUS;
                            let nx = px + dx - OUTLINE_RADIUS;
                            if ny < height && nx < width {
                                let idx = (ny * width + nx) * 3;
                                rgb[idx]     = 0;
                                rgb[idx + 1] = 0;
                                rgb[idx + 2] = 0;
                            }
                        }
                    }
                }
            }

            // Third pass: dashed contour colour on top of outline
            for py in 0..height {
                for px in 0..width {
                    if !on_contour[py * width + px] { continue; }
                    // Dash phase: use Manhattan position for consistent dash pattern
                    let phase = (px + py) % dash_period;
                    if phase >= CONTOUR_DASH_LEN { continue; } // in gap
                    let idx = (py * width + px) * 3;
                    let bg = [rgb[idx], rgb[idx + 1], rgb[idx + 2]];
                    let dash_color = contour_dash_color(bg);
                    rgb[idx]     = dash_color[0];
                    rgb[idx + 1] = dash_color[1];
                    rgb[idx + 2] = dash_color[2];
                }
            }
        }

        // ── Build FITS keywords for metadata ──────────────────────────────────

        let mut keywords: HashMap<String, KeywordEntry> = HashMap::new();

        let mut kw = |name: &str, value: &str, comment: &str| {
            keywords.insert(
                name.to_string(),
                KeywordEntry::new(name, value, Some(comment)),
            );
        };

        kw("PXTYPE",   "HEATMAP",          "Photyx image type");
        kw("PXGRID",   &format!("{}x{}", grid, grid), "Heatmap grid cols x rows");
        kw("PXSTARS",  &n.to_string(),      "Stars used in heatmap");
        kw("PXFWMIN",  &format!("{:.4}", fwhm_min), "Minimum FWHM (pixels)");
        kw("PXFWMAX",  &format!("{:.4}", fwhm_max), "Maximum FWHM (pixels)");
        kw("PXFWRNG",  &format!("{:.4}", fwhm_range), "FWHM range (pixels)");
        kw("PXPAL",    palette_name,        "Heatmap colour palette");
        kw("PXCLEVLS", &contour_levels.to_string(), "Number of contour levels");
        if warn {
            kw("PXWARN", "LOW_STAR_COUNT", "Star count below reliable threshold");
        }

        // ── Build ImageBuffer ─────────────────────────────────────────────────

        let heatmap_buffer = ImageBuffer {
            filename:      heatmap_key.clone(),
            width:         width as u32,
            height:        height as u32,
            display_width: 0,
            bit_depth:     BitDepth::U8,
            color_space:   ColorSpace::RGB,
            channels:      3,
            keywords,
            pixels:        Some(PixelData::U8(rgb)),
        };

        // ── Write XISF to disk ────────────────────────────────────────────────

        {
            use photyx_xisf::{XisfWriter, WriteOptions, Codec};
            use crate::plugins::write_xisf::buffer_to_xisf_image;
            let xisf_image = buffer_to_xisf_image(&heatmap_buffer)
                .map_err(|e| PluginError::new("XISF_CONVERT", &e))?;
            let options = WriteOptions {
                codec:           Codec::None,
                shuffle:         false,
                creator_app:     "Photyx".to_string(),
                block_alignment: 4096,
            };
            XisfWriter::write(&out_path, &xisf_image, &options)
                .map_err(|e| PluginError::new("XISF_WRITE", &format!("Failed to write heatmap: {}", e)))?;
            info!("ContourHeatmap: wrote {}", out_path);
        }
        ctx.variables.insert("NEW_FILE".to_string(), out_path.clone());

        // ── Build response ────────────────────────────────────────────────────

        let warn_text = if warn {
            format!(" WARNING: only {} stars detected — results may be unreliable.", n)
        } else {
            String::new()
        };

        let message = format!(
            "ContourHeatmap: {}×{} grid, {} stars, FWHM {:.2}–{:.2}px, palette={}, written to {}.{}",
            grid, grid, n, fwhm_min, fwhm_max, palette_name, out_path, warn_text
        );

        Ok(PluginOutput::Data(json!({
            "plugin":         "ContourHeatmap",
            "source":         source_path,
            "output":         out_path,
            "grid":           grid,
            "star_count":     n,
            "fwhm_min":       fwhm_min,
            "fwhm_max":       fwhm_max,
            "palette":        palette_name,
            "contour_levels": contour_levels,
            "warned":         warn,
            "message":        message,
        })))
    }
}

// ----------------------------------------------------------------------
