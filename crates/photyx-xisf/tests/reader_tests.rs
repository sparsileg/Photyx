// Integration tests for the XISF reader

use photyx_xisf::{XisfReader, PixelData};

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
