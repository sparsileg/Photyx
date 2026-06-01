// plugins/background_extract.rs — BackgroundExtract plugin
// Fits a 2D polynomial surface to each channel's background independently
// and subtracts it, correcting light pollution gradients and vignetting.
//
// Operates on the current session frame (default) or the transient stack
// result (stack=true). Modifies the pixel buffer in place.

use crate::analysis::background::sigma_clipped_background;
use crate::context::{AppContext, ColorSpace, PixelData};
use crate::plugin::{ArgMap, ParamSpec, ParamType, PhotonPlugin, PluginError, PluginOutput};
use crate::settings::defaults::{
    DEFAULT_BE_DEGREE, DEFAULT_BE_GRID, MAX_BE_DEGREE, MAX_BE_GRID, MIN_BE_DEGREE, MIN_BE_GRID,
};
use crate::analysis::SigmaClipConfig;
use rayon::prelude::*;

pub struct BackgroundExtract;

impl PhotonPlugin for BackgroundExtract {
    fn name(&self)        -> &str { "BackgroundExtract" }
    fn version(&self)     -> &str { "1.0.0" }
    fn description(&self) -> &str {
        "Fits a 2D polynomial surface to each channel's background independently and subtracts it. \
         Corrects light pollution gradients and vignetting residuals. \
         Operates on the current frame or the stack result (stack=true)."
    }

    fn parameters(&self) -> Vec<ParamSpec> {
        vec![
            ParamSpec {
                name:        "grid".to_string(),
                param_type:  ParamType::Integer,
                required:    false,
                description: format!(
                    "Sampling grid size N (N×N cells, {MIN_BE_GRID}–{MAX_BE_GRID}, default {DEFAULT_BE_GRID})"
                ),
                default: Some(DEFAULT_BE_GRID.to_string()),
            },
            ParamSpec {
                name:        "degree".to_string(),
                param_type:  ParamType::Integer,
                required:    false,
                description: format!(
                    "Polynomial degree ({MIN_BE_DEGREE}–{MAX_BE_DEGREE}, default {DEFAULT_BE_DEGREE})"
                ),
                default: Some(DEFAULT_BE_DEGREE.to_string()),
            },
            ParamSpec {
                name:        "stack".to_string(),
                param_type:  ParamType::Boolean,
                required:    false,
                description: "Operate on the transient stack result instead of the current session frame (default: false)".to_string(),
                default:     Some("false".to_string()),
            },
        ]
    }

