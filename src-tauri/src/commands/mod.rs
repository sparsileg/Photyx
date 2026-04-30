// commands/mod.rs — Declares all Tauri command handler submodules.
// lib.rs references commands using fully qualified paths e.g. commands::analysis::get_analysis_results

pub mod analysis;
pub mod backup;
pub mod display;
pub mod logging;
pub mod macros;
pub mod preferences;
pub mod session;

// ----------------------------------------------------------------------
