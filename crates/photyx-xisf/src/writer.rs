// writer.rs — XISF file writer
//
// File format (monolithic XISF):
//   [8]  signature: "XISF0100"
//   [4]  header_length: u32 little-endian
//   [4]  reserved: zeros
//   [N]  XML header (UTF-8)
//   ...  data blocks at aligned offsets (default: 4096-byte alignment)
//
// The position of the image data block depends on the header length,
// which depends on the position string in the XML — a circular dependency.
// We resolve it by iterating until position string lengths stabilize.

use std::fs::File;
use std::io::Write;
use std::path::Path;

use crate::compress;
use crate::error::XisfError;
use crate::types::{
    Codec, ColorSpace, FitsKeyword, PixelData, SampleFormat,
    WriteOptions, XisfImage, XisfProperty, PropertyValue,
};

// ── Constants ─────────────────────────────────────────────────────────────────

const SIGNATURE: &[u8] = b"XISF0100";
const HEADER_LENGTH_LEN: usize = 4;
const RESERVED_LEN: usize = 4;
const XISF_XMLNS: &str = "http://www.pixinsight.com/xisf";

// ── Public API ────────────────────────────────────────────────────────────────

/// Writes XISF files.
pub struct XisfWriter;

impl XisfWriter {
    /// Write a single image to an XISF file.
    ///
    /// # Example
    /// ```no_run
    /// use photyx_xisf::{XisfReader, XisfWriter, WriteOptions};
    /// let reader = XisfReader::open("input.xisf").unwrap();
    /// let image = reader.read_image(0).unwrap();
    /// XisfWriter::write("output.xisf", &image, &WriteOptions::default()).unwrap();
    /// ```
    pub fn write(
        path:    impl AsRef<Path>,
        image:   &XisfImage,
        options: &WriteOptions,
    ) -> Result<(), XisfError> {
        write_xisf(path.as_ref(), image, options)
    }
}

// ── Implementation ────────────────────────────────────────────────────────────

fn write_xisf(path: &Path, image: &XisfImage, options: &WriteOptions) -> Result<(), XisfError> {
    // ── Step 1: Serialize pixel data to bytes ─────────────────────────────────
    let pixel_bytes = serialize_pixels(image);
    let uncompressed_size = pixel_bytes.len();
    let item_size = sample_format_item_size(&image.sample_format);

    // ── Step 2: Optionally compress ───────────────────────────────────────────
    let (data_block, compression_attr) = if options.codec != Codec::None {
        match compress::compress(&pixel_bytes, &options.codec, options.shuffle, item_size)? {
            Some(compressed) => {
                let codec_str = codec_to_str(&options.codec);
                let attr = if options.shuffle {
                    format!("{}+sh:{}:{}", codec_str, uncompressed_size, item_size)
                } else {
                    format!("{}:{}", codec_str, uncompressed_size)
                };
                (compressed, Some(attr))
            }
            None => (pixel_bytes, None), // compression didn't help, store uncompressed
        }
    } else {
        (pixel_bytes, None)
    };

    let data_size = data_block.len() as u64;

    // ── Step 3: Build XML header ──────────────────────────────────────────────
    // We need to compute the position of the data block, which requires knowing
    // the header size, which requires knowing the position string length.
    // We iterate until the position string length stabilizes (usually 1-2 iterations).

    let alignment = options.block_alignment;

    let xml = build_xml_header(image, options, data_size, compression_attr.as_deref(), 0)?;
    let provisional_header_size =
        SIGNATURE.len() + HEADER_LENGTH_LEN + RESERVED_LEN + xml.len();

    // Compute stable data block position
    let data_pos = compute_stable_position(provisional_header_size as u64, data_size, alignment,
        |pos| {
            build_xml_header(image, options, data_size, compression_attr.as_deref(), pos)
                .map(|x| x.len())
        }
    )?;

    // Build final XML with correct position
    let final_xml = build_xml_header(image, options, data_size, compression_attr.as_deref(), data_pos)?;

    // ── Step 4: Write file ────────────────────────────────────────────────────
    let mut f = File::create(path)?;

    // Signature
    f.write_all(SIGNATURE)?;

    // Header length (u32 little-endian)
    let header_len = final_xml.len() as u32;
    f.write_all(&header_len.to_le_bytes())?;

    // Reserved field (4 zero bytes)
    f.write_all(&[0u8; RESERVED_LEN])?;

    // XML header
    f.write_all(final_xml.as_bytes())?;

    // Padding to data block position
    let current_pos = (SIGNATURE.len() + HEADER_LENGTH_LEN + RESERVED_LEN + final_xml.len()) as u64;
    let padding = data_pos - current_pos;
    for _ in 0..padding {
        f.write_all(&[0u8])?;
    }

    // Data block
    assert_eq!(data_pos, current_pos + padding);
    f.write_all(&data_block)?;

    Ok(())
}

