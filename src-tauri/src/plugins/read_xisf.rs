// plugins/read_xisf.rs — ReadXISF built-in native plugin
// Spec §5.2, §6.3

use std::path::Path;
use tracing::{info, warn};
use photyx_xisf::{XisfReader, PixelData as XisfPixelData, SampleFormat, ColorSpace as XisfColorSpace};
use crate::plugin::{PhotonPlugin, ArgMap, ParamSpec, PluginOutput, PluginError};
use crate::context::{AppContext, ImageBuffer, BitDepth, ColorSpace, KeywordEntry, PixelData};

pub struct ReadXISF;

impl PhotonPlugin for ReadXISF {
    fn name(&self)        -> &str { "ReadXISF" }
    fn version(&self)     -> &str { "1.0" }
    fn description(&self) -> &str { "Reads all XISF files in the active directory into the image buffer pool" }
    fn parameters(&self)  -> Vec<ParamSpec> { vec![] }

    fn execute(&self, ctx: &mut AppContext, _args: &ArgMap) -> Result<PluginOutput, PluginError> {
        let dir = ctx.active_directory.clone().ok_or_else(|| {
            PluginError::new("NO_DIRECTORY", "No active directory. Use SelectDirectory first.")
        })?;

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
                ext == "xisf"
            })
            .filter_map(|e| e.path().to_str().map(|s| s.replace('\\', "/")))
            .collect();

        files.sort();

        if files.is_empty() {
            return Ok(PluginOutput::Message(format!("No XISF files found in '{}'", dir)));
        }

        let total = files.len();

        ctx.file_list.clear();
        ctx.image_buffers.clear();
        ctx.display_cache.clear();
        ctx.full_res_cache.clear();

        let mut loaded = 0;
        let mut errors = 0;

        for path in &files {
            match read_xisf_file(path) {
                Ok(buffer) => {
                    info!("Loaded XISF: {} ({}x{} {:?})", path, buffer.width, buffer.height, buffer.bit_depth);
                    ctx.file_list.push(path.clone());
                    ctx.image_buffers.insert(path.clone(), buffer);
                    loaded += 1;
                }
                Err(e) => {
                    warn!("Failed to load XISF '{}': {}", path, e);
                    errors += 1;
                }
            }
        }

        ctx.current_frame = 0;

        let msg = if errors > 0 {
            format!("Loaded {}/{} XISF files ({} errors)", loaded, total, errors)
        } else {
            format!("Loaded {} XISF file(s)", loaded)
        };

        Ok(PluginOutput::Message(msg))
    }
}

pub fn read_xisf_file(path: &str) -> Result<ImageBuffer, String> {
    let reader = XisfReader::open(path)
        .map_err(|e| format!("Cannot open: {}", e))?;

    if reader.image_count() == 0 {
        return Err("No images in XISF file".to_string());
    }

    let meta = reader.image_meta(0)
        .map_err(|e| format!("Cannot read metadata: {}", e))?;

    let width    = meta.width;
    let height   = meta.height;
    let channels = meta.channels as u8;

    let bit_depth = match meta.sample_format {
        SampleFormat::UInt8   => BitDepth::U8,
        SampleFormat::UInt16  => BitDepth::U16,
        SampleFormat::UInt32  => BitDepth::U16,
        SampleFormat::Float32 => BitDepth::F32,
        SampleFormat::Float64 => BitDepth::F32,
    };

    let mut color_space = match meta.color_space {
        XisfColorSpace::Gray       => ColorSpace::Mono,
        XisfColorSpace::RGB        => ColorSpace::RGB,
        XisfColorSpace::CFA        => ColorSpace::Bayer,
        XisfColorSpace::Unknown(_) => ColorSpace::Mono,
    };

    let mut keywords = std::collections::HashMap::new();
    for kw in &meta.fits_keywords {
        let name = kw.name.to_uppercase();
        if name == "COMMENT" || name == "HISTORY" { continue; }
        keywords.insert(
            name.clone(),
            KeywordEntry::new(&name, &kw.value, Some(&kw.comment)),
        );
    }

    if color_space == ColorSpace::Mono && keywords.contains_key("BAYERPAT") {
        color_space = ColorSpace::Bayer;
    }

    let filename = Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path)
        .to_string();

    let image = reader.read_image(0)
        .map_err(|e| format!("Cannot read pixels: {}", e))?;

    let pixels = match image.pixels {
        XisfPixelData::U8(v)  => Some(PixelData::U8(v)),
        XisfPixelData::U16(v) => Some(PixelData::U16(v)),
        XisfPixelData::U32(v) => Some(PixelData::U16(v.iter().map(|&p| (p >> 16) as u16).collect())),
        XisfPixelData::F32(v) => Some(PixelData::F32(v)),
        XisfPixelData::F64(v) => Some(PixelData::F32(v.iter().map(|&p| p as f32).collect())),
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
