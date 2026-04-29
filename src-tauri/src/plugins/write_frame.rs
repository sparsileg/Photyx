// plugins/write_frame.rs — WriteFrame built-in native plugin
// Writes the currently active frame back to its source path in its source format.
// Spec §5.3, §6.3

use tracing::info;
use crate::plugin::{PhotonPlugin, ArgMap, ParamSpec, PluginOutput, PluginError};
use crate::context::AppContext;
use super::write_fits::write_fits_new;
use super::write_tiff::write_tiff_file;

pub struct WriteFrame;

impl PhotonPlugin for WriteFrame {
    fn name(&self)        -> &str { "WriteFrame" }
    fn version(&self)     -> &str { "1.0" }
    fn description(&self) -> &str { "Writes the currently active frame back to its source path in its original format" }
    fn parameters(&self)  -> Vec<ParamSpec> { vec![] }

    fn execute(&self, ctx: &mut AppContext, _args: &ArgMap) -> Result<PluginOutput, PluginError> {
        let path = ctx.file_list.get(ctx.current_frame)
            .cloned()
            .ok_or_else(|| PluginError::new("NO_IMAGE", "No image loaded."))?;

        let ext = std::path::Path::new(&path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        let temp_path = format!("{}.tmp", path);
        let _ = std::fs::remove_file(&temp_path);

        match ext.as_str() {
            "fit" | "fits" | "fts" => {
                let buffer = ctx.image_buffers.get(&path)
                    .ok_or_else(|| PluginError::new("NO_BUFFER", "Image buffer not found."))?;
                match write_fits_new(&temp_path, buffer) {
                    Ok(()) => {
                        if let Err(e) = std::fs::rename(&temp_path, &path) {
                            let _ = std::fs::remove_file(&temp_path);
                            return Err(PluginError::new("WRITE_ERROR", &format!("Cannot replace file: {}", e)));
                        }
                        info!("WriteFrame: updated FITS {}", path);
                    }
                    Err(e) => {
                        let _ = std::fs::remove_file(&temp_path);
                        return Err(PluginError::new("WRITE_ERROR", &e.to_string()));
                    }
                }
            }
            "xisf" => {
                let buffer = ctx.image_buffers.get(&path)
                    .ok_or_else(|| PluginError::new("NO_BUFFER", "Image buffer not found."))?;
                let xisf_image = super::write_xisf::buffer_to_xisf_image(buffer)
                    .map_err(|e| PluginError::new("CONVERT_ERROR", &e))?;
                let options = photyx_xisf::WriteOptions {
                    codec:           photyx_xisf::Codec::None,
                    shuffle:         false,
                    creator_app:     "Photyx".to_string(),
                    block_alignment: 4096,
                };
                match photyx_xisf::XisfWriter::write(&temp_path, &xisf_image, &options) {
                    Ok(()) => {
                        if let Err(e) = std::fs::rename(&temp_path, &path) {
                            let _ = std::fs::remove_file(&temp_path);
                            return Err(PluginError::new("WRITE_ERROR", &format!("Cannot replace file: {}", e)));
                        }
                        info!("WriteFrame: updated XISF {}", path);
                    }
                    Err(e) => {
                        let _ = std::fs::remove_file(&temp_path);
                        return Err(PluginError::new("WRITE_ERROR", &e.to_string()));
                    }
                }
            }
            "tif" | "tiff" => {
                let buffer = ctx.image_buffers.get(&path)
                    .ok_or_else(|| PluginError::new("NO_BUFFER", "Image buffer not found."))?;
                match write_tiff_file(&temp_path, buffer) {
                    Ok(()) => {
                        if let Err(e) = std::fs::rename(&temp_path, &path) {
                            let _ = std::fs::remove_file(&temp_path);
                            return Err(PluginError::new("WRITE_ERROR", &format!("Cannot replace file: {}", e)));
                        }
                        info!("WriteFrame: updated TIFF {}", path);
                    }
                    Err(e) => {
                        let _ = std::fs::remove_file(&temp_path);
                        return Err(PluginError::new("WRITE_ERROR", &e.to_string()));
                    }
                }
            }
            _ => {
                return Err(PluginError::new("UNSUPPORTED_FORMAT",
                                            &format!("WriteFrame does not support .{} files", ext)));
            }
        }

        let filename = std::path::Path::new(&path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&path);

        Ok(PluginOutput::Message(format!("WriteFrame: wrote {}", filename)))
    }
}

// ----------------------------------------------------------------------
