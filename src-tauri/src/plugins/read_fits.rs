// plugins/read_fits.rs — ReadFITS built-in native plugin
// Spec §5.2, §6.3

use std::path::Path;
use tracing::{info, warn};
use fitsio::FitsFile;
use fitsio::hdu::HduInfo;
use fitsio::images::ImageType;
use crate::plugin::{PhotonPlugin, ArgMap, ParamSpec, PluginOutput, PluginError};
use crate::context::{AppContext, ImageBuffer, BitDepth, ColorSpace, KeywordEntry, PixelData};

pub struct ReadFITS;

impl PhotonPlugin for ReadFITS {
    fn name(&self) -> &str { "ReadFITS" }
    fn version(&self) -> &str { "1.0" }
    fn description(&self) -> &str { "Reads all FITS files in the active directory into the image buffer pool" }

    fn parameters(&self) -> Vec<ParamSpec> {
        vec![]
    }

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
            return Ok(PluginOutput::Message(
                format!("No FITS files found in '{}'", dir)
            ));
        }

        let total = files.len();

        ctx.file_list.clear();
        ctx.image_buffers.clear();
        ctx.display_cache.clear();
        ctx.full_res_cache.clear();

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
            format!("Loaded {}/{} FITS files ({} errors)", loaded, total, errors)
        } else {
            format!("Loaded {} FITS file(s)", loaded)
        };

        Ok(PluginOutput::Message(msg))
    }
}

pub fn read_fits_file(path: &str) -> Result<ImageBuffer, String> {
    let mut fitsfile = FitsFile::open(path)
        .map_err(|e| format!("Cannot open: {}", e))?;

    let hdu = fitsfile.primary_hdu()
        .map_err(|e| format!("Cannot read primary HDU: {}", e))?;

    // Extract shape and data type from HduInfo
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

    // Read ALL header keywords dynamically using raw cfitsio calls
    let mut keywords = std::collections::HashMap::new();

    unsafe {
        let fptr = fitsfile.as_raw();
        let mut status: std::os::raw::c_int = 0;
        let mut nkeys: std::os::raw::c_int = 0;
        let mut morekeys: std::os::raw::c_int = 0;

        // Get total number of keywords in header
        fitsio_sys::ffghsp(fptr, &mut nkeys, &mut morekeys, &mut status);

        for i in 1..=nkeys {
            let mut record = [0i8; 81];
            status = 0;
            fitsio_sys::ffgrec(fptr, i, record.as_mut_ptr(), &mut status);
            if status != 0 { continue; }

            // Convert to string — each record is 80 chars
            let record_str: String = record[..80].iter()
                .map(|&c| if c == 0 { ' ' } else { c as u8 as char })
                .collect();

            // Skip blank, COMMENT, HISTORY, END records
            let key = record_str[..8].trim().to_uppercase();
            if key.is_empty() || key == "COMMENT" || key == "HISTORY" || key == "END" {
                continue;
            }

            // Parse value and comment — format is: KEY     = VALUE / comment
            if record_str.len() > 10 && &record_str[8..9] == "=" {
                let rest = &record_str[9..].trim_start().to_string();
                let (value, comment) = if let Some(slash) = rest.find(" /") {
                    (rest[..slash].trim().to_string(), Some(rest[slash+2..].trim().to_string()))
                } else {
                    (rest.trim().to_string(), None)
                };

                // Strip surrounding quotes from string values
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

    // Check for Bayer pattern keyword to override color space
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

    // Read pixel data
    let pixels = match &bit_depth {
        BitDepth::U8 => {
            let data: Vec<u8> = hdu.read_image(&mut fitsfile)
                .map_err(|e| format!("Cannot read pixel data: {}", e))?;
            if data.is_empty() { return Err("Pixel data is empty".to_string()); }
            Some(PixelData::U8(data))
        }
        BitDepth::U16 => {
            // Read as i32 to handle both signed and unsigned 16-bit FITS conventions
            // (BITPIX=16 with or without BZERO=32768)
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
        display_width: 0,  // set by AutoStretch after display cache is built
        bit_depth,
        color_space,
        channels,
        keywords,
        pixels,
    })
}

// Command alias — ReadAllFITFiles is the pcode command name per spec §7.8
pub struct ReadAllFITFiles;

impl PhotonPlugin for ReadAllFITFiles {
    fn name(&self) -> &str { "ReadAllFITFiles" }
    fn version(&self) -> &str { "1.0" }
    fn description(&self) -> &str { "Reads all FITS files in the active directory into the image buffer pool" }
    fn parameters(&self) -> Vec<ParamSpec> { vec![] }
    fn execute(&self, ctx: &mut AppContext, args: &ArgMap) -> Result<PluginOutput, PluginError> {
        ReadFITS.execute(ctx, args)
    }
}
