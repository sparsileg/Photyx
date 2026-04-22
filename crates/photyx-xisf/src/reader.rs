// reader.rs — XISF file reader
//
// File format (monolithic XISF):
//   [8]  signature: "XISF0100"
//   [4]  header_length: u32 little-endian
//   [4]  reserved: zeros
//   [N]  XML header (may have null padding — strip before parsing)
//   ...  data blocks at aligned offsets

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use quick_xml::Reader as XmlReader;
use quick_xml::events::Event;

use crate::compress;
use crate::error::XisfError;
use crate::types::{
    Codec, ColorSpace, CompressionInfo, DataBlockLocation,
    FitsKeyword, PixelData, PropertyValue, SampleFormat,
    XisfImage, XisfImageMeta, XisfProperty,
};

// ── Constants ─────────────────────────────────────────────────────────────────

const SIGNATURE: &[u8] = b"XISF0100";
const HEADER_LENGTH_LEN: usize = 4;
const RESERVED_LEN: usize = 4;
const XISF_NS: &str = "http://www.pixinsight.com/xisf";

// ── Public struct ─────────────────────────────────────────────────────────────

/// Reads XISF files. Open with `XisfReader::open()`, then call `read_image()`
/// to load pixel data for a specific image.
pub struct XisfReader {
    path:       PathBuf,
    image_meta: Vec<XisfImageMeta>,
    file_props: Vec<XisfProperty>,
}

impl XisfReader {
    /// Open an XISF file and parse its XML header.
    /// Pixel data is not loaded until `read_image()` is called.
    ///
    /// # Example
    /// ```no_run
    /// use photyx_xisf::XisfReader;
    /// let reader = XisfReader::open("image.xisf").unwrap();
    /// println!("{} image(s) found", reader.image_count());
    /// ```
    pub fn open(path: impl AsRef<Path>) -> Result<Self, XisfError> {
        let path = path.as_ref().to_path_buf();
        let mut f = File::open(&path)?;

        // ── Validate signature ────────────────────────────────────────────────
        let mut sig = [0u8; 8];
        f.read_exact(&mut sig)?;
        if sig != SIGNATURE {
            return Err(XisfError::InvalidSignature);
        }

        // ── Read header length ────────────────────────────────────────────────
        let mut len_buf = [0u8; HEADER_LENGTH_LEN];
        f.read_exact(&mut len_buf)?;
        let header_len = u32::from_le_bytes(len_buf) as usize;

        // ── Skip reserved field ───────────────────────────────────────────────
        f.seek(SeekFrom::Current(RESERVED_LEN as i64))?;

        // ── Read XML header ───────────────────────────────────────────────────
        let mut xml_bytes = vec![0u8; header_len];
        f.read_exact(&mut xml_bytes)?;
        // Strip null padding that some tools add at the end of the header
        let xml_bytes = xml_bytes
            .iter()
            .rposition(|&b| b != 0)
            .map(|pos| &xml_bytes[..=pos])
            .unwrap_or(&xml_bytes);
        let xml_str = std::str::from_utf8(xml_bytes)?;

        // ── Parse XML ─────────────────────────────────────────────────────────
        let (image_meta, file_props) = parse_header(xml_str)?;

        Ok(XisfReader { path, image_meta, file_props })
    }

    /// Number of images in the file.
    pub fn image_count(&self) -> usize {
        self.image_meta.len()
    }

    /// Metadata for image at `index` (no pixel data loaded).
    pub fn image_meta(&self, index: usize) -> Result<&XisfImageMeta, XisfError> {
        self.image_meta.get(index).ok_or(XisfError::ImageIndexOutOfRange {
            index,
            count: self.image_meta.len(),
        })
    }

    /// Read and decompress pixel data for image at `index`.
    pub fn read_image(&self, index: usize) -> Result<XisfImage, XisfError> {
        let meta = self.image_meta.get(index).ok_or(XisfError::ImageIndexOutOfRange {
            index,
            count: self.image_meta.len(),
        })?;

        // ── Read raw bytes from data block ────────────────────────────────────
        let raw = read_data_block(&self.path, &meta.location)?;

        // ── Decompress if needed ──────────────────────────────────────────────
        let raw = if let Some(ref info) = meta.compression {
            compress::decompress(&raw, info)?
        } else {
            raw
        };

        // ── Deserialize pixels ────────────────────────────────────────────────
        let pixels = deserialize_pixels(&raw, &meta.sample_format,
            meta.width, meta.height, meta.channels)?;

        Ok(XisfImage {
            width:         meta.width,
            height:        meta.height,
            channels:      meta.channels,
            sample_format: meta.sample_format.clone(),
            color_space:   meta.color_space.clone(),
            pixels,
            fits_keywords: meta.fits_keywords.clone(),
            properties:    meta.properties.clone(),
        })
    }

