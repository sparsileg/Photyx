// plugins/image_reader.rs — Format-agnostic single image file reader
//
// Consolidates FITS, XISF, and TIFF readers. Dispatches to the appropriate
// format-specific reader based on file extension. Used by AddFiles and ReadImages,
// load_file (commands/display.rs), and the LoadFile pcode plugin.

use std::path::Path;
use std::collections::HashMap;
use tracing::info;
use crate::context::{ImageBuffer, BitDepth, ColorSpace, KeywordEntry, PixelData};

// ── Format dispatch ───────────────────────────────────────────────────────────

/// Read a single image file from disk into an ImageBuffer.
/// Dispatches based on file extension. Does not modify AppContext.
pub fn read_image_file(path: &str) -> Result<ImageBuffer, String> {
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    match ext.as_str() {
        "fit" | "fits" | "fts" => read_fits_file(path),
        "xisf"                 => read_xisf_file(path),
        "tif" | "tiff"         => read_tiff_file(path),
        other => Err(format!(
            "Unsupported file format: '{}'. Supported formats: fit, fits, fts, xisf, tif, tiff",
            other
        )),
    }
}

// ── FITS ──────────────────────────────────────────────────────────────────────

use fitsio::FitsFile;
use fitsio::hdu::HduInfo;
use fitsio::images::ImageType;

/// Peek at a FITS file header to get dimensions without reading pixel data.
/// Returns (width, height, channels, bytes_per_pixel).
pub fn peek_fits_dimensions(path: &str) -> Option<(u32, u32, u8, usize)> {
    let mut fitsfile = FitsFile::open(path).ok()?;
    let hdu = fitsfile.primary_hdu().ok()?;
    match &hdu.info {
        HduInfo::ImageInfo { shape, image_type } => {
            let (w, h, c) = match shape.as_slice() {
                [h, w]    => (*w as u32, *h as u32, 1u8),
                [_, h, w] => (*w as u32, *h as u32, 3u8),
                _         => return None,
            };
            let bpp = match image_type {
                ImageType::UnsignedByte => 1,
                ImageType::Float        => 4,
                ImageType::Double       => 4,
                _                       => 2,
            };
            Some((w, h, c, bpp))
        }
        _ => None,
    }
}

pub fn read_fits_file(path: &str) -> Result<ImageBuffer, String> {
    let mut fitsfile = FitsFile::open(path)
        .map_err(|e| format!("Cannot open: {}", e))?;

    let hdu = fitsfile.primary_hdu()
        .map_err(|e| format!("Cannot read primary HDU: {}", e))?;

    let (width, height, channels, bit_depth) = match &hdu.info {
        HduInfo::ImageInfo { shape, image_type } => {
            let (w, h, c) = match shape.as_slice() {
                [h, w]    => (*w as u32, *h as u32, 1u8),
                [_, h, w] => (*w as u32, *h as u32, 3u8),
                s         => return Err(format!("Unsupported image shape: {:?}", s)),
            };
            let bd = match image_type {
                ImageType::UnsignedByte  => BitDepth::U8,
                ImageType::Short         => BitDepth::U16,
                ImageType::UnsignedShort => BitDepth::U16,
                ImageType::Long          => BitDepth::U16,
                ImageType::Float         => BitDepth::F32,
                ImageType::Double        => BitDepth::F32,
                _                        => BitDepth::U16,
            };
            (w, h, c, bd)
        }
        _ => return Err("Primary HDU is not an image".to_string()),
    };

    let color_space = if channels == 3 { ColorSpace::RGB } else { ColorSpace::Mono };

    let mut keywords = HashMap::new();

    unsafe {
        let fptr = fitsfile.as_raw();
        let mut status: std::os::raw::c_int = 0;
        let mut nkeys: std::os::raw::c_int = 0;
        let mut morekeys: std::os::raw::c_int = 0;

        fitsio_sys::ffghsp(fptr, &mut nkeys, &mut morekeys, &mut status);

        for i in 1..=nkeys {
            let mut record = [0i8; 81];
            status = 0;
            fitsio_sys::ffgrec(fptr, i, record.as_mut_ptr(), &mut status);
            if status != 0 { continue; }

            let record_str: String = record[..80].iter()
                .map(|&c| if c == 0 { ' ' } else { c as u8 as char })
                .collect();

            let key = record_str[..8].trim().to_uppercase();
            if key.is_empty() || key == "COMMENT" || key == "HISTORY" || key == "END" {
                continue;
            }

            if record_str.len() > 10 && &record_str[8..9] == "=" {
                let rest = &record_str[9..].trim_start().to_string();
                let (value, comment) = if let Some(slash) = rest.find(" /") {
                    (rest[..slash].trim().to_string(), Some(rest[slash+2..].trim().to_string()))
                } else {
                    (rest.trim().to_string(), None)
                };
                let value = if value.starts_with('\'') && value.ends_with('\'') {
                    value[1..value.len()-1].trim_end().to_string()
                } else {
                    value
                };
                keywords.insert(
                    key.clone(),
                    KeywordEntry::new(&key, &value, comment.as_deref()),
                );
            }
        }
    }

    let color_space = if color_space == ColorSpace::Mono && keywords.contains_key("BAYERPAT") {
        ColorSpace::Bayer
    } else {
        color_space
    };

    let filename = Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path)
        .to_string();

    let pixels = match &bit_depth {
        BitDepth::U8 => {
            let data: Vec<u8> = hdu.read_image(&mut fitsfile)
                .map_err(|e| format!("Cannot read pixel data: {}", e))?;
            if data.is_empty() { return Err("Pixel data is empty".to_string()); }
            Some(PixelData::U8(data))
        }
        BitDepth::U16 => {
            let data: Vec<i32> = hdu.read_image(&mut fitsfile)
                .map_err(|e| format!("Cannot read pixel data: {}", e))?;
            if data.is_empty() { return Err("Pixel data is empty".to_string()); }
            let data_u16: Vec<u16> = data.iter().map(|&v| v.clamp(0, 65535) as u16).collect();
            Some(PixelData::U16(data_u16))
        }
        BitDepth::F32 => {
            let data: Vec<f32> = hdu.read_image(&mut fitsfile)
                .map_err(|e| format!("Cannot read pixel data: {}", e))?;
            if data.is_empty() { return Err("Pixel data is empty".to_string()); }
            Some(PixelData::F32(data))
        }
    };

    info!("Loaded FITS: {} ({}x{} {:?})", path, width, height, bit_depth);

    Ok(ImageBuffer {
        filename,
        width,
        height,
        display_width: 0,
        bit_depth,
        color_space,
        channels,
        keywords,
        pixels,
    })
}

