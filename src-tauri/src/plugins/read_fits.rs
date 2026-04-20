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

fn read_fits_file(path: &str) -> Result<ImageBuffer, String> {
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

    // Read header keywords using fitsio's iter
    let mut keywords = std::collections::HashMap::new();

    // Common astrophotography keywords to attempt reading
    let known_keys = [
        "OBJECT", "TELESCOP", "INSTRUME", "EXPTIME", "GAIN", "OFFSET",
        "TEMP", "FILTER", "BAYERPAT", "XBINNING", "YBINNING", "FOCALLEN",
        "APERTURE", "RA", "DEC", "DATE-OBS", "SITELONG", "SITELAT",
        "SITEELEV", "IMAGETYP", "SWCREATE", "NAXIS", "NAXIS1", "NAXIS2",
        "BITPIX", "BZERO", "BSCALE",
    ];

    for key in &known_keys {
        if let Ok(value) = hdu.read_key::<String>(&mut fitsfile, key) {
            keywords.insert(
                key.to_uppercase(),
                KeywordEntry::new(key, &value, None),
            );
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
                .unwrap_or_default();
            Some(PixelData::U8(data))
        }
        BitDepth::U16 => {
            let data: Vec<u16> = hdu.read_image(&mut fitsfile)
                .unwrap_or_default();
            Some(PixelData::U16(data))
        }
        BitDepth::F32 => {
            let data: Vec<f32> = hdu.read_image(&mut fitsfile)
                .unwrap_or_default();
            Some(PixelData::F32(data))
        }
    };

    Ok(ImageBuffer {
        filename,
        width,
        height,
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