    /// File-level XISF properties from the `<Metadata>` block.
    pub fn file_properties(&self) -> &[XisfProperty] {
        &self.file_props
    }
}

// ── XML Header Parsing ────────────────────────────────────────────────────────

/// Parse the XISF XML header string into image metadata and file properties.
fn parse_header(xml: &str) -> Result<(Vec<XisfImageMeta>, Vec<XisfProperty>), XisfError> {
    // We use quick-xml in buffered reader mode
    // The XISF header is a single <xisf> root element containing
    // <Image> and <Metadata> child elements.

    // Parse using minidom-style approach: read the full XML into a tree
    // quick-xml doesn't have a DOM, so we'll use it in event mode carefully.
    // For simplicity and correctness, we parse the XML as a string using
    // quick-xml's reader and build our own lightweight structure.

    let mut image_metas = Vec::new();
    let mut file_props = Vec::new();

    // We'll use a simple recursive-descent approach over quick-xml events.
    // State machine: we track where we are in the tree.
    enum State {
        Root,
        InImage(ImageBuilder),
        InMetadata,
    }

    let mut reader = XmlReader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut state = State::Root;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
            let name = e.name();
            let local = local_name(name.as_ref());
                match (&mut state, local) {
                    (State::Root, "Image") => {
                        let builder = parse_image_attrs(e)?;
                        state = State::InImage(builder);
                    }
                    (State::Root, "Metadata") => {
                        state = State::InMetadata;
                    }
                    (State::InImage(ref mut builder), "FITSKeyword") => {
                        if let Some(kw) = parse_fits_keyword(e)? {
                            builder.fits_keywords.push(kw);
                        }
                    }
                    (State::InImage(ref mut builder), "Property") => {
                        if let Some(prop) = parse_property(e)? {
                            builder.properties.push(prop);
                        }
                    }
                    (State::InMetadata, "Property") => {
                        if let Some(prop) = parse_property(e)? {
                            file_props.push(prop);
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::End(ref e)) => {
            let name = e.name();
            let local = local_name(name.as_ref());
                match (&mut state, local) {
                    (State::InImage(_), "Image") => {
                        if let State::InImage(builder) = std::mem::replace(&mut state, State::Root) {
                            image_metas.push(builder.build()?);
                        }
                    }
                    (State::InMetadata, "Metadata") => {
                        state = State::Root;
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(XisfError::XmlParse(e)),
            _ => {}
        }
        buf.clear();
    }

    Ok((image_metas, file_props))
}

/// Strip XML namespace prefix from element name.
fn local_name(name: &[u8]) -> &str {
    let s = std::str::from_utf8(name).unwrap_or("");
    if let Some(pos) = s.find(':') {
        &s[pos + 1..]
    } else {
        s
    }
}

// ── Image attribute parsing ───────────────────────────────────────────────────

struct ImageBuilder {
    width:         u32,
    height:        u32,
    channels:      u32,
    sample_format: SampleFormat,
    color_space:   ColorSpace,
    location:      DataBlockLocation,
    compression:   Option<CompressionInfo>,
    fits_keywords: Vec<FitsKeyword>,
    properties:    Vec<XisfProperty>,
}

impl ImageBuilder {
    fn build(self) -> Result<XisfImageMeta, XisfError> {
        Ok(XisfImageMeta {
            width:         self.width,
            height:        self.height,
            channels:      self.channels,
            sample_format: self.sample_format,
            color_space:   self.color_space,
            location:      self.location,
            compression:   self.compression,
            fits_keywords: self.fits_keywords,
            properties:    self.properties,
        })
    }
}

fn parse_image_attrs(e: &quick_xml::events::BytesStart) -> Result<ImageBuilder, XisfError> {
    let mut geometry = None;
    let mut location_str = None;
    let mut sample_format_str = None;
    let mut color_space_str = None;
    let mut compression_str = None;

    for attr in e.attributes().flatten() {
        let key = std::str::from_utf8(attr.key.as_ref()).unwrap_or("").to_string();
        let val = attr.unescape_value().unwrap_or_default().to_string();
        match key.as_str() {
            "geometry"     => geometry = Some(val),
            "location"     => location_str = Some(val),
            "sampleFormat" => sample_format_str = Some(val),
            "colorSpace"   => color_space_str = Some(val),
            "compression"  => compression_str = Some(val),
            _ => {}
        }
    }

    let geometry = geometry.ok_or_else(|| XisfError::MissingAttribute {
        element: "Image".into(), attr: "geometry".into(),
    })?;
    let location_str = location_str.ok_or_else(|| XisfError::MissingAttribute {
        element: "Image".into(), attr: "location".into(),
    })?;
    let sample_format_str = sample_format_str.ok_or_else(|| XisfError::MissingAttribute {
        element: "Image".into(), attr: "sampleFormat".into(),
    })?;

    let (width, height, channels) = parse_geometry(&geometry)?;
    let sample_format = parse_sample_format(&sample_format_str)?;
    let color_space = parse_color_space(color_space_str.as_deref());
    let location = parse_location(&location_str)?;
    let compression = compression_str.as_deref().map(parse_compression).transpose()?;

    Ok(ImageBuilder {
        width, height, channels,
        sample_format, color_space, location, compression,
        fits_keywords: Vec::new(),
        properties: Vec::new(),
    })
}

// ── Attribute parsers ─────────────────────────────────────────────────────────

/// Parse `"width:height:channels"` geometry string.
fn parse_geometry(s: &str) -> Result<(u32, u32, u32), XisfError> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() < 2 {
        return Err(XisfError::UnsupportedGeometry(s.to_string()));
    }
    let w = parts[0].parse::<u32>().map_err(|_| XisfError::UnsupportedGeometry(s.to_string()))?;
    let h = parts[1].parse::<u32>().map_err(|_| XisfError::UnsupportedGeometry(s.to_string()))?;
    let c = if parts.len() >= 3 {
        parts[2].parse::<u32>().map_err(|_| XisfError::UnsupportedGeometry(s.to_string()))?
    } else { 1 };
    Ok((w, h, c))
}

/// Parse sampleFormat attribute, handling canonical and alternate names.
fn parse_sample_format(s: &str) -> Result<SampleFormat, XisfError> {
    // Handle alternate names per XISF spec and Python reference
    let canonical = match s {
        "Byte"   | "UInt8"   => "UInt8",
        "UInt16" | "UShort"  => "UInt16",
        "UInt32" | "UInt"    => "UInt32",
        "Float32"| "Float"   => "Float32",
        "Float64"| "Double"  => "Float64",
        other                => other,
    };
    match canonical {
        "UInt8"   => Ok(SampleFormat::UInt8),
        "UInt16"  => Ok(SampleFormat::UInt16),
        "UInt32"  => Ok(SampleFormat::UInt32),
        "Float32" => Ok(SampleFormat::Float32),
        "Float64" => Ok(SampleFormat::Float64),
        other     => Err(XisfError::UnsupportedSampleFormat(other.to_string())),
    }
}

/// Parse colorSpace attribute.
fn parse_color_space(s: Option<&str>) -> ColorSpace {
    match s {
        Some("Gray") | Some("Grayscale") | None => ColorSpace::Gray,
        Some("RGB")                              => ColorSpace::RGB,
        Some("CFA")                              => ColorSpace::CFA,
        Some(other)                              => ColorSpace::Unknown(other.to_string()),
    }
}

/// Parse location attribute: `"attachment:offset:size"`, `"inline:encoding"`, `"embedded"`.
fn parse_location(s: &str) -> Result<DataBlockLocation, XisfError> {
    let parts: Vec<&str> = s.splitn(3, ':').collect();
    match parts.first().copied() {
        Some("attachment") => {
            if parts.len() < 3 {
                return Err(XisfError::UnsupportedLocation(s.to_string()));
            }
            let offset = parts[1].parse::<u64>()
                .map_err(|_| XisfError::UnsupportedLocation(s.to_string()))?;
            let size   = parts[2].parse::<u64>()
                .map_err(|_| XisfError::UnsupportedLocation(s.to_string()))?;
            Ok(DataBlockLocation::Attachment { offset, size })
        }
        Some("inline") => {
            let encoding = parts.get(1).unwrap_or(&"base64").to_string();
            // Inline data is in the value attribute or element text — handled elsewhere
            Ok(DataBlockLocation::Inline { encoding, data: Vec::new() })
        }
        Some("embedded") => Ok(DataBlockLocation::Embedded { data: Vec::new() }),
        _ => Err(XisfError::UnsupportedLocation(s.to_string())),
    }
}

/// Parse compression attribute: `"codec:uncompressed_size"` or `"codec+sh:uncompressed_size:item_size"`.
fn parse_compression(s: &str) -> Result<CompressionInfo, XisfError> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() < 2 {
        return Err(XisfError::UnsupportedCodec(s.to_string()));
    }
    let codec_str = parts[0];
    let uncompressed_size = parts[1].parse::<u64>()
        .map_err(|_| XisfError::UnsupportedCodec(s.to_string()))?;
    let item_size = if parts.len() >= 3 {
        Some(parts[2].parse::<usize>()
            .map_err(|_| XisfError::UnsupportedCodec(s.to_string()))?)
    } else {
        None
    };

    // Codec string may be "lz4+sh", "lz4hc+sh", etc — strip "+sh" suffix
    let (codec_name, shuffled) = if codec_str.ends_with("+sh") {
        (&codec_str[..codec_str.len()-3], true)
    } else {
        (codec_str, false)
    };

    let codec = match codec_name {
        "lz4"   => Codec::Lz4,
        "lz4hc" => Codec::Lz4Hc,
        "zlib"  => Codec::Zlib,
        "zstd"  => Codec::Zstd,
        other   => return Err(XisfError::UnsupportedCodec(other.to_string())),
    };

    // item_size from suffix takes priority; fall back to shuffled flag
    let item_size = if shuffled && item_size.is_none() {
        Some(2usize) // default fallback — should always be present in valid files
    } else {
        item_size
    };

    Ok(CompressionInfo { codec, uncompressed_size, item_size })
}

// ── FITS keyword parsing ──────────────────────────────────────────────────────

fn parse_fits_keyword(e: &quick_xml::events::BytesStart) -> Result<Option<FitsKeyword>, XisfError> {
    let mut name    = String::new();
    let mut value   = String::new();
    let mut comment = String::new();

    for attr in e.attributes().flatten() {
        let key = std::str::from_utf8(attr.key.as_ref()).unwrap_or("").to_string();
        let val = attr.unescape_value().unwrap_or_default().to_string();
        match key.as_str() {
            "name"    => name    = val,
            "value"   => value   = val.trim_matches('\'').trim().to_string(),
            "comment" => comment = val,
            _ => {}
        }
    }

    if name.is_empty() { return Ok(None); }

    Ok(Some(FitsKeyword { name, value, comment }))
}

// ── Property parsing ──────────────────────────────────────────────────────────

fn parse_property(e: &quick_xml::events::BytesStart) -> Result<Option<XisfProperty>, XisfError> {
    let mut id     = String::new();
    let mut type_  = String::new();
    let mut value  = String::new();

    for attr in e.attributes().flatten() {
        let key = std::str::from_utf8(attr.key.as_ref()).unwrap_or("").to_string();
        let val = attr.unescape_value().unwrap_or_default().to_string();
        match key.as_str() {
            "id"    => id    = val,
            "type"  => type_ = val,
            "value" => value = val,
            _ => {}
        }
    }

    if id.is_empty() || type_.is_empty() { return Ok(None); }

    let prop_value = parse_property_value(&type_, &value)?;

    Ok(Some(XisfProperty { id, type_, value: prop_value }))
}

fn parse_property_value(type_: &str, value: &str) -> Result<PropertyValue, XisfError> {
    match type_ {
        "String" | "TimePoint" => Ok(PropertyValue::String(value.to_string())),
        "Boolean" => Ok(PropertyValue::Boolean(value == "true")),
        t if t.starts_with("Int") || t.starts_with("UInt") || t == "Byte" || t == "Short" => {
            value.parse::<i64>()
                .map(PropertyValue::Int64)
                .or_else(|_| value.parse::<u64>().map(PropertyValue::UInt64))
                .map_err(|_| XisfError::UnsupportedPropertyType(
                    format!("{}: {}", type_, value)
                ))
        }
        t if t.starts_with("Float") || t.starts_with("Double") => {
            value.parse::<f64>()
                .map(PropertyValue::Float64)
                .map_err(|_| XisfError::UnsupportedPropertyType(
                    format!("{}: {}", type_, value)
                ))
        }
        // Vectors and matrices — value is empty, data is in a child element or attachment
        // For now we store as a placeholder; full support deferred
        t if t.contains("Vector") || t.contains("Matrix") => {
            Ok(PropertyValue::String(format!("[{} data]", t)))
        }
        other => Err(XisfError::UnsupportedPropertyType(other.to_string())),
    }
}

// ── Data block reading ────────────────────────────────────────────────────────

fn read_data_block(path: &Path, location: &DataBlockLocation) -> Result<Vec<u8>, XisfError> {
    match location {
        DataBlockLocation::Attachment { offset, size } => {
            let mut f = File::open(path)?;
            f.seek(SeekFrom::Start(*offset))?;
            let mut data = vec![0u8; *size as usize];
            f.read_exact(&mut data)?;
            Ok(data)
        }
        DataBlockLocation::Inline { data, .. } => Ok(data.clone()),
        DataBlockLocation::Embedded { data }   => Ok(data.clone()),
    }
}

// ── Pixel deserialization ─────────────────────────────────────────────────────

/// Convert raw bytes to typed pixel data.
/// XISF stores pixels in planar format: all channel 0, then all channel 1, etc.
/// We reorder to interleaved (channel-last) to match Photyx's ImageBuffer convention.
fn deserialize_pixels(
    raw: &[u8],
    format: &SampleFormat,
    width: u32,
    height: u32,
    channels: u32,
) -> Result<PixelData, XisfError> {
    let pixel_count = (width * height) as usize;
    let ch = channels as usize;

    match format {
        SampleFormat::UInt8 => {
            // U8: zero-copy, just reinterpret and interleave
            let interleaved = planar_to_interleaved(raw, pixel_count, ch);
            Ok(PixelData::U8(interleaved))
        }
        SampleFormat::UInt16 => {
            // Zero-copy cast from &[u8] to &[u16] on little-endian systems
            let v: &[u16] = bytemuck::cast_slice(raw);
            let interleaved = planar_to_interleaved(v, pixel_count, ch);
            Ok(PixelData::U16(interleaved))
        }
        SampleFormat::UInt32 => {
            let v: &[u32] = bytemuck::cast_slice(raw);
            let interleaved = planar_to_interleaved(v, pixel_count, ch);
            Ok(PixelData::U32(interleaved))
        }
        SampleFormat::Float32 => {
            let v: &[f32] = bytemuck::cast_slice(raw);
            let interleaved = planar_to_interleaved(v, pixel_count, ch);
            Ok(PixelData::F32(interleaved))
        }
        SampleFormat::Float64 => {
            let v: &[f64] = bytemuck::cast_slice(raw);
            let interleaved = planar_to_interleaved(v, pixel_count, ch);
            Ok(PixelData::F64(interleaved))
        }
    }
}

/// Convert planar (channel-first) layout to interleaved (channel-last).
/// XISF: [ch0px0, ch0px1, ..., ch1px0, ch1px1, ...]
/// Photyx: [px0ch0, px0ch1, ..., px1ch0, px1ch1, ...]
fn planar_to_interleaved<T: Copy + Default>(
    planar: &[T],
    pixel_count: usize,
    channels: usize,
) -> Vec<T> {
    if channels == 1 {
        return planar.to_vec();
    }
    let mut out = vec![T::default(); pixel_count * channels];
    for ch in 0..channels {
        for px in 0..pixel_count {
            out[px * channels + ch] = planar[ch * pixel_count + px];
        }
    }
    out
}

fn planar_to_interleaved_u8(planar: &[u8], pixel_count: usize, channels: usize) -> Vec<u8> {
    planar_to_interleaved(planar, pixel_count, channels)
}
