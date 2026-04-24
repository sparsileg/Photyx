// plugins/write_tiff.rs — WriteTIFF built-in native plugin
// Spec §5.3, §5.4, §6.3

use std::io::BufWriter;
use std::path::Path;
use tracing::{info, warn};
use tiff::encoder::{TiffEncoder, colortype};
use crate::plugin::{PhotonPlugin, ArgMap, ParamSpec, ParamType, PluginOutput, PluginError};
use crate::context::{AppContext, ColorSpace, ImageBuffer, PixelData};

pub struct WriteTIFF;

impl PhotonPlugin for WriteTIFF {
    fn name(&self)        -> &str { "WriteTIFF" }
    fn version(&self)     -> &str { "1.0" }
    fn description(&self) -> &str { "Writes all loaded images as TIFF files to a destination directory" }

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
            ctx.active_directory.as_deref(),
        );

        let overwrite = args.get("overwrite").map(|v| v == "true").unwrap_or(false);

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
            let out_path = format!("{}/{}.tiff", destination.trim_end_matches('/'), stem);

            if !overwrite && Path::new(&out_path).exists() {
                skipped += 1;
                continue;
            }

            match write_tiff_file(&out_path, buffer) {
                Ok(()) => { info!("Wrote TIFF: {}", out_path); written += 1; }
                Err(e) => { warn!("Failed to write '{}': {}", out_path, e); errors += 1; }
            }
        }

        let msg = match (errors, skipped) {
            (0, 0) => format!("Wrote {} TIFF file(s)", written),
            (0, s) => format!("Wrote {} TIFF file(s), {} skipped", written, s),
            (e, 0) => format!("Wrote {}/{} TIFF file(s) ({} errors)", written, total, e),
            (e, s) => format!("Wrote {}/{} TIFF file(s), {} skipped, {} errors", written, total, s, e),
        };

        Ok(PluginOutput::Message(msg))
    }
}

fn build_image_description(buffer: &ImageBuffer) -> String {
    let mut sorted: Vec<_> = buffer.keywords.values().collect();
    sorted.sort_by(|a, b| a.name.cmp(&b.name));
    sorted.iter().map(|kw| {
        let comment = kw.comment.as_deref().unwrap_or("");
        if comment.is_empty() {
            format!("{:<8}= {}", kw.name, kw.value)
        } else {
            format!("{:<8}= {} / {}", kw.name, kw.value, comment)
        }
    }).collect::<Vec<_>>().join("\n")
}

pub(crate) fn write_tiff_file(out_path: &str, buffer: &ImageBuffer) -> Result<(), String> {
    let file = std::fs::File::create(out_path)
        .map_err(|e| format!("Cannot create file: {}", e))?;
    let writer = BufWriter::new(file);
    let mut encoder = TiffEncoder::new(writer)
        .map_err(|e| format!("Cannot create TIFF encoder: {}", e))?;

    let w = buffer.width;
    let h = buffer.height;
    let is_rgb = buffer.channels == 3 && buffer.color_space == ColorSpace::RGB;
    let description = build_image_description(buffer);

    let pixels = buffer.pixels.as_ref()
        .ok_or_else(|| "No pixel data".to_string())?;

    match pixels {
        PixelData::U8(data) => {
            if is_rgb {
                let mut image = encoder.new_image_with_compression::<colortype::RGB8, _>(
                    w, h, tiff::encoder::compression::Uncompressed)
                    .map_err(|e| format!("Encoder error: {}", e))?;
                image.encoder().write_tag(tiff::tags::Tag::ImageDescription, description.as_str())
                    .map_err(|e| format!("Tag write error: {}", e))?;
                image.write_data(data.as_slice())
                    .map_err(|e| format!("Write error: {}", e))?;
            } else {
                let mut image = encoder.new_image_with_compression::<colortype::Gray8, _>(
                    w, h, tiff::encoder::compression::Uncompressed)
                    .map_err(|e| format!("Encoder error: {}", e))?;
                image.encoder().write_tag(tiff::tags::Tag::ImageDescription, description.as_str())
                    .map_err(|e| format!("Tag write error: {}", e))?;
                image.write_data(data.as_slice())
                    .map_err(|e| format!("Write error: {}", e))?;
            }
        }
        PixelData::U16(data) => {
            if is_rgb {
                let mut image = encoder.new_image_with_compression::<colortype::RGB16, _>(
                    w, h, tiff::encoder::compression::Uncompressed)
                    .map_err(|e| format!("Encoder error: {}", e))?;
                image.encoder().write_tag(tiff::tags::Tag::ImageDescription, description.as_str())
                    .map_err(|e| format!("Tag write error: {}", e))?;
                image.write_data(data.as_slice())
                    .map_err(|e| format!("Write error: {}", e))?;
            } else {
                let mut image = encoder.new_image_with_compression::<colortype::Gray16, _>(
                    w, h, tiff::encoder::compression::Uncompressed)
                    .map_err(|e| format!("Encoder error: {}", e))?;
                image.encoder().write_tag(tiff::tags::Tag::ImageDescription, description.as_str())
                    .map_err(|e| format!("Tag write error: {}", e))?;
                image.write_data(data.as_slice())
                    .map_err(|e| format!("Write error: {}", e))?;
            }
        }
        PixelData::F32(data) => {
            let as_u32: Vec<u32> = data.iter().map(|&f| f.to_bits()).collect();
            if is_rgb {
                let mut image = encoder.new_image_with_compression::<colortype::RGB32, _>(
                    w, h, tiff::encoder::compression::Uncompressed)
                    .map_err(|e| format!("Encoder error: {}", e))?;
                image.encoder().write_tag(tiff::tags::Tag::ImageDescription, description.as_str())
                    .map_err(|e| format!("Tag write error: {}", e))?;
                image.write_data(as_u32.as_slice())
                    .map_err(|e| format!("Write error: {}", e))?;
            } else {
                let mut image = encoder.new_image_with_compression::<colortype::Gray32, _>(
                    w, h, tiff::encoder::compression::Uncompressed)
                    .map_err(|e| format!("Encoder error: {}", e))?;
                image.encoder().write_tag(tiff::tags::Tag::ImageDescription, description.as_str())
                    .map_err(|e| format!("Tag write error: {}", e))?;
                image.write_data(as_u32.as_slice())
                    .map_err(|e| format!("Write error: {}", e))?;
            }
        }
    }

    Ok(())
}


// ----------------------------------------------------------------------