    fn execute(&self, ctx: &mut AppContext, args: &ArgMap) -> Result<PluginOutput, PluginError> {
        // ── Parse arguments ───────────────────────────────────────────────────

        let use_stack = args.get("stack")
            .map(|v| v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);

        let grid = match args.get("grid") {
            Some(s) => s.parse::<usize>().map_err(|_| {
                PluginError::invalid_arg("grid", "must be a positive integer (e.g. grid=32)")
            })?,
            None => DEFAULT_BE_GRID,
        };

        let degree = match args.get("degree") {
            Some(s) => s.parse::<usize>().map_err(|_| {
                PluginError::invalid_arg("degree", "must be a positive integer (e.g. degree=2)")
            })?,
            None => DEFAULT_BE_DEGREE,
        };

        if grid < MIN_BE_GRID || grid > MAX_BE_GRID {
            return Err(PluginError::invalid_arg(
                "grid",
                &format!("must be between {MIN_BE_GRID} and {MAX_BE_GRID}"),
            ));
        }

        if degree < MIN_BE_DEGREE || degree > MAX_BE_DEGREE {
            return Err(PluginError::invalid_arg(
                "degree",
                &format!("must be between {MIN_BE_DEGREE} and {MAX_BE_DEGREE}"),
            ));
        }

        // ── Validate target buffer exists ─────────────────────────────────────

        if use_stack {
            if ctx.stack_result.is_none() {
                return Err(PluginError::new(
                    "NO_STACK",
                    "No stack result available. Run StackFrames first.",
                ));
            }
        } else if ctx.current_image().is_none() {
            return Err(PluginError::new(
                "NO_IMAGE",
                "No image loaded. Load files before running BackgroundExtract.",
            ));
        }

        // ── Extract geometry and label ────────────────────────────────────────

        let (width, height, channels, color_space, label) = if use_stack {
            let buf = ctx.stack_result.as_ref().unwrap();
            (
                buf.width as usize,
                buf.height as usize,
                buf.channels as usize,
                buf.color_space.clone(),
                "stack result".to_string(),
            )
        } else {
            let buf = ctx.current_image().unwrap();
            (
                buf.width as usize,
                buf.height as usize,
                buf.channels as usize,
                buf.color_space.clone(),
                buf.filename.clone(),
            )
        };

        // ── Validate grid vs image size ───────────────────────────────────────

        let cell_w = width  / grid;
        let cell_h = height / grid;

        if cell_w == 0 || cell_h == 0 {
            return Err(PluginError::invalid_arg(
                "grid",
                &format!("grid={grid} is too large for a {width}×{height} image"),
            ));
        }

        let n_coeffs = (degree + 1) * (degree + 2) / 2;
        let n_cells  = grid * grid;

        if n_cells < n_coeffs {
            return Err(PluginError::new(
                "UNDERDETERMINED",
                &format!(
                    "grid={grid} provides only {n_cells} sample points but degree={degree} \
                     requires at least {n_coeffs}. Increase grid or decrease degree."
                ),
            ));
        }

        // ── Build normalised pixel planes per channel ─────────────────────────
        // For Mono: one plane. For RGB/Bayer: three planes (R, G, B).
        // Pixels are interleaved: index = py * width * channels + px * channels + ch

        let n_pixels = width * height;
        let n_planes = if matches!(color_space, ColorSpace::Mono) { 1 } else { channels.min(3) };

        // Extract normalised f32 planes from the pixel buffer
        let planes: Vec<Vec<f32>> = {
            let get_pixels = |pixels: &PixelData| -> Vec<Vec<f32>> {
                (0..n_planes).map(|ch| {
                    match pixels {
                        PixelData::U8(v) => (0..n_pixels)
                            .map(|px| v[px * channels + ch] as f32 / 255.0)
                            .collect(),
                        PixelData::U16(v) => (0..n_pixels)
                            .map(|px| v[px * channels + ch] as f32 / 65535.0)
                            .collect(),
                        PixelData::F32(v) => (0..n_pixels)
                            .map(|px| v[px * channels + ch])
                            .collect(),
                    }
                }).collect()
            };

            if use_stack {
                let pixels = ctx.stack_result.as_ref().unwrap().pixels.as_ref()
                    .ok_or_else(|| PluginError::new("NO_PIXELS", "Stack buffer contains no pixel data."))?;
                get_pixels(pixels)
            } else {
                let path = ctx.file_list.get(ctx.current_frame).cloned()
                    .ok_or_else(|| PluginError::new("NO_IMAGE", "No current frame."))?;
                let pixels = ctx.image_buffers.get(&path)
                    .and_then(|b| b.pixels.as_ref())
                    .ok_or_else(|| PluginError::new("NO_PIXELS", "Image buffer contains no pixel data."))?;
                get_pixels(pixels)
            }
        };

        // ── Fit and build a correction surface per channel ────────────────────

        let sigma_config = SigmaClipConfig::default();

        // Normalised cell centre coordinates [-1, 1]
        let cx_scale = if width  > 1 { (width  - 1) as f64 } else { 1.0 };
        let cy_scale = if height > 1 { (height - 1) as f64 } else { 1.0 };

        let basis = |x: f64, y: f64| -> Vec<f64> {
            let mut row = Vec::with_capacity(n_coeffs);
            for d in 0..=degree {
                for k in 0..=d {
                    let px = (d - k) as i32;
                    let py = k        as i32;
                    row.push(x.powi(px) * y.powi(py));
                }
            }
            row
        };

        // One surface per channel, computed independently
        let surfaces: Vec<Vec<f32>> = planes.iter().map(|plane| {
            // Sample grid cells
            let mut sample_x:   Vec<f64> = Vec::with_capacity(n_cells);
            let mut sample_y:   Vec<f64> = Vec::with_capacity(n_cells);
            let mut sample_val: Vec<f64> = Vec::with_capacity(n_cells);

            for row in 0..grid {
                for col in 0..grid {
                    let x0 = col * cell_w;
                    let y0 = row * cell_h;
                    let x1 = (x0 + cell_w).min(width);
                    let y1 = (y0 + cell_h).min(height);

                    // Subsample within cell (every 4th pixel) before sigma-clipping
                    let subsampled: Vec<f32> = (y0..y1).flat_map(|y| {
                        (x0..x1).map(move |x| (y * width + x, x, y))
                    })
                    .enumerate()
                    .filter(|(i, _)| i % 4 == 0)
                    .map(|(_, (idx, _, _))| plane[idx])
                    .collect();

                    if subsampled.is_empty() {
                        continue;
                    }

                    let est = sigma_clipped_background(&subsampled, &sigma_config);

                    let cx = ((x0 + x1) as f64 * 0.5) / cx_scale * 2.0 - 1.0;
                    let cy = ((y0 + y1) as f64 * 0.5) / cy_scale * 2.0 - 1.0;

                    sample_x.push(cx);
                    sample_y.push(cy);
                    sample_val.push(est.median as f64);
                }
            }

            let n_samples = sample_x.len();

            // Fall back to zero surface if underdetermined (shouldn't happen after
            // the pre-flight check above, but be defensive per channel)
            if n_samples < n_coeffs {
                return vec![0.0f32; n_pixels];
            }

            // Build normal equations
            let mut ata = vec![0.0f64; n_coeffs * n_coeffs];
            let mut atb = vec![0.0f64; n_coeffs];

            for i in 0..n_samples {
                let row = basis(sample_x[i], sample_y[i]);
                let b   = sample_val[i];
                for r in 0..n_coeffs {
                    atb[r] += row[r] * b;
                    for c in 0..n_coeffs {
                        ata[r * n_coeffs + c] += row[r] * row[c];
                    }
                }
            }

            let coeffs = match cholesky_solve(&ata, &atb, n_coeffs) {
                Some(c) => c,
                None    => return vec![0.0f32; n_pixels],
            };

            // Evaluate surface at every pixel in parallel
            (0..n_pixels).into_par_iter().map(|px| {
                let xi = px % width;
                let yi = px / width;
                let xn = xi as f64 / cx_scale * 2.0 - 1.0;
                let yn = yi as f64 / cy_scale * 2.0 - 1.0;
                let row = basis(xn, yn);
                let val: f64 = row.iter().zip(coeffs.iter()).map(|(b, c)| b * c).sum();
                val.max(0.0) as f32
            }).collect()
        }).collect();

        // ── Apply per-channel correction to the pixel buffer ──────────────────

        let apply_correction = |pixels: &mut PixelData| {
            match pixels {
                PixelData::U8(ref mut v) => {
                    for px in 0..n_pixels {
                        for ch in 0..n_planes {
                            let correction = (surfaces[ch][px] * 255.0).round() as i32;
                            let idx = px * channels + ch;
                            v[idx] = (v[idx] as i32 - correction).max(0) as u8;
                        }
                    }
                }
                PixelData::U16(ref mut v) => {
                    for px in 0..n_pixels {
                        for ch in 0..n_planes {
                            let correction = (surfaces[ch][px] * 65535.0).round() as i32;
                            let idx = px * channels + ch;
                            v[idx] = (v[idx] as i32 - correction).max(0) as u16;
                        }
                    }
                }
                PixelData::F32(ref mut v) => {
                    for px in 0..n_pixels {
                        for ch in 0..n_planes {
                            let idx = px * channels + ch;
                            v[idx] = (v[idx] - surfaces[ch][px]).max(0.0);
                        }
                    }
                }
            }
        };

        if use_stack {
            let buf = ctx.stack_result.as_mut().unwrap();
            if let Some(ref mut pixels) = buf.pixels {
                apply_correction(pixels);
            }
        } else {
            let path = ctx.file_list.get(ctx.current_frame).cloned()
                .ok_or_else(|| PluginError::new("NO_IMAGE", "No current frame."))?;
            ctx.display_cache.remove(&path);
            ctx.full_res_cache.remove(&path);
            ctx.blink_cache_12.remove(&path);
            ctx.blink_cache_25.remove(&path);
            if let Some(buf) = ctx.image_buffers.get_mut(&path) {
                if let Some(ref mut pixels) = buf.pixels {
                    apply_correction(pixels);
                }
            }
        }

        Ok(PluginOutput::Message(format!(
            "BackgroundExtract: {label} corrected ({n_planes} channel(s), grid={grid}, degree={degree})"
        )))
    }
}

