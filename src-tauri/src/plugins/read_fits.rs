// plugins/read_fits.rs — ReadFIT built-in native plugin
// Spec §5.2, §6.3

use std::path::Path;
use tracing::{info, warn};
use fitsio::FitsFile;
use fitsio::hdu::HduInfo;
use fitsio::images::ImageType;
use crate::plugin::{PhotonPlugin, ArgMap, ParamSpec, PluginOutput, PluginError};
use crate::context::{AppContext, ImageBuffer, BitDepth, ColorSpace, KeywordEntry, PixelData};

pub struct ReadFIT;

impl PhotonPlugin for ReadFIT {
    fn name(&self)        -> &str { "ReadFIT" }
    fn version(&self)     -> &str { "1.0" }
    fn description(&self) -> &str { "Reads all FITS files in the active directory into the image buffer pool" }
    fn parameters(&self)  -> Vec<ParamSpec> { vec![] }

    fn execute(&self, ctx: &mut AppContext, _args: &ArgMap) -> Result<PluginOutput, PluginError> {
        let dir = ctx.active_directory.clone().ok_or_else(|| {
            PluginError::new("NO_DIRECTORY", "No active directory. Use SelectDirectory first.")
        })?;

        let fits_extensions = ["fit", "fits", "fts"];

        let entries = std::fs::read_dir(&dir).map_err(|e| {
            PluginError::new("IO_ERROR", &format!("Cannot read directory '{}': {}", dir, e))
        })?;

        let mut files: Vec<String> = entries
            .filter_map(|e| e.ok())
            .filter(|e| {
                let path = e.path();
                let ext = path.extension()
                    .and_then(|x| x.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                fits_extensions.contains(&ext.as_str())
            })
            .filter_map(|e| e.path().to_str().map(|s| s.replace('\\', "/")))
            .collect();

        files.sort();

        if files.is_empty() {
            return Ok(PluginOutput::Message(format!("No FITS files found in '{}'", dir)));
        }

        let total = files.len();

        // ── Memory estimate and limit check ───────────────────────────────────
        let estimated_bytes = if let Some(first) = files.first() {
            peek_fits_dimensions(first).map(|(w, h, c, bpp)| {
                (w as i64) * (h as i64) * (c as i64) * (bpp as i64)
                    * (files.len() as i64)
            })
        } else {
            None
        };

        if let Some(raw_bytes) = estimated_bytes {
            let peak_bytes = (raw_bytes as f64 * 2.1) as i64;
            if peak_bytes > ctx.buffer_pool_bytes {
                return Err(PluginError::new(
                    "MEMORY_LIMIT_EXCEEDED",
                    &format!(
                        "Load cancelled: {} files require ~{:.1} GB of memory. Preferences limit is set to {:.1} GB.",
                        files.len(),
                        peak_bytes as f64 / (1024.0 * 1024.0 * 1024.0),
                        ctx.buffer_pool_bytes as f64 / (1024.0 * 1024.0 * 1024.0),
                    ),
                ));
            }
        }

        let raw_mb = estimated_bytes.unwrap_or(0) / (1024 * 1024);
        let peak_mb = (estimated_bytes.unwrap_or(0) as f64 * 2.1) as i64 / (1024 * 1024);

        ctx.clear_session();

        let mut loaded = 0;
        let mut errors = 0;

        for path in &files {
            match read_fits_file(path) {
                Ok(buffer) => {
                    info!("Loaded FITS: {} ({}x{} {:?})", path, buffer.width, buffer.height, buffer.bit_depth);
                    ctx.file_list.push(path.clone());
                    ctx.image_buffers.insert(path.clone(), buffer);
                    loaded += 1;
                }
                Err(e) => {
                    warn!("Failed to load FITS '{}': {}", path, e);
                    errors += 1;
                }
            }
        }

        ctx.current_frame = 0;

        let msg = if errors > 0 {
            format!(
                "Loaded {}/{} FIT files (~{} MB raw, ~{} MB peak with analysis) ({} errors)",
                loaded, total, raw_mb, peak_mb, errors
            )
        } else {
            format!(
                "Loaded {} FIT file(s) (~{} MB raw, ~{} MB peak with analysis)",
                loaded, raw_mb, peak_mb
            )
        };

        Ok(PluginOutput::Message(msg))
    }
}

/// Peek at a FITS file header to get dimensions, channels, and bit depth
/// without reading pixel data. Returns (width, height, channels, bytes_per_pixel).
pub fn peek_fits_dimensions(path: &str) -> Option<(u32, u32, u8, usize)> {
    let mut fitsfile = FitsFile::open(path).ok()?;
    let hdu = fitsfile.primary_hdu().ok()?;
    match &hdu.info {
        HduInfo::ImageInfo { shape, image_type } => {
            let (w, h, c) = match shape.as_slice() {
                [h, w]    => (*w as u32, *h as u32, 1u8),
                [_, h, w] => (*w as u32, *h as u32, 3u8),
                _         => return None,
            };
            let bpp = match image_type {
                ImageType::UnsignedByte => 1,
                ImageType::Float        => 4,
                ImageType::Double       => 4, // downcast to f32
                _                       => 2, // Short, UnsignedShort, Long → U16
            };
            Some((w, h, c, bpp))
        }
        _ => None,
    }
}

pub fn read_fits_file(path: &str) -> Result<ImageBuffer, String> {
    let mut fitsfile = FitsFile::open(path)
        .map_err(|e| format!("Cannot open: {}", e))?;

    let hdu = fitsfile.primary_hdu()
        .map_err(|e| format!("Cannot read primary HDU: {}", e))?;

    let (width, height, channels, bit_depth) = match &hdu.info {
        HduInfo::ImageInfo { shape, image_type } => {
            let (w, h, c) = match shape.as_slice() {
                [h, w]    => (*w as u32, *h as u32, 1u8),
                [_, h, w] => (*w as u32, *h as u32, 3u8),
                s         => return Err(format!("Unsupported image shape: {:?}", s)),
            };
            let bd = match image_type {
                ImageType::UnsignedByte  => BitDepth::U8,
                ImageType::Short         => BitDepth::U16,
                ImageType::UnsignedShort => BitDepth::U16,
                ImageType::Long          => BitDepth::U16,
                ImageType::Float         => BitDepth::F32,
                ImageType::Double        => BitDepth::F32,
                _                        => BitDepth::U16,
            };
            (w, h, c, bd)
        }
        _ => return Err("Primary HDU is not an image".to_string()),
    };

    let color_space = if channels == 3 { ColorSpace::RGB } else { ColorSpace::Mono };

    let mut keywords = std::collections::HashMap::new();

    unsafe {
        let fptr = fitsfile.as_raw();
        let mut status: std::os::raw::c_int = 0;
        let mut nkeys: std::os::raw::c_int = 0;
        let mut morekeys: std::os::raw::c_int = 0;

        fitsio_sys::ffghsp(fptr, &mut nkeys, &mut morekeys, &mut status);

        for i in 1..=nkeys {
            let mut record = [0i8; 81];
            status = 0;
            fitsio_sys::ffgrec(fptr, i, record.as_mut_ptr(), &mut status);
            if status != 0 { continue; }

            let record_str: String = record[..80].iter()
                .map(|&c| if c == 0 { ' ' } else { c as u8 as char })
                .collect();

            let key = record_str[..8].trim().to_uppercase();
            if key.is_empty() || key == "COMMENT" || key == "HISTORY" || key == "END" {
                continue;
            }

            if record_str.len() > 10 && &record_str[8..9] == "=" {
                let rest = &record_str[9..].trim_start().to_string();
                let (value, comment) = if let Some(slash) = rest.find(" /") {
                    (rest[..slash].trim().to_string(), Some(rest[slash+2..].trim().to_string()))
                } else {
                    (rest.trim().to_string(), None)
                };

                let value = if value.starts_with('\'') && value.ends_with('\'') {
                    value[1..value.len()-1].trim_end().to_string()
                } else {
                    value
                };

                keywords.insert(
                    key.clone(),
                    KeywordEntry::new(&key, &value, comment.as_deref()),
                );
            }
        }
    }

    let color_space = if color_space == ColorSpace::Mono && keywords.contains_key("BAYERPAT") {
        ColorSpace::Bayer
    } else {
        color_space
    };

    let filename = Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path)
        .to_string();

