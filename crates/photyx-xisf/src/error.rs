// error.rs — XISF error types

use thiserror::Error;

#[derive(Debug, Error)]
pub enum XisfError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Invalid XISF signature — not an XISF file")]
    InvalidSignature,

    #[error("XML parse error: {0}")]
    XmlParse(#[from] quick_xml::Error),

    #[error("Missing required XML attribute '{attr}' on element '{element}'")]
    MissingAttribute { element: String, attr: String },

    #[error("Unsupported sample format: '{0}'")]
    UnsupportedSampleFormat(String),

    #[error("Unsupported compression codec: '{0}'")]
    UnsupportedCodec(String),

    #[error("Unsupported data block location type: '{0}'")]
    UnsupportedLocation(String),

    #[error("Decompression error: {0}")]
    Decompression(String),

    #[error("Image index {index} out of range (file contains {count} images)")]
    ImageIndexOutOfRange { index: usize, count: usize },

    #[error("Unsupported geometry: '{0}' (only 2D images supported)")]
    UnsupportedGeometry(String),

    #[error("Unsupported property type: '{0}'")]
    UnsupportedPropertyType(String),

    #[error("UTF-8 decode error: {0}")]
    Utf8(#[from] std::str::Utf8Error),

    #[error("Base64 decode error: {0}")]
    Base64(String),
}
