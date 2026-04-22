// Integration tests for the XISF reader

use photyx_xisf::{XisfReader, XisfWriter, WriteOptions, Codec, PixelData};

const TEST_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data");

#[test]
fn test_open_and_metadata() {
    let files: Vec<_> = std::fs::read_dir(TEST_DIR)
        .expect("tests/data directory not found")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "xisf").unwrap_or(false))
        .collect();

    assert!(!files.is_empty(), "No .xisf files found in tests/data/");

    for entry in &files {
        let path = entry.path();
        println!("Testing: {}", path.display());

        let reader = XisfReader::open(&path)
            .unwrap_or_else(|e| panic!("Failed to open {}: {}", path.display(), e));

        assert!(reader.image_count() > 0, "No images found in {}", path.display());

        let meta = reader.image_meta(0)
            .unwrap_or_else(|e| panic!("Failed to get metadata: {}", e));

        println!("  {}x{}x{} {:?} {:?}",
            meta.width, meta.height, meta.channels,
            meta.sample_format, meta.color_space);
        println!("  FITS keywords: {}", meta.fits_keywords.len());
        println!("  Properties: {}", meta.properties.len());

        assert!(meta.width > 0);
        assert!(meta.height > 0);
        assert!(meta.channels > 0);
    }
}

#[test]
fn test_read_pixels() {
    let files: Vec<_> = std::fs::read_dir(TEST_DIR)
        .expect("tests/data directory not found")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "xisf").unwrap_or(false))
        .collect();

    for entry in &files {
        let path = entry.path();
        println!("Reading pixels: {}", path.display());

        let reader = XisfReader::open(&path).unwrap();
        let image = reader.read_image(0)
            .unwrap_or_else(|e| panic!("Failed to read image from {}: {}", path.display(), e));

        let expected_pixels = (image.width * image.height * image.channels) as usize;

        let actual_pixels = match &image.pixels {
            PixelData::U8(v)  => v.len(),
            PixelData::U16(v) => v.len(),
            PixelData::U32(v) => v.len(),
            PixelData::F32(v) => v.len(),
            PixelData::F64(v) => v.len(),
        };

        assert_eq!(actual_pixels, expected_pixels,
            "Pixel count mismatch for {}: expected {}, got {}",
            path.display(), expected_pixels, actual_pixels);

        println!("  {} pixels OK", actual_pixels);
    }
}

#[test]
#[test]
fn test_round_trip() {
    use photyx_xisf::{XisfWriter, WriteOptions, Codec};
    use std::path::PathBuf;

    let files: Vec<_> = std::fs::read_dir(TEST_DIR)
        .expect("tests/data directory not found")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "xisf").unwrap_or(false))
        .collect();

    for entry in &files {
        let path = entry.path();
        println!("Round-trip: {}", path.display());

        // Read original
        let reader = XisfReader::open(&path).unwrap();
        let original = reader.read_image(0).unwrap();
        let original_meta = reader.image_meta(0).unwrap();

        // Write to temp file — uncompressed first
        let out_path = PathBuf::from(TEST_DIR).join("_roundtrip_test.xisf");
        XisfWriter::write(&out_path, &original, &WriteOptions {
            codec: Codec::None,
            shuffle: false,
            creator_app: "photyx-xisf test".to_string(),
            block_alignment: 4096,
        }).unwrap_or_else(|e| panic!("Write failed for {}: {}", path.display(), e));

        // Read back
        let reader2 = XisfReader::open(&out_path).unwrap();
        let roundtripped = reader2.read_image(0).unwrap();
        let meta2 = reader2.image_meta(0).unwrap();

        // Verify dimensions
        assert_eq!(roundtripped.width,    original.width);
        assert_eq!(roundtripped.height,   original.height);
        assert_eq!(roundtripped.channels, original.channels);

        // Verify pixel count
        let orig_len = match &original.pixels {
            PixelData::U16(v) => v.len(),
            PixelData::F32(v) => v.len(),
            PixelData::U8(v)  => v.len(),
            PixelData::U32(v) => v.len(),
            PixelData::F64(v) => v.len(),
        };
        let rt_len = match &roundtripped.pixels {
            PixelData::U16(v) => v.len(),
            PixelData::F32(v) => v.len(),
            PixelData::U8(v)  => v.len(),
            PixelData::U32(v) => v.len(),
            PixelData::F64(v) => v.len(),
        };
        assert_eq!(orig_len, rt_len, "Pixel count mismatch after round-trip");

        // Verify FITS keyword count preserved
        // (COMMENT keywords are skipped on write so allow some reduction)
        let comment_count = original_meta.fits_keywords.iter()
            .filter(|k| k.name == "COMMENT").count();
        assert!(meta2.fits_keywords.len() >= original_meta.fits_keywords.len() - comment_count,
            "Lost FITS keywords in round-trip");

        println!("  {}x{}x{} — {} pixels — {} keywords OK",
            roundtripped.width, roundtripped.height, roundtripped.channels,
            rt_len, meta2.fits_keywords.len());

        // Clean up
        std::fs::remove_file(&out_path).ok();
    }
}

#[test]
fn test_round_trip_compressed() {
    use photyx_xisf::{XisfWriter, WriteOptions, Codec};
    use std::path::PathBuf;

    let files: Vec<_> = std::fs::read_dir(TEST_DIR)
        .expect("tests/data directory not found")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "xisf").unwrap_or(false))
        .collect();

    for entry in &files {
        let path = entry.path();
        println!("Round-trip compressed: {}", path.display());

        let reader = XisfReader::open(&path).unwrap();
        let original = reader.read_image(0).unwrap();

        let out_path = PathBuf::from(TEST_DIR).join("_roundtrip_compressed_test.xisf");
        XisfWriter::write(&out_path, &original, &WriteOptions::default())
            .unwrap_or_else(|e| panic!("Compressed write failed: {}", e));

        let reader2 = XisfReader::open(&out_path).unwrap();
        let roundtripped = reader2.read_image(0).unwrap();

        assert_eq!(roundtripped.width,    original.width);
        assert_eq!(roundtripped.height,   original.height);
        assert_eq!(roundtripped.channels, original.channels);

        // Verify pixel values are identical
        match (&original.pixels, &roundtripped.pixels) {
            (PixelData::U16(a), PixelData::U16(b)) => {
                assert_eq!(a, b, "Pixel values changed after compressed round-trip");
            }
            (PixelData::F32(a), PixelData::F32(b)) => {
                assert_eq!(a, b, "Pixel values changed after compressed round-trip");
            }
            _ => {}
        }

        let out_size = std::fs::metadata(&out_path).map(|m| m.len()).unwrap_or(0);
        println!("  Compressed output: {} bytes", out_size);

        std::fs::remove_file(&out_path).ok();
    }
}

#[test]
fn test_fits_keywords() {
    let files: Vec<_> = std::fs::read_dir(TEST_DIR)
        .expect("tests/data directory not found")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "xisf").unwrap_or(false))
        .collect();

    for entry in &files {
        let path = entry.path();
        let reader = XisfReader::open(&path).unwrap();
        let meta = reader.image_meta(0).unwrap();

        println!("Keywords in {}:", path.file_name().unwrap().to_string_lossy());
        for kw in &meta.fits_keywords {
            println!("  {} = '{}' / {}", kw.name, kw.value, kw.comment);
        }
    }
}