    let pixels = match &bit_depth {
        BitDepth::U8 => {
            let data: Vec<u8> = hdu.read_image(&mut fitsfile)
                .map_err(|e| format!("Cannot read pixel data: {}", e))?;
            if data.is_empty() { return Err("Pixel data is empty".to_string()); }
            Some(PixelData::U8(data))
        }
        BitDepth::U16 => {
            let data: Vec<i32> = hdu.read_image(&mut fitsfile)
                .map_err(|e| format!("Cannot read pixel data: {}", e))?;
            if data.is_empty() { return Err("Pixel data is empty".to_string()); }
            let data_u16: Vec<u16> = data.iter().map(|&v| v.clamp(0, 65535) as u16).collect();
            Some(PixelData::U16(data_u16))
        }
        BitDepth::F32 => {
            let data: Vec<f32> = hdu.read_image(&mut fitsfile)
                .map_err(|e| format!("Cannot read pixel data: {}", e))?;
            if data.is_empty() { return Err("Pixel data is empty".to_string()); }
            Some(PixelData::F32(data))
        }
    };

    Ok(ImageBuffer {
        filename,
        width,
        height,
        display_width: 0,
        bit_depth,
        color_space,
        channels,
        keywords,
        pixels,
    })
}


// ----------------------------------------------------------------------
