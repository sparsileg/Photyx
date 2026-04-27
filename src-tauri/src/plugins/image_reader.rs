// plugins/image_reader.rs — Format-agnostic single image file reader
//
// Dispatches to the appropriate format-specific reader based on file extension.
// Used by load_file (lib.rs) and the LoadFile pcode plugin.
// The per-format read_*_file() functions remain the single source of truth
// for each format — this module just routes between them.

use crate::context::ImageBuffer;
use crate::plugins::{read_fits, read_tiff, read_xisf};

/// Read a single image file from disk into an ImageBuffer.
/// Dispatches based on file extension. Does not modify AppContext.
pub fn read_image_file(path: &str) -> Result<ImageBuffer, String> {
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "fit" | "fits" | "fts" => read_fits::read_fits_file(path),
        "xisf"                 => read_xisf::read_xisf_file(path),
        "tif" | "tiff"         => read_tiff::read_tiff_file(path),
        other => Err(format!(
            "Unsupported file format: '{}'. Supported formats: fit, fits, fts, xisf, tif, tiff",
            other
        )),
    }
}

// ----------------------------------------------------------------------
