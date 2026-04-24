// plugins/write_current_files.rs — WriteCurrent built-in native plugin
// Writes all buffered images back to their source paths in their source format.
// Spec §5.3, §6.3

use tracing::{info, warn};
use crate::plugin::{PhotonPlugin, ArgMap, ParamSpec, PluginOutput, PluginError};
use crate::context::AppContext;
use super::write_fits::write_fits_new;
use super::write_tiff::write_tiff_file;

pub struct WriteCurrent;

impl PhotonPlugin for WriteCurrent {
    fn name(&self)        -> &str { "WriteCurrent" }
    fn version(&self)     -> &str { "1.0" }
    fn description(&self) -> &str { "Writes all buffered images back to their source paths in their original format" }
    fn parameters(&self)  -> Vec<ParamSpec> { vec![] }

    fn execute(&self, ctx: &mut AppContext, _args: &ArgMap) -> Result<PluginOutput, PluginError> {
        if ctx.file_list.is_empty() {
            return Ok(PluginOutput::Message("No files loaded.".to_string()));
        }

        let mut written = 0usize;
        let mut errors  = 0usize;
        let total = ctx.file_list.len();

        for path in ctx.file_list.clone() {
            let ext = std::path::Path::new(&path)
                .extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();

            match ext.as_str() {
                "fit" | "fits" | "fts" => {
                    let buffer = match ctx.image_buffers.get(&path) {
                        Some(b) => b,
                        None => { errors += 1; continue; }
                    };
                    // Write to temp file then atomically replace.
                    // This ensures deleted keywords are not preserved and avoids
                    // duplicate keyword issues from in-place editing.
                    let temp_path = format!("{}.tmp", path);
                    let _ = std::fs::remove_file(&temp_path);
                    match write_fits_new(&temp_path, buffer) {
                        Ok(()) => {
                            if let Err(e) = std::fs::rename(&temp_path, &path) {
                                warn!("WriteCurrent: cannot replace {}: {}", path, e);
                                let _ = std::fs::remove_file(&temp_path);
                                errors += 1;
                            } else {
                                info!("WriteCurrent: updated FITS {}", path);
                                written += 1;
                            }
                        }
                        Err(e) => {
                            warn!("WriteCurrent: FITS write error {}: {}", path, e);
                            let _ = std::fs::remove_file(&temp_path);
                            errors += 1;
                        }
                    }
                }
                "xisf" => {
                    let temp_path = format!("{}.tmp", path);
                    let buffer = match ctx.image_buffers.get(&path) {
                        Some(b) => b,
                        None => { errors += 1; continue; }
                    };
                    let xisf_image = match super::write_xisf::buffer_to_xisf_image(buffer) {
                        Ok(img) => img,
                        Err(e) => {
                            warn!("WriteCurrent: XISF convert error {}: {}", path, e);
                            errors += 1;
                            continue;
                        }
                    };
                    let options = photyx_xisf::WriteOptions {
                        codec:           photyx_xisf::Codec::None,
                        shuffle:         false,
                        creator_app:     "Photyx".to_string(),
                        block_alignment: 4096,
                    };
                    match photyx_xisf::XisfWriter::write(&temp_path, &xisf_image, &options) {
                        Ok(()) => {
                            if let Err(e) = std::fs::rename(&temp_path, &path) {
                                warn!("WriteCurrent: cannot replace {}: {}", path, e);
                                let _ = std::fs::remove_file(&temp_path);
                                errors += 1;
                            } else {
                                info!("WriteCurrent: updated XISF {}", path);
                                written += 1;
                            }
                        }
                        Err(e) => {
                            warn!("WriteCurrent: XISF write error {}: {}", path, e);
                            let _ = std::fs::remove_file(&temp_path);
                            errors += 1;
                        }
                    }
                }
                "tif" | "tiff" => {
                    let temp_path = format!("{}.tmp", path);
                    let buffer = match ctx.image_buffers.get(&path) {
                        Some(b) => b,
                        None => { errors += 1; continue; }
                    };
                    match write_tiff_file(&temp_path, buffer) {
                        Ok(()) => {
                            if let Err(e) = std::fs::rename(&temp_path, &path) {
                                warn!("WriteCurrent: cannot replace {}: {}", path, e);
                                let _ = std::fs::remove_file(&temp_path);
                                errors += 1;
                            } else {
                                info!("WriteCurrent: updated TIFF {}", path);
                                written += 1;
                            }
                        }
                        Err(e) => {
                            warn!("WriteCurrent: TIFF write error {}: {}", path, e);
                            let _ = std::fs::remove_file(&temp_path);
                            errors += 1;
                        }
                    }
                }
                _ => {
                    // Silently ignore unsupported formats
                }
            }
        }

        let msg = if errors > 0 {
            format!("Wrote {}/{} file(s) ({} errors)", written, total, errors)
        } else {
            format!("Wrote {} file(s)", written)
        };

        Ok(PluginOutput::Message(msg))
    }
}


// ----------------------------------------------------------------------