/// Compute the data block position that is stable under iteration.
/// The position depends on the header size, which depends on the position string,
/// which changes length as the number grows.
fn compute_stable_position(
    provisional_header_size: u64,
    data_size: u64,
    alignment: u64,
    xml_len_for_pos: impl Fn(u64) -> Result<usize, XisfError>,
) -> Result<u64, XisfError> {
    let mut pos = aligned(provisional_header_size, alignment);

    for _ in 0..10 {
        let xml_len = xml_len_for_pos(pos)? as u64;
        let header_size = SIGNATURE.len() as u64
            + HEADER_LENGTH_LEN as u64
            + RESERVED_LEN as u64
            + xml_len;
        let new_pos = aligned(header_size, alignment);
        if new_pos == pos {
            return Ok(pos);
        }
        pos = new_pos;
    }

    Ok(pos)
}

/// Round `n` up to the nearest multiple of `alignment`.
fn aligned(n: u64, alignment: u64) -> u64 {
    if alignment == 0 { return n; }
    ((n + alignment - 1) / alignment) * alignment
}

// ── XML header generation ─────────────────────────────────────────────────────

fn build_xml_header(
    image:            &XisfImage,
    options:          &WriteOptions,
    data_size:        u64,
    compression_attr: Option<&str>,
    data_pos:         u64,
) -> Result<String, XisfError> {
    let mut xml = String::new();

    // XML declaration
    xml.push_str(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
    xml.push('\n');

    // Root element
    xml.push_str(&format!(
        r#"<xisf version="1.0" xmlns="{}" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:schemaLocation="{} http://pixinsight.com/xisf/xisf-1.0.xsd">"#,
        XISF_XMLNS, XISF_XMLNS
    ));
    xml.push('\n');

    // Image element
    let geometry = format!("{}:{}:{}", image.width, image.height, image.channels);
    let sample_format = sample_format_to_str(&image.sample_format);
    let color_space = color_space_to_str(&image.color_space);
    let location = if data_pos == 0 {
        format!("attachment:0:{}", data_size)
    } else {
        format!("attachment:{}:{}", data_pos, data_size)
    };

    xml.push_str("   <Image");
    xml.push_str(&format!(r#" geometry="{}""#, geometry));
    xml.push_str(&format!(r#" sampleFormat="{}""#, sample_format));
    xml.push_str(&format!(r#" colorSpace="{}""#, color_space));
    if matches!(image.sample_format, SampleFormat::Float32 | SampleFormat::Float64) {
        xml.push_str(r#" bounds="0:1""#);
    }
    if let Some(comp) = compression_attr {
        xml.push_str(&format!(r#" compression="{}""#, comp));
    }
    xml.push_str(&format!(r#" location="{}""#, location));
    xml.push_str(">\n");

    // FITSKeyword elements
    for kw in &image.fits_keywords {
        let value = escape_fits_value(&kw.value);
        let comment = xml_escape(&kw.comment);
        xml.push_str(&format!(
            r#"      <FITSKeyword name="{}" value="{}" comment="{}"/>"#,
            xml_escape(&kw.name), value, comment
        ));
        xml.push('\n');
    }

    // XISFProperty elements
    for prop in &image.properties {
        if let Some(prop_xml) = property_to_xml(prop) {
            xml.push_str(&format!("      {}\n", prop_xml));
        }
    }

    xml.push_str("   </Image>\n");

    // Metadata element
    xml.push_str("   <Metadata>\n");
    xml.push_str(&format!(
        r#"      <Property id="XISF:CreationTime" type="String">{}</Property>"#,
        chrono_now()
    ));
    xml.push('\n');
    xml.push_str(&format!(
        r#"      <Property id="XISF:CreatorApplication" type="String">{}</Property>"#,
        xml_escape(&options.creator_app)
    ));
    xml.push('\n');
    xml.push_str(&format!(
        r#"      <Property id="XISF:CreatorModule" type="String">photyx-xisf v{}</Property>"#,
        env!("CARGO_PKG_VERSION")
    ));
    xml.push('\n');
    xml.push_str(&format!(
        r#"      <Property id="XISF:BlockAlignmentSize" type="UInt16" value="{}"/>"#,
        options.block_alignment
    ));
    xml.push('\n');
    if options.codec != Codec::None {
        xml.push_str(&format!(
            r#"      <Property id="XISF:CompressionCodecs" type="String">{}</Property>"#,
            codec_to_str(&options.codec)
        ));
        xml.push('\n');
    }
    xml.push_str("   </Metadata>\n");

    xml.push_str("</xisf>\n");

    Ok(xml)
}

fn property_to_xml(prop: &XisfProperty) -> Option<String> {
    match &prop.value {
        PropertyValue::String(s) => Some(format!(
            r#"<Property id="{}" type="String">{}</Property>"#,
            xml_escape(&prop.id), xml_escape(s)
        )),
        PropertyValue::Boolean(b) => Some(format!(
            r#"<Property id="{}" type="Boolean" value="{}"/>"#,
            xml_escape(&prop.id), if *b { "true" } else { "false" }
        )),
        PropertyValue::Int64(n) => Some(format!(
            r#"<Property id="{}" type="{}" value="{}"/>"#,
            xml_escape(&prop.id), prop.type_, n
        )),
        PropertyValue::UInt64(n) => Some(format!(
            r#"<Property id="{}" type="{}" value="{}"/>"#,
            xml_escape(&prop.id), prop.type_, n
        )),
        PropertyValue::Float64(f) => Some(format!(
            r#"<Property id="{}" type="{}" value="{}"/>"#,
            xml_escape(&prop.id), prop.type_, f
        )),
        PropertyValue::TimePoint(s) => Some(format!(
            r#"<Property id="{}" type="TimePoint" value="{}"/>"#,
            xml_escape(&prop.id), xml_escape(s)
        )),
        // Vector and Matrix — skip for now, not easily round-trippable without binary blocks
        PropertyValue::Vector(_) | PropertyValue::Matrix { .. } => None,
    }
}

// ── Pixel serialization ───────────────────────────────────────────────────────

/// Convert interleaved (channel-last) pixels to planar (channel-first) bytes.
/// XISF stores pixels in planar format: all channel 0, then all channel 1, etc.
fn serialize_pixels(image: &XisfImage) -> Vec<u8> {
    let pixel_count = (image.width * image.height) as usize;
    let channels = image.channels as usize;

    match &image.pixels {
        PixelData::U8(v) => {
            let planar = interleaved_to_planar(v, pixel_count, channels);
            planar
        }
        PixelData::U16(v) => {
            let planar = interleaved_to_planar(v, pixel_count, channels);
            let mut bytes = Vec::with_capacity(planar.len() * 2);
            for &p in &planar {
                bytes.extend_from_slice(&p.to_le_bytes());
            }
            bytes
        }
        PixelData::U32(v) => {
            let planar = interleaved_to_planar(v, pixel_count, channels);
            let mut bytes = Vec::with_capacity(planar.len() * 4);
            for &p in &planar {
                bytes.extend_from_slice(&p.to_le_bytes());
            }
            bytes
        }
        PixelData::F32(v) => {
            let planar = interleaved_to_planar(v, pixel_count, channels);
            let mut bytes = Vec::with_capacity(planar.len() * 4);
            for &p in &planar {
                bytes.extend_from_slice(&p.to_le_bytes());
            }
            bytes
        }
        PixelData::F64(v) => {
            let planar = interleaved_to_planar(v, pixel_count, channels);
            let mut bytes = Vec::with_capacity(planar.len() * 8);
            for &p in &planar {
                bytes.extend_from_slice(&p.to_le_bytes());
            }
            bytes
        }
    }
}

/// Convert interleaved (channel-last) layout to planar (channel-first).
/// Photyx: [px0ch0, px0ch1, ..., px1ch0, px1ch1, ...]
/// XISF:   [ch0px0, ch0px1, ..., ch1px0, ch1px1, ...]
fn interleaved_to_planar<T: Copy + Default>(
    interleaved: &[T],
    pixel_count: usize,
    channels: usize,
) -> Vec<T> {
    if channels == 1 {
        return interleaved.to_vec();
    }
    let mut out = vec![T::default(); pixel_count * channels];
    for px in 0..pixel_count {
        for ch in 0..channels {
            out[ch * pixel_count + px] = interleaved[px * channels + ch];
        }
    }
    out
}

// ── Format conversion helpers ─────────────────────────────────────────────────

fn sample_format_to_str(f: &SampleFormat) -> &'static str {
    match f {
        SampleFormat::UInt8   => "UInt8",
        SampleFormat::UInt16  => "UInt16",
        SampleFormat::UInt32  => "UInt32",
        SampleFormat::Float32 => "Float32",
        SampleFormat::Float64 => "Float64",
    }
}

fn sample_format_item_size(f: &SampleFormat) -> usize {
    match f {
        SampleFormat::UInt8   => 1,
        SampleFormat::UInt16  => 2,
        SampleFormat::UInt32  => 4,
        SampleFormat::Float32 => 4,
        SampleFormat::Float64 => 8,
    }
}

fn color_space_to_str(cs: &ColorSpace) -> &'static str {
    match cs {
        ColorSpace::Gray       => "Gray",
        ColorSpace::RGB        => "RGB",
        ColorSpace::CFA        => "Gray",  // CFA stored as single-channel
        ColorSpace::Unknown(_) => "Gray",
    }
}

fn codec_to_str(codec: &Codec) -> &'static str {
    match codec {
        Codec::Lz4   => "lz4",
        Codec::Lz4Hc => "lz4hc",
        Codec::Zlib  => "zlib",
        Codec::Zstd  => "zstd",
        Codec::None  => "",
    }
}

// ── XML helpers ───────────────────────────────────────────────────────────────

/// Escape special XML characters in attribute values and text content.
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
     .replace('<', "&lt;")
     .replace('>', "&gt;")
     .replace('"', "&quot;")
     .replace('\'', "&apos;")
}

/// Escape a FITS keyword value for use as an XML attribute.
/// FITS string values are not quoted in XISF — the quotes are stripped.
fn escape_fits_value(s: &str) -> String {
    xml_escape(s)
}

/// Return current UTC time as ISO 8601 string.
fn chrono_now() -> String {
    // Use a simple approach without the chrono crate dependency
    // Format: YYYY-MM-DDTHH:MM:SS (UTC approximation via SystemTime)
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Convert Unix timestamp to UTC date/time components
    let s = secs;
    let sec = s % 60;
    let min = (s / 60) % 60;
    let hour = (s / 3600) % 24;
    let days = s / 86400;

    // Days since epoch to date (simplified — good for 2024-2100)
    let (year, month, day) = days_to_ymd(days);

    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}", year, month, day, hour, min, sec)
}

/// Convert days since Unix epoch to (year, month, day).
fn days_to_ymd(days: u64) -> (u64, u64, u64) {
    // Algorithm from https://howardhinnant.github.io/date_algorithms.html
    let z = days + 719468;
    let era = z / 146097;
    let doe = z % 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}