// ── XISF ─────────────────────────────────────────────────────────────────────

use photyx_xisf::{XisfReader, PixelData as XisfPixelData, SampleFormat, ColorSpace as XisfColorSpace};

/// Peek at an XISF file header to get dimensions without reading pixel data.
/// Returns (width, height, channels, bytes_per_pixel).
pub fn peek_xisf_dimensions(path: &str) -> Option<(u32, u32, u8, usize)> {
    let reader = XisfReader::open(path).ok()?;
    if reader.image_count() == 0 { return None; }
    let meta = reader.image_meta(0).ok()?;
    let bpp = match meta.sample_format {
        SampleFormat::UInt8   => 1,
        SampleFormat::UInt16  => 2,
        SampleFormat::UInt32  => 2,
        SampleFormat::Float32 => 4,
        SampleFormat::Float64 => 4,
    };
    Some((meta.width, meta.height, meta.channels as u8, bpp))
}

pub fn read_xisf_file(path: &str) -> Result<ImageBuffer, String> {
    let reader = XisfReader::open(path)
        .map_err(|e| format!("Cannot open: {}", e))?;

    if reader.image_count() == 0 {
        return Err("No images in XISF file".to_string());
    }

    let meta = reader.image_meta(0)
        .map_err(|e| format!("Cannot read metadata: {}", e))?;

    let width    = meta.width;
    let height   = meta.height;
    let channels = meta.channels as u8;

    let bit_depth = match meta.sample_format {
        SampleFormat::UInt8   => BitDepth::U8,
        SampleFormat::UInt16  => BitDepth::U16,
        SampleFormat::UInt32  => BitDepth::U16,
        SampleFormat::Float32 => BitDepth::F32,
        SampleFormat::Float64 => BitDepth::F32,
    };

    let mut color_space = match meta.color_space {
        XisfColorSpace::Gray       => ColorSpace::Mono,
        XisfColorSpace::RGB        => ColorSpace::RGB,
        XisfColorSpace::CFA        => ColorSpace::Bayer,
        XisfColorSpace::Unknown(_) => ColorSpace::Mono,
    };

    let mut keywords = HashMap::new();
    for kw in &meta.fits_keywords {
        let name = kw.name.to_uppercase();
        if name == "COMMENT" || name == "HISTORY" { continue; }
        keywords.insert(
            name.clone(),
            KeywordEntry::new(&name, &kw.value, Some(&kw.comment)),
        );
    }

    if color_space == ColorSpace::Mono && keywords.contains_key("BAYERPAT") {
        color_space = ColorSpace::Bayer;
    }

    let filename = Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path)
        .to_string();

    let image = reader.read_image(0)
        .map_err(|e| format!("Cannot read pixels: {}", e))?;

    let pixels = match image.pixels {
        XisfPixelData::U8(v)  => Some(PixelData::U8(v)),
        XisfPixelData::U16(v) => Some(PixelData::U16(v)),
        XisfPixelData::U32(v) => Some(PixelData::U16(v.iter().map(|&p| (p >> 16) as u16).collect())),
        XisfPixelData::F32(v) => Some(PixelData::F32(v)),
        XisfPixelData::F64(v) => Some(PixelData::F32(v.iter().map(|&p| p as f32).collect())),
    };

    info!("Loaded XISF: {} ({}x{} {:?})", path, width, height, bit_depth);

    Ok(ImageBuffer {
        filename,
        width,
        height,
        display_width: 0,
        bit_depth,
        color_space,
        channels,
        keywords,
        pixels,
    })
}

