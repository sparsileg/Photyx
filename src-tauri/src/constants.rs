// src-tauri/src/constants.rs — Application-wide constants
// Spec §4.2

/// Threshold profiles that cannot be deleted by the user.
/// These are seeded automatically on first launch via seed_defaults().
pub const PROTECTED_THRESHOLD_PROFILES: &[&str] = &[
    "Default",
    "Project",
    "Session",
];

// ----------------------------------------------------------------------
