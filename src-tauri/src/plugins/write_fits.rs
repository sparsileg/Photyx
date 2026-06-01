// plugins/write_fits.rs — WriteFIT built-in native plugin
// Spec §5.3, §6.3

use std::path::Path;
use tracing::{info, warn};
use fitsio::FitsFile;
use fitsio::images::{ImageDescription, ImageType};
use crate::plugin::{PhotonPlugin, ArgMap, ParamSpec, ParamType, PluginOutput, PluginError};
use crate::context::{AppContext, BitDepth, ImageBuffer, PixelData};

pub struct WriteFIT;

impl PhotonPlugin for WriteFIT {
    fn name(&self)        -> &str { "WriteFIT" }
    fn version(&self)     -> &str { "1.0" }
    fn description(&self) -> &str { "Writes all loaded images as FITS files to a destination directory" }

    fn parameters(&self) -> Vec<ParamSpec> {
        vec![
            ParamSpec {
                name:        "destination".to_string(),
                param_type:  ParamType::String,
                required:    true,
                description: "Output directory path".to_string(),
                default:     None,
            },
            ParamSpec {
                name:        "overwrite".to_string(),
                param_type:  ParamType::Boolean,
                required:    false,
                description: "Overwrite existing files (default: false)".to_string(),
                default:     Some("false".to_string()),
            },
        ]
    }

    fn execute(&self, ctx: &mut AppContext, args: &ArgMap) -> Result<PluginOutput, PluginError> {
        let destination = crate::utils::resolve_path(
            args.get("destination")
                .ok_or_else(|| PluginError::missing_arg("destination"))?,
            ctx.common_parent().as_ref().and_then(|p| p.to_str()),
        );

        let overwrite  = args.get("overwrite").map(|v| v == "true").unwrap_or(false);
        let use_stack  = args.get("stack").map(|v| v.eq_ignore_ascii_case("true")).unwrap_or(false);

        // ── Stack result path: write single file ──────────────────────────────
        if use_stack {
            let buffer = ctx.stack_result.as_ref()
                .ok_or_else(|| PluginError::new("NO_STACK", "No stack result available."))?;

            // If destination looks like a file path, use it directly.
            // Otherwise treat it as a directory and auto-generate the filename.
            let out_path = {
                let ext = Path::new(&destination)
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                if matches!(ext.as_str(), "fit" | "fits" | "fts") {
                    destination.clone()
                } else {
                    format!("{}.fit", destination.trim_end_matches('/'))
                }
            };

            if !overwrite && Path::new(&out_path).exists() {
                return Err(PluginError::new("FILE_EXISTS",
                    &format!("File already exists: '{}'. Use overwrite=true to replace.", out_path)));
            }
            if Path::new(&out_path).exists() {
                let _ = std::fs::remove_file(&out_path);
            }

            // Ensure parent directory exists (use original destination, not out_path,
            // to avoid treating the filename stem as a directory name).
            if let Some(parent) = Path::new(&destination).parent() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    PluginError::new("IO_ERROR", &format!("Cannot create directory: {}", e))
                })?;
            }

            write_fits_new(&out_path, buffer)
                .map_err(|e| PluginError::new("WRITE_ERROR", &e))?;

            info!("Wrote stack FITS: {}", out_path);
            return Ok(PluginOutput::Message(format!("Wrote stack result to '{}'", out_path)));
        }

        // ── Session frames: write all to directory ────────────────────────────

        if ctx.file_list.is_empty() {
            return Ok(PluginOutput::Message("No files loaded.".to_string()));
        }

        std::fs::create_dir_all(&destination).map_err(|e| {
            PluginError::new("IO_ERROR", &format!("Cannot create directory '{}': {}", destination, e))
        })?;

        let mut written = 0usize;
        let mut skipped = 0usize;
        let mut errors  = 0usize;
        let total = ctx.file_list.len();

        for path in ctx.file_list.clone() {
            let buffer = match ctx.image_buffers.get(&path) {
                Some(b) => b,
                None => { errors += 1; continue; }
            };

            let stem = Path::new(&buffer.filename)
                .file_stem().and_then(|s| s.to_str()).unwrap_or("image");
            let out_path = format!("{}/{}.fit", destination.trim_end_matches('/'), stem);

            if !overwrite && Path::new(&out_path).exists() {
                skipped += 1;
                continue;
            }

            if Path::new(&out_path).exists() {
                let _ = std::fs::remove_file(&out_path);
            }

            match write_fits_new(&out_path, buffer) {
                Ok(()) => { info!("Wrote FITS: {}", out_path); written += 1; }
                Err(e) => { warn!("Failed to write '{}': {}", out_path, e); errors += 1; }
            }
        }

        let msg = match (errors, skipped) {
            (0, 0) => format!("Wrote {} FITS file(s)", written),
            (0, s) => format!("Wrote {} FITS file(s), {} skipped", written, s),
            (e, 0) => format!("Wrote {}/{} FITS file(s) ({} errors)", written, total, e),
            (e, s) => format!("Wrote {}/{} FITS file(s), {} skipped, {} errors", written, total, s, e),
        };

        Ok(PluginOutput::Message(msg))
    }
}