// ── Cholesky decomposition solver ─────────────────────────────────────────────
// Solves the symmetric positive definite system Ax = b.
// A is provided as a flat row-major vec of size n×n.
// Returns None if the matrix is singular or not positive definite.

fn cholesky_solve(a: &[f64], b: &[f64], n: usize) -> Option<Vec<f64>> {
    let mut l = vec![0.0f64; n * n];

    for i in 0..n {
        for j in 0..=i {
            let sum: f64 = (0..j).map(|k| l[i * n + k] * l[j * n + k]).sum();
            if i == j {
                let diag = a[i * n + i] - sum;
                if diag <= 0.0 {
                    return None;
                }
                l[i * n + j] = diag.sqrt();
            } else {
                l[i * n + j] = (a[i * n + j] - sum) / l[j * n + j];
            }
        }
    }

    // Forward substitution: L y = b
    let mut y = vec![0.0f64; n];
    for i in 0..n {
        let sum: f64 = (0..i).map(|j| l[i * n + j] * y[j]).sum();
        y[i] = (b[i] - sum) / l[i * n + i];
    }

    // Back substitution: Lᵀ x = y
    let mut x = vec![0.0f64; n];
    for i in (0..n).rev() {
        let sum: f64 = (i + 1..n).map(|j| l[j * n + i] * x[j]).sum();
        x[i] = (y[i] - sum) / l[i * n + i];
    }

    Some(x)
}

// ----------------------------------------------------------------------
