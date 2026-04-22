// types.rs — Public XISF data types

/// Pixel sample format, including all XISF canonical and alternate names.
#[derive(Debug, Clone, PartialEq)]
pub enum SampleFormat {
    UInt8,
    UInt16,
    UInt32,
    Float32,
    Float64,
}

/// Color space of the image.
#[derive(Debug, Clone, PartialEq)]
pub enum ColorSpace {
    Gray,
    RGB,
    CFA,   // Bayer mosaic
    Unknown(String),
}

/// Raw pixel data buffer.
#[derive(Debug, Clone)]
pub enum PixelData {
    U8(Vec<u8>),
    U16(Vec<u16>),
    U32(Vec<u32>),
    F32(Vec<f32>),
    F64(Vec<f64>),
}

impl PixelData {
    pub fn len(&self) -> usize {
        match self {
            PixelData::U8(v)  => v.len(),
            PixelData::U16(v) => v.len(),
            PixelData::U32(v) => v.len(),
            PixelData::F32(v) => v.len(),
            PixelData::F64(v) => v.len(),
        }
    }

    pub fn is_empty(&self) -> bool { self.len() == 0 }

    pub fn item_size(&self) -> usize {
        match self {
            PixelData::U8(_)  => 1,
            PixelData::U16(_) => 2,
            PixelData::U32(_) => 4,
            PixelData::F32(_) => 4,
            PixelData::F64(_) => 8,
        }
    }
}

/// A single FITS keyword entry.
/// Note: the same keyword name can appear multiple times in a file.
#[derive(Debug, Clone)]
pub struct FitsKeyword {
    pub name:    String,
    pub value:   String,
    pub comment: String,
}

/// Value of an XISF Property.
#[derive(Debug, Clone)]
pub enum PropertyValue {
    String(String),
    Boolean(bool),
    Int64(i64),
    UInt64(u64),
    Float64(f64),
    TimePoint(String),   // ISO 8601 string — conversion to DateTime deferred
    Vector(Vec<f64>),
    Matrix { rows: usize, cols: usize, data: Vec<f64> },
}

/// An XISF Property entry from the Properties block.
#[derive(Debug, Clone)]
pub struct XisfProperty {
    pub id:    String,
    pub type_: String,
    pub value: PropertyValue,
}

/// Compression codec.
#[derive(Debug, Clone, PartialEq)]
pub enum Codec {
    None,
    Lz4,
    Lz4Hc,
    Zlib,
    Zstd,
}

impl Default for Codec {
    fn default() -> Self { Codec::Lz4Hc }
}

/// Metadata for a single image in an XISF file.
/// Parsed from the XML header — no pixel data loaded yet.
#[derive(Debug, Clone)]
pub struct XisfImageMeta {
    pub width:        u32,
    pub height:       u32,
    pub channels:     u32,
    pub sample_format: SampleFormat,
    pub color_space:  ColorSpace,
    pub fits_keywords: Vec<FitsKeyword>,
    pub properties:   Vec<XisfProperty>,
    /// Internal — byte offset and size of pixel data block in file.
    /// None for inline/embedded blocks.
    pub(crate) location: DataBlockLocation,
    pub(crate) compression: Option<CompressionInfo>,
}

/// A fully loaded XISF image with pixel data.
#[derive(Debug, Clone)]
pub struct XisfImage {
    pub width:         u32,
    pub height:        u32,
    pub channels:      u32,
    pub sample_format: SampleFormat,
    pub color_space:   ColorSpace,
    pub pixels:        PixelData,
    pub fits_keywords: Vec<FitsKeyword>,
    pub properties:    Vec<XisfProperty>,
}

/// Options for writing XISF files.
#[derive(Debug, Clone)]
pub struct WriteOptions {
    pub codec:        Codec,
    pub shuffle:      bool,
    pub creator_app:  String,
    /// Block alignment size in bytes (default: 4096).
    pub block_alignment: u64,
}

impl Default for WriteOptions {
    fn default() -> Self {
        WriteOptions {
            codec:           Codec::Lz4Hc,
            shuffle:         true,
            creator_app:     "photyx-xisf".to_string(),
            block_alignment: 4096,
        }
    }
}

/// Internal: data block location parsed from XML location attribute.
#[derive(Debug, Clone)]
pub(crate) enum DataBlockLocation {
    Attachment { offset: u64, size: u64 },
    #[allow(dead_code)]
    Inline     { encoding: String, data: Vec<u8> },
    Embedded   { data: Vec<u8> },
}

/// Internal: compression parameters parsed from XML compression attribute.
#[derive(Debug, Clone)]
pub(crate) struct CompressionInfo {
    pub codec:             Codec,
    pub uncompressed_size: u64,
    pub item_size:         Option<usize>,  // Some(n) = byte-shuffled with item size n
}
