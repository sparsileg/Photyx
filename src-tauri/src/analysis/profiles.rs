// analysis/profiles.rs — camera pixel size lookup table
// Keyed on INSTRUME keyword value via case-insensitive contains match.
// Add new cameras here as needed.

/// Returns the pixel size in micrometres for a known camera, matched
/// case-insensitively against the INSTRUME keyword value.
/// Returns None if the camera is not in the table.
pub fn pixel_size_um(instrume: &str) -> Option<f32> {
    let lower = instrume.to_lowercase();
    for (pattern, size) in CAMERA_TABLE {
        if lower.contains(pattern) {
            return Some(*size);
        }
    }
    None
}

/// Compute plate scale in arcseconds per pixel.
///
/// - `focal_length_mm` — telescope focal length from FOCALLEN keyword
/// - `pixel_size_um`   — sensor pixel size in micrometres
/// - `binning`         — binning factor from XBINNING keyword (1 = unbinned)
pub fn plate_scale(focal_length_mm: f32, pixel_size_um: f32, binning: u32) -> f32 {
    let effective_pixel_um = pixel_size_um * binning as f32;
    (effective_pixel_um / focal_length_mm) * 206.265
}

/// (pattern, pixel_size_um)
/// Pattern is matched case-insensitively as a substring of the INSTRUME value.
static CAMERA_TABLE: &[(&str, f32)] = &[
    ("533mc", 3.76),   // ZWO ASI533MC Pro / ZWO ASI533MC
    ("2600mc", 3.76),  // ZWO ASI2600MC Pro
    ("2600mm", 3.76),  // ZWO ASI2600MM Pro
    ("1600mc", 3.80),  // ZWO ASI1600MC Pro
    ("1600mm", 3.80),  // ZWO ASI1600MM Pro
    ("294mc", 4.63),   // ZWO ASI294MC Pro
    ("183mc", 2.40),   // ZWO ASI183MC Pro
    ("183mm", 2.40),   // ZWO ASI183MM Pro
    ("071mc", 4.78),   // ZWO ASI071MC Pro
    ("6200mc", 3.76),  // ZWO ASI6200MC Pro
    ("6200mm", 3.76),  // ZWO ASI6200MM Pro
    ("2400mc", 5.94),  // ZWO ASI2400MC Pro
    ("268mc", 3.76),   // ZWO ASI268MC Pro
    ("585mc", 2.90),   // ZWO ASI585MC
    ("imx571", 3.76),  // Generic IMX571 sensor
    ("imx294", 4.63),  // Generic IMX294 sensor
    ("imx183", 2.40),  // Generic IMX183 sensor
    ("imx455", 3.76),  // Generic IMX455 sensor
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_known_camera_exact() {
        assert_eq!(pixel_size_um("ZWO ASI533MC Pro"), Some(3.76));
    }

    #[test]
    fn test_known_camera_case_insensitive() {
        assert_eq!(pixel_size_um("zwo asi533mc pro"), Some(3.76));
        assert_eq!(pixel_size_um("ZWO ASI533MC"), Some(3.76));
    }

    #[test]
    fn test_unknown_camera() {
        assert_eq!(pixel_size_um("Canon EOS 6D"), None);
    }

    #[test]
    fn test_plate_scale_no_binning() {
        // AT115EDT + ASI533MC Pro: (3.76 / 805) * 206.265 ≈ 0.964 arcsec/px
        let ps = plate_scale(805.0, 3.76, 1);
        assert!((ps - 0.964).abs() < 0.001, "plate scale {} not near 0.964", ps);
    }

    #[test]
    fn test_plate_scale_binning_2() {
        let ps = plate_scale(805.0, 3.76, 2);
        assert!((ps - 1.928).abs() < 0.001, "plate scale {} not near 1.928", ps);
    }
}


// ----------------------------------------------------------------------
