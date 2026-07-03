// src-tauri/src/constants.rs — Application-wide constants
// Spec §4.2
/// Threshold profiles that cannot be deleted by the user.
/// These are seeded automatically on first launch via seed_defaults().
pub const PROTECTED_THRESHOLD_PROFILES: &[&str] = &[
    "Default",
    "Project",
];

/// Forces GDK_BACKEND=x11 on Linux at startup, working around a
/// WebKitGTK/Wayland compositor interaction that causes sustained high
/// idle CPU usage (observed: one thread pegged ~50-55%, dropped to ~18%
/// under XWayland). No-op on native X11 sessions and on Windows/macOS.
/// Issue 57.
pub const FORCE_X11_ON_LINUX: bool = true;


// ----------------------------------------------------------------------
