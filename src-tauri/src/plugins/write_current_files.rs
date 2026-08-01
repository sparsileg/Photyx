// plugins/write_current_files.rs — WriteCurrent built-in native plugin
// Writes all buffered images back to their source paths in their source format.
// Spec §5.3, §6.3

use tracing::{info, warn};
use crate::plugin::{PhotyxPlugin, ArgMap, ParamSpec, PluginOutput, PluginError};
use crate::context::AppContext;
use super::atomic_write::atomic_write;

pub struct WriteCurrent;

impl PhotyxPlugin for WriteCurrent {
    fn name(&self)        -> &str { "WriteCurrent" }
    fn version(&self)     -> &str { "1.1.0" }
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
                    match super::write_fits::update_fits_keywords(&path, buffer) {
                        Ok(()) => {
                            info!("WriteCurrent: updated FITS keywords {}", path);
                            written += 1;
                        }
                        Err(e) => {
                            warn!("WriteCurrent: FITS keyword update error {}: {}", path, e);
                            errors += 1;
                        }
                    }
                }
                "xisf" => {
                    let buffer = match ctx.image_buffers.get(&path) {
                        Some(b) => b,
                        None => { errors += 1; continue; }
                    };
                    let pixels = match super::pixel_chunking::load_request(&path, super::pixel_chunking::LoadKind::Raw) {
                        super::pixel_chunking::LoadOutcome::Loaded(super::pixel_chunking::LoadedFrame::Raw(snap)) => snap.pixels,
                        super::pixel_chunking::LoadOutcome::Loaded(_) => {
                            warn!("WriteCurrent: XISF read error {}: unexpected LoadedFrame kind for a Raw request", path);
                            errors += 1;
                            continue;
                        }
                        super::pixel_chunking::LoadOutcome::Missing { .. } => {
                            warn!("WriteCurrent: XISF read error {}: source file missing", path);
                            errors += 1;
                            continue;
                        }
                        super::pixel_chunking::LoadOutcome::Unreadable { error, .. } => {
                            warn!("WriteCurrent: XISF read error {}: {}", path, error);
                            errors += 1;
                            continue;
                        }
                    };
                    let xisf_image = match super::write_xisf::build_xisf_image(buffer, pixels) {
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
                    match atomic_write(&path, |tmp| {
                        photyx_xisf::XisfWriter::write(tmp, &xisf_image, &options).map_err(|e| e.to_string())
                    }) {
                        Ok(()) => {
                            info!("WriteCurrent: updated XISF {}", path);
                            written += 1;
                        }
                        Err(e) => {
                            warn!("WriteCurrent: XISF write error {}: {}", path, e);
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
// ----------------------------------------------------------------------
// ----------------------------------------------------------------------
