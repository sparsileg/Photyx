// compress.rs — Compression and byte-shuffling for XISF data blocks

use crate::error::XisfError;
use crate::types::{Codec, CompressionInfo};

/// Byte-unshuffle: reverses the byte-shuffle transform applied before compression.
/// Data is treated as a 2D array of (item_count × item_size) bytes, transposed.
pub fn unshuffle(data: &[u8], item_size: usize) -> Vec<u8> {
    let count = data.len() / item_size;
    let mut out = vec![0u8; data.len()];
    for i in 0..item_size {
        for j in 0..count {
            out[j * item_size + i] = data[i * count + j];
        }
    }
    out
}

/// Byte-shuffle: rearranges bytes before compression for better ratios.
/// Data is treated as a 2D array of (count × item_size) bytes, transposed.
pub fn shuffle(data: &[u8], item_size: usize) -> Vec<u8> {
    let count = data.len() / item_size;
    let mut out = vec![0u8; data.len()];
    for i in 0..count {
        for j in 0..item_size {
            out[j * count + i] = data[i * item_size + j];
        }
    }
    out
}

/// Decompress a data block using the codec and parameters in `info`.
pub fn decompress(data: &[u8], info: &CompressionInfo) -> Result<Vec<u8>, XisfError> {
    // Step 1: decompress
    let mut decompressed = match info.codec {
        Codec::Lz4 | Codec::Lz4Hc => {
            lz4_flex::decompress(data, info.uncompressed_size as usize)
                .map_err(|e| XisfError::Decompression(e.to_string()))?
        }
        Codec::Zstd => {
            zstd::decode_all(data)
                .map_err(|e| XisfError::Decompression(e.to_string()))?
        }
        Codec::Zlib => {
            use std::io::Read;
            let mut decoder = flate2::read::ZlibDecoder::new(data);
            let mut out = Vec::with_capacity(info.uncompressed_size as usize);
            decoder.read_to_end(&mut out)
                .map_err(|e| XisfError::Decompression(e.to_string()))?;
            out
        }
        Codec::None => {
            return Err(XisfError::Decompression(
                "decompress() called with Codec::None".to_string()
            ));
        }
    };

    // Step 2: unshuffle if byte-shuffling was applied
    if let Some(item_size) = info.item_size {
        decompressed = unshuffle(&decompressed, item_size);
    }

    Ok(decompressed)
}

/// Compress a data block using the codec in `options`.
/// Returns None if compression did not reduce the data size (caller should store uncompressed).
pub fn compress(
    data: &[u8],
    codec: &Codec,
    shuffle_items: bool,
    item_size: usize,
) -> Result<Option<Vec<u8>>, XisfError> {
    let input = if shuffle_items {
        shuffle(data, item_size)
    } else {
        data.to_vec()
    };

    let compressed = match codec {
        Codec::Lz4 => lz4_flex::compress(&input),
        Codec::Lz4Hc => lz4_flex::compress(&input), // lz4_flex uses HC by default
        Codec::Zstd => {
            zstd::encode_all(input.as_slice(), 3)
                .map_err(|e| XisfError::Decompression(e.to_string()))?
        }
        Codec::Zlib => {
            use std::io::Write;
            let mut encoder = flate2::write::ZlibEncoder::new(
                Vec::new(),
                flate2::Compression::default(),
            );
            encoder.write_all(&input)
                .map_err(|e| XisfError::Decompression(e.to_string()))?;
            encoder.finish()
                .map_err(|e| XisfError::Decompression(e.to_string()))?
        }
        Codec::None => return Ok(None),
    };

    // Only use compression if it actually reduces size
    if compressed.len() < data.len() {
        Ok(Some(compressed))
    } else {
        Ok(None)
    }
}
