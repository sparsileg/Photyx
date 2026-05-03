// plugins/read_tiff.rs — ReadTIFF built-in native plugin
// Spec §5.2, §5.4, §5.5, §6.3

use crate::context::{AppContext, BitDepth, ColorSpace, ImageBuffer, KeywordEntry, PixelData};
use crate::plugin::{ArgMap, ParamSpec, PhotonPlugin, PluginError, PluginOutput};
use std::collections::HashMap;
use std::path::Path;
use tiff::decoder::{Decoder, DecodingResult};
use tiff::tags::Tag;
use tiff::ColorType;
use tracing::{info, warn};

pub struct ReadTIFF;

impl PhotonPlugin for ReadTIFF {
    fn name(&self)        -> &str { "ReadTIFF" }
    fn version(&self)     -> &str { "1.0" }
    fn description(&self) -> &str { "Reads all TIFF image files (.tif, .tiff) in the active directory" }
    fn parameters(&self)  -> Vec<ParamSpec> { vec![] }

    fn execute(&self, ctx: &mut AppContext, _args: &ArgMap) -> Result<PluginOutput, PluginError> {
        let dir = ctx.active_directory.clone().ok_or_else(|| {
            PluginError::new("NO_DIRECTORY", "No active directory. Use SelectDirectory first.")
        })?;

        let tiff_extensions = ["tif", "tiff"];

        let entries = std::fs::read_dir(&dir).map_err(|e| {
            PluginError::new("IO_ERROR", &format!("Cannot read directory '{}': {}", dir, e))
        })?;

        let mut tiff_files: Vec<String> = Vec::new();

        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            let ext = path.extension()
                .and_then(|x| x.to_str())
                .unwrap_or("")
                .to_lowercase();
            let path_str = match path.to_str() {
                Some(s) => s.replace('\\', "/"),
                None => continue,
            };
            if tiff_extensions.contains(&ext.as_str()) {
                tiff_files.push(path_str);
            }
        }

        tiff_files.sort();

        if tiff_files.is_empty() {
            return Ok(PluginOutput::Message(format!("No TIFF files found in '{}'", dir)));
        }

        info!("ReadTIFF: loading {} files from {}", tiff_files.len(), dir);

        ctx.clear_session();

        let mut loaded = 0;
        let mut errors = 0;

        for path in &tiff_files {
            match read_tiff_file(path) {
                Ok(buffer) => {
                    info!("Loaded TIFF: {} ({}x{} {:?})", path, buffer.width, buffer.height, buffer.bit_depth);
                    ctx.file_list.push(path.clone());
                    ctx.image_buffers.insert(path.clone(), buffer);
                    loaded += 1;
                }
                Err(e) => {
                    warn!("Failed to load TIFF '{}': {}", path, e);
                    errors += 1;
                }
            }
        }

        ctx.current_frame = 0;

        let msg = if errors > 0 {
            format!("Loaded {}/{} TIFF file(s) ({} errors)", loaded, tiff_files.len(), errors)
        } else {
            format!("Loaded {} TIFF file(s)", loaded)
        };

        Ok(PluginOutput::Message(msg))
    }
}

pub fn read_tiff_file(path: &str) -> Result<ImageBuffer, String> {
    let file = std::fs::File::open(path)
        .map_err(|e| format!("Cannot open file: {e}"))?;

    let mut decoder = Decoder::new(file)
        .map_err(|e| format!("TIFF decode error: {e}"))?;

    let (width, height) = decoder.dimensions()
        .map_err(|e| format!("Cannot read dimensions: {e}"))?;

    let color_type = decoder.colortype()
        .map_err(|e| format!("Cannot read color type: {e}"))?;

    let (channels, color_space): (u8, ColorSpace) = match color_type {
        ColorType::Gray(_) => (1, ColorSpace::Mono),
        ColorType::RGB(_)  => (3, ColorSpace::RGB),
        other => return Err(format!("Unsupported color type: {other:?}")),
    };

    let result = decoder.read_image()
        .map_err(|e| format!("Failed to read image data: {e}"))?;

    let (pixels, bit_depth): (PixelData, BitDepth) = match result {
        DecodingResult::U8(data)  => (PixelData::U8(data), BitDepth::U8),
        DecodingResult::U16(data) => (PixelData::U16(data), BitDepth::U16),
        DecodingResult::U32(data) => {
            let converted: Vec<u16> = data.iter().map(|&v| (v >> 16) as u16).collect();
            (PixelData::U16(converted), BitDepth::U16)
        }
        DecodingResult::F32(data) => (PixelData::F32(data), BitDepth::F32),
        DecodingResult::F64(data) => {
            let converted: Vec<f32> = data.iter().map(|&v| v as f32).collect();
            (PixelData::F32(converted), BitDepth::F32)
        }
        other => return Err(format!("Unsupported pixel format: {other:?}")),
    };

    let filename = Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    let mut keywords: HashMap<String, KeywordEntry> = HashMap::new();
    keywords.insert(
        "FILENAME".to_string(),
        KeywordEntry::new("FILENAME", &filename, Some("Source filename")),
    );

    if let Ok(desc) = decoder.get_tag_ascii_string(Tag::ImageDescription) {
        for line in desc.lines() {
            let line = line.trim();
            if line.is_empty() { continue; }
            if line.len() < 10 || &line[8..9] != "=" { continue; }
            let name = line[..8].trim().to_uppercase();
            if name.is_empty() { continue; }
            let rest = line[9..].trim();
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
            keywords.insert(name.clone(), KeywordEntry::new(&name, &value, comment.as_deref()));
        }
    }

    Ok(ImageBuffer {
        filename,
        width,
        height,
        display_width: 0,
        bit_depth,
        color_space,
        channels,
        keywords,
        pixels: Some(pixels),
    })
}


// ----------------------------------------------------------------------
