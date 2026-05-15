// plugins/write_xisf.rs — WriteXISF built-in native plugin
// Spec §5.3, §6.3

use tracing::info;
use photyx_xisf::{XisfWriter, WriteOptions, Codec};
use photyx_xisf::{XisfImage, PixelData as XisfPixelData, SampleFormat, ColorSpace as XisfColorSpace};
use crate::plugin::{PhotonPlugin, ArgMap, ParamSpec, ParamType, PluginOutput, PluginError};
use crate::context::{AppContext, BitDepth, ColorSpace, PixelData};

pub struct WriteXISF;

impl PhotonPlugin for WriteXISF {
    fn name(&self)        -> &str { "WriteXISF" }
    fn version(&self)     -> &str { "1.0" }
    fn description(&self) -> &str { "Writes all loaded images as XISF files to a destination directory" }

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
            ParamSpec {
                name:        "stack".to_string(),
                param_type:  ParamType::Boolean,
                required:    false,
                description: "Write the transient stack result instead of session files (default: false)".to_string(),
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
        let compress   = args.get("compress").map(|v| v == "true").unwrap_or(false);
        let write_stack = args.get("stack").map(|v| v == "true").unwrap_or(false);

        if write_stack {
            return write_stack_result(ctx, &destination, overwrite, compress);
        }

        if ctx.file_list.is_empty() {
            return Ok(PluginOutput::Message("No files loaded.".to_string()));
        }

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
        let mut errors  = 0;
        let total = ctx.file_list.len();

        for path in ctx.file_list.clone() {
            let buffer = match ctx.image_buffers.get(&path) {
                Some(b) => b,
                None => { errors += 1; continue; }
            };

            let stem = std::path::Path::new(&buffer.filename)
                .file_stem().and_then(|s| s.to_str()).unwrap_or("image");
            let out_path = format!("{}/{}.xisf", destination.trim_end_matches('/'), stem);

            if !overwrite && std::path::Path::new(&out_path).exists() {
                skipped += 1;
                continue;
            }

            let xisf_image = match buffer_to_xisf_image(buffer) {
                Ok(img) => img,
                Err(e) => { tracing::warn!("Failed to convert {}: {}", path, e); errors += 1; continue; }
            };

            match XisfWriter::write(&out_path, &xisf_image, &options) {
                Ok(()) => { info!("Wrote XISF: {}", out_path); written += 1; }
                Err(e) => { tracing::warn!("Failed to write '{}': {}", out_path, e); errors += 1; }
            }
        }

        let msg = match (errors, skipped) {
            (0, 0) => format!("Wrote {} XISF file(s)", written),
            (0, s) => format!("Wrote {} XISF file(s), {} skipped", written, s),
            (e, 0) => format!("Wrote {}/{} XISF file(s) ({} errors)", written, total, e),
            (e, s) => format!("Wrote {}/{} XISF file(s), {} skipped, {} errors", written, total, s, e),
        };

        Ok(PluginOutput::Message(msg))
    }
}

fn write_stack_result(
    ctx: &mut AppContext,
    destination: &str,
    overwrite: bool,
    compress: bool,
) -> Result<PluginOutput, PluginError> {
    let buffer = ctx.stack_result.as_ref()
        .ok_or_else(|| PluginError::new("NO_STACK", "No stack result available. Run StackFrames first."))?;

    std::fs::create_dir_all(destination).map_err(|e| {
        PluginError::new("IO_ERROR", &format!("Cannot create directory '{}': {}", destination, e))
    })?;

    // Build suggested filename from stack_summary (§3.10)
    let filename = if let Some(summary) = &ctx.stack_summary {
        let target    = summary.target.as_deref().unwrap_or("unknown").replace(' ', "_");
        let filter    = summary.filter.as_deref().unwrap_or("nofilter").replace(' ', "_");
        let int_secs  = summary.integration_seconds.round() as u64;
        let timestamp = summary.completed_at
            .replace(['-', ':', 'T'], "")
            .chars().take(15).collect::<String>()
            .replace(' ', "_");
        format!("Photyx_stack_{}_{}_{:}s_{}.xisf", target, filter, int_secs, timestamp)
    } else {
        "Photyx_stack.xisf".to_string()
    };

    let out_path = format!("{}/{}", destination.trim_end_matches('/'), filename);

    if !overwrite && std::path::Path::new(&out_path).exists() {
        return Ok(PluginOutput::Message(format!("Skipped — file already exists: {}", out_path)));
    }

    let options = WriteOptions {
        codec:           if compress { Codec::Lz4Hc } else { Codec::None },
        shuffle:         compress,
        creator_app:     "Photyx".to_string(),
        block_alignment: 4096,
    };

    let xisf_image = buffer_to_xisf_image(buffer)
        .map_err(|e| PluginError::new("CONVERT_ERROR", &e))?;

    XisfWriter::write(&out_path, &xisf_image, &options)
        .map_err(|e| PluginError::new("WRITE_ERROR", &format!("Failed to write '{}': {}", out_path, e)))?;

    info!("Wrote stack XISF: {}", out_path);
    Ok(PluginOutput::Message(format!("Stack exported: {}", out_path)))
}

pub(crate) fn buffer_to_xisf_image(buffer: &crate::context::ImageBuffer) -> Result<XisfImage, String> {
    let pixels = match buffer.pixels.as_ref().ok_or("No pixel data")? {
        PixelData::U8(v)  => XisfPixelData::U8(v.clone()),
        PixelData::U16(v) => XisfPixelData::U16(v.clone()),
        PixelData::F32(v) => XisfPixelData::F32(v.clone()),
    };

    let sample_format = match buffer.bit_depth {
        BitDepth::U8  => SampleFormat::UInt8,
        BitDepth::U16 => SampleFormat::UInt16,
        BitDepth::F32 => SampleFormat::Float32,
    };

    let color_space = match buffer.color_space {
        ColorSpace::Mono  => XisfColorSpace::Gray,
        ColorSpace::RGB   => XisfColorSpace::RGB,
        ColorSpace::Bayer => XisfColorSpace::CFA,
    };

    let mut fits_keywords: Vec<photyx_xisf::FitsKeyword> = buffer.keywords
        .values()
        .map(|kw| photyx_xisf::FitsKeyword {
            name:    kw.name.clone(),
            value:   kw.value.clone(),
            comment: kw.comment.clone().unwrap_or_default(),
        })
        .collect();
    fits_keywords.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(XisfImage {
        width:         buffer.width,
        height:        buffer.height,
        channels:      buffer.channels as u32,
        sample_format,
        color_space,
        pixels,
        fits_keywords,
        properties:    Vec::new(),
    })
}

// ----------------------------------------------------------------------