/// Convert interleaved RGB pixels [R0,G0,B0,R1,G1,B1,...] to planar [R plane, G plane, B plane].
/// For mono images (channels == 1), returns the data unchanged.
fn deinterleave_u8(data: &[u8], n_pixels: usize, channels: usize) -> Vec<u8> {
    if channels == 1 { return data.to_vec(); }
    let mut out = vec![0u8; data.len()];
    for ch in 0..channels {
        for px in 0..n_pixels {
            out[ch * n_pixels + px] = data[px * channels + ch];
        }
    }
    out
}

fn deinterleave_u16(data: &[u16], n_pixels: usize, channels: usize) -> Vec<u16> {
    if channels == 1 { return data.to_vec(); }
    let mut out = vec![0u16; data.len()];
    for ch in 0..channels {
        for px in 0..n_pixels {
            out[ch * n_pixels + px] = data[px * channels + ch];
        }
    }
    out
}

fn deinterleave_f32(data: &[f32], n_pixels: usize, channels: usize) -> Vec<f32> {
    if channels == 1 { return data.to_vec(); }
    let mut out = vec![0.0f32; data.len()];
    for ch in 0..channels {
        for px in 0..n_pixels {
            out[ch * n_pixels + px] = data[px * channels + ch];
        }
    }
    out
}

/// Create a new FITS file from scratch with pixel data and keywords.
pub(crate) fn write_fits_new(out_path: &str, buffer: &ImageBuffer) -> Result<(), String> {
    let image_type = match buffer.bit_depth {
        BitDepth::U8  => ImageType::UnsignedByte,
        BitDepth::U16 => ImageType::Short,
        BitDepth::F32 => ImageType::Float,
    };

    let shape: Vec<usize> = if buffer.channels == 3 {
        vec![buffer.channels as usize, buffer.height as usize, buffer.width as usize]
    } else {
        vec![buffer.height as usize, buffer.width as usize]
    };

    let image_desc = ImageDescription { data_type: image_type, dimensions: &shape };

    let mut fitsfile = FitsFile::create(out_path)
        .with_custom_primary(&image_desc)
        .open()
        .map_err(|e| format!("Cannot create FITS file: {}", e))?;

    let hdu = fitsfile.hdu(0)
        .map_err(|e| format!("Cannot access primary HDU: {}", e))?;

    let pixels = buffer.pixels.as_ref()
        .ok_or_else(|| "No pixel data".to_string())?;

    let n_pixels = buffer.width as usize * buffer.height as usize;
    let channels = buffer.channels as usize;

    match pixels {
        PixelData::U8(data) => {
            let planar = deinterleave_u8(data, n_pixels, channels);
            hdu.write_image(&mut fitsfile, planar.as_slice())
                .map_err(|e| format!("Cannot write pixel data: {}", e))?;
        }
        PixelData::U16(data) => {
            let planar = deinterleave_u16(data, n_pixels, channels);
            let data_i16: Vec<i16> = planar.iter()
                .map(|&v| (v as i32 - 32768) as i16)
                .collect();
            hdu.write_image(&mut fitsfile, data_i16.as_slice())
                .map_err(|e| format!("Cannot write pixel data: {}", e))?;
        }
        PixelData::F32(data) => {
            let planar = deinterleave_f32(data, n_pixels, channels);
            hdu.write_image(&mut fitsfile, planar.as_slice())
                .map_err(|e| format!("Cannot write pixel data: {}", e))?;
        }
    }

    if let BitDepth::U16 = buffer.bit_depth {
        let _ = hdu.write_key(&mut fitsfile, "BZERO",  (32768i32, "offset data range to that of unsigned short"));
        let _ = hdu.write_key(&mut fitsfile, "BSCALE", (1i32, "default scaling factor"));
    }

    for kw in buffer.keywords.values() {
        match kw.name.as_str() {
            "SIMPLE" | "BITPIX" | "NAXIS" | "NAXIS1" | "NAXIS2" | "NAXIS3"
                | "EXTEND" | "END" | "FILENAME" | "BZERO" | "BSCALE"
                | "EXTNAME" | "ROWORDER" => continue,
            _ => {}
        }
        let result = if let Some(comment) = &kw.comment {
            hdu.write_key(&mut fitsfile, &kw.name, (kw.value.as_str(), comment.as_str()))
        } else {
            hdu.write_key(&mut fitsfile, &kw.name, kw.value.as_str())
        };
        if let Err(e) = result {
            warn!("Could not write keyword {}: {}", kw.name, e);
        }
    }

    Ok(())
}


// ----------------------------------------------------------------------
