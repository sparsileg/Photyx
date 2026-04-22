// photyx-xisf — XISF image format reader and writer
// Spec §4.5, §5.2, §5.3
//
// Copyright (c) 2026 Stanley Grant Barton
// Licensed under MIT OR Apache-2.0

pub mod error;
pub mod reader;
pub mod writer;
pub mod types;
pub(crate) mod compress;

pub use error::XisfError;
pub use types::{
    Codec, ColorSpace, FitsKeyword, PixelData, PropertyValue,
    SampleFormat, WriteOptions, XisfImage, XisfImageMeta, XisfProperty,
};
pub use reader::XisfReader;
pub use writer::XisfWriter;