// ── TIFF ─────────────────────────────────────────────────────────────────────

use tiff::decoder::{Decoder, DecodingResult};
use tiff::tags::Tag;
use tiff::ColorType;

/// Peek at a TIFF file header to get dimensions without reading pixel data.
/// Returns (width, height, channels, bytes_per_pixel).
pub fn peek_tiff_dimensions(path: &str) -> Option<(u32, u32, u8, usize)> {
    let file = std::fs::File::open(path).ok()?;
    let mut decoder = tiff::decoder::Decoder::new(file).ok()?;
    let (width, height) = decoder.dimensions().ok()?;
    let color_type = decoder.colortype().ok()?;
    let (channels, bpp): (u8, usize) = match color_type {
        tiff::ColorType::Gray(8)  => (1, 1),
        tiff::ColorType::Gray(16) => (1, 2),
        tiff::ColorType::Gray(32) => (1, 4),
        tiff::ColorType::RGB(8)   => (3, 1),
        tiff::ColorType::RGB(16)  => (3, 2),
        tiff::ColorType::RGB(32)  => (3, 4),
        _                         => (1, 2),
    };
    Some((width, height, channels, bpp))
}

pub fn read_tiff_file(path: &str) -> Result<ImageBuffer, String> {
    let file = std::fs::File::open(path)
        .map_err(|e| format!("Cannot open file: {e}"))?;

    let mut decoder = Decoder::new(file)
        .map_err(|e| format!("TIFF decode error: {e}"))?;

    let (width, height) = decoder.dimensions()
        .map_err(|e| format!("Cannot read dimensions: {e}"))?;

    let color_type = decoder.colortype()
        .map_err(|e| format!("Cannot read color type: {e}"))?;

    let (channels, color_space): (u8, ColorSpace) = match color_type {
        ColorType::Gray(_) => (1, ColorSpace::Mono),
        ColorType::RGB(_)  => (3, ColorSpace::RGB),
        other => return Err(format!("Unsupported color type: {other:?}")),
    };

    let result = decoder.read_image()
        .map_err(|e| format!("Failed to read image data: {e}"))?;

    let (pixels, bit_depth): (PixelData, BitDepth) = match result {
        DecodingResult::U8(data)  => (PixelData::U8(data), BitDepth::U8),
        DecodingResult::U16(data) => (PixelData::U16(data), BitDepth::U16),
        DecodingResult::U32(data) => {
            let converted: Vec<u16> = data.iter().map(|&v| (v >> 16) as u16).collect();
            (PixelData::U16(converted), BitDepth::U16)
        }
        DecodingResult::F32(data) => (PixelData::F32(data), BitDepth::F32),
        DecodingResult::F64(data) => {
            let converted: Vec<f32> = data.iter().map(|&v| v as f32).collect();
            (PixelData::F32(converted), BitDepth::F32)
        }
        other => return Err(format!("Unsupported pixel format: {other:?}")),
    };

    let filename = Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    let mut keywords: HashMap<String, KeywordEntry> = HashMap::new();
    keywords.insert(
        "FILENAME".to_string(),
        KeywordEntry::new("FILENAME", &filename, Some("Source filename")),
    );

    if let Ok(desc) = decoder.get_tag_ascii_string(Tag::ImageDescription) {
        for line in desc.lines() {
            let line = line.trim();
            if line.is_empty() { continue; }
            if line.len() < 10 || &line[8..9] != "=" { continue; }
            let name = line[..8].trim().to_uppercase();
            if name.is_empty() { continue; }
            let rest = line[9..].trim();
            let (value, comment) = if let Some(slash) = rest.find(" /") {
                (rest[..slash].trim().to_string(), Some(rest[slash+2..].trim().to_string()))
            } else {
                (rest.trim().to_string(), None)
            };
            let value = if value.starts_with('\'') && value.ends_with('\'') {
                value[1..value.len()-1].trim_end().to_string()
            } else {
                value
            };
            keywords.insert(name.clone(), KeywordEntry::new(&name, &value, comment.as_deref()));
        }
    }

    info!("Loaded TIFF: {} ({}x{} {:?})", path, width, height, bit_depth);

    Ok(ImageBuffer {
        filename,
        width,
        height,
        display_width: 0,
        bit_depth,
        color_space,
        channels,
        keywords,
        pixels: Some(pixels),
    })
}

// ----------------------------------------------------------------------
