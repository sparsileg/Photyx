// plugins/write_xisf.rs — WriteXISF built-in native plugin
// Spec §5.3, §6.3

use std::path::Path;
use tracing::info;
use photyx_xisf::{XisfWriter, WriteOptions, Codec};
use photyx_xisf::{XisfImage, PixelData as XisfPixelData, SampleFormat, ColorSpace as XisfColorSpace};
use crate::plugin::{PhotonPlugin, ArgMap, ParamSpec, ParamType, PluginOutput, PluginError};
use crate::context::{AppContext, BitDepth, ColorSpace, PixelData};

pub struct WriteXISF;

impl PhotonPlugin for WriteXISF {
    fn name(&self) -> &str { "WriteXISF" }
    fn version(&self) -> &str { "1.0" }
    fn description(&self) -> &str { "Writes all loaded images as XISF files" }

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
            ParamSpec {
                name:        "compress".to_string(),
                param_type:  ParamType::Boolean,
                required:    false,
                description: "Compress with LZ4HC + byte shuffling (default: false)".to_string(),
                default:     Some("false".to_string()),
            },
        ]
    }

    fn execute(&self, ctx: &mut AppContext, args: &ArgMap) -> Result<PluginOutput, PluginError> {
        let destination = args.get("destination")
            .ok_or_else(|| PluginError::new("MISSING_ARG", "destination argument required"))?
            .clone();

        let overwrite = args.get("overwrite")
            .map(|v| v == "true")
            .unwrap_or(false);

        let compress = args.get("compress")
            .map(|v| v == "true")
            .unwrap_or(false);

        if ctx.file_list.is_empty() {
            return Ok(PluginOutput::Message("No files loaded.".to_string()));
        }

        // Create destination directory if it doesn't exist
        std::fs::create_dir_all(&destination).map_err(|e| {
            PluginError::new("IO_ERROR", &format!("Cannot create directory '{}': {}", destination, e))
        })?;

        let options = WriteOptions {
            codec:           if compress { Codec::Lz4Hc } else { Codec::None },
            shuffle:         compress,
            creator_app:     "Photyx".to_string(),
            block_alignment: 4096,
        };

        let mut written = 0;
        let mut skipped = 0;
        let mut errors = 0;
        let total = ctx.file_list.len();

        for path in ctx.file_list.clone() {
            let buffer = match ctx.image_buffers.get(&path) {
                Some(b) => b,
                None => { errors += 1; continue; }
            };

            // Derive output filename — change extension to .xisf
            let stem = std::path::Path::new(&buffer.filename)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("image");
            let out_filename = format!("{}.xisf", stem);
            let out_path = format!("{}/{}", destination.trim_end_matches('/'), out_filename);

            if !overwrite && std::path::Path::new(&out_path).exists() {
                skipped += 1;
                continue;
            }

            // Convert Photyx ImageBuffer → photyx_xisf XisfImage
            let xisf_image = match buffer_to_xisf_image(buffer) {
                Ok(img) => img,
                Err(e) => {
                    tracing::warn!("Failed to convert {}: {}", path, e);
                    errors += 1;
                    continue;
                }
            };

            match XisfWriter::write(&out_path, &xisf_image, &options) {
                Ok(()) => {
                    info!("Wrote XISF: {}", out_path);
                    written += 1;
                }
                Err(e) => {
                    tracing::warn!("Failed to write '{}': {}", out_path, e);
                    errors += 1;
                }
            }
        }

        let msg = match (errors, skipped) {
            (0, 0) => format!("Wrote {} XISF file(s)", written),
            (0, s) => format!("Wrote {} XISF file(s), {} skipped (already exist)", written, s),
            (e, 0) => format!("Wrote {}/{} XISF file(s) ({} errors)", written, total, e),
            (e, s) => format!("Wrote {}/{} XISF file(s), {} skipped, {} errors", written, total, s, e),
        };

        Ok(PluginOutput::Message(msg))
    }
}

fn buffer_to_xisf_image(
    buffer: &crate::context::ImageBuffer,
) -> Result<XisfImage, String> {
    // Convert Photyx PixelData → photyx_xisf PixelData
    let pixels = match buffer.pixels.as_ref().ok_or("No pixel data")? {
        PixelData::U8(v)  => XisfPixelData::U8(v.clone()),
        PixelData::U16(v) => XisfPixelData::U16(v.clone()),
        PixelData::F32(v) => XisfPixelData::F32(v.clone()),
    };

    // Convert Photyx BitDepth → photyx_xisf SampleFormat
    let sample_format = match buffer.bit_depth {
        BitDepth::U8  => SampleFormat::UInt8,
        BitDepth::U16 => SampleFormat::UInt16,
        BitDepth::F32 => SampleFormat::Float32,
    };

    // Convert Photyx ColorSpace → photyx_xisf ColorSpace
    let color_space = match buffer.color_space {
        ColorSpace::Mono  => XisfColorSpace::Gray,
        ColorSpace::RGB   => XisfColorSpace::RGB,
        ColorSpace::Bayer => XisfColorSpace::CFA,
    };

    // Convert keywords — Photyx uses HashMap, XISF uses Vec (preserves order)
    let mut fits_keywords: Vec<photyx_xisf::FitsKeyword> = buffer.keywords
        .values()
        .map(|kw| photyx_xisf::FitsKeyword {
            name:    kw.name.clone(),
            value:   kw.value.clone(),
            comment: kw.comment.clone().unwrap_or_default(),
        })
        .collect();
    // Sort by name for deterministic output
    fits_keywords.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(XisfImage {
        width:         buffer.width,
        height:        buffer.height,
        channels:      buffer.channels as u32,
        sample_format,
        color_space,
        pixels,
        fits_keywords,
        properties:    Vec::new(), // XISF Properties not yet populated from Photyx session
    })
}

// Command alias — WriteAllXISFFiles is the pcode command name per spec §7.8
pub struct WriteAllXISFFiles;

impl PhotonPlugin for WriteAllXISFFiles {
    fn name(&self) -> &str { "WriteAllXISFFiles" }
    fn version(&self) -> &str { "1.0" }
    fn description(&self) -> &str { "Writes all loaded images as XISF files" }
    fn parameters(&self) -> Vec<ParamSpec> { WriteXISF.parameters() }
    fn execute(&self, ctx: &mut AppContext, args: &ArgMap) -> Result<PluginOutput, PluginError> {
        WriteXISF.execute(ctx, args)
    }
}
