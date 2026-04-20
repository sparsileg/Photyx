// plugin/mod.rs — PhotonPlugin trait, types, and plugin infrastructure
// Spec §6.4

pub mod registry;

use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use crate::context::AppContext;

// ── Argument map ─────────────────────────────────────────────────────────────

pub type ArgMap = HashMap<String, String>;

// ── Parameter specification ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ParamType {
    String,
    Integer,
    Float,
    Boolean,
    Path,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParamSpec {
    pub name:        String,
    pub param_type:  ParamType,
    pub required:    bool,
    pub description: String,
    pub default:     Option<String>,
}

// ── Plugin output ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PluginOutput {
    Success,
    Message(String),
    Value(String),
    Values(Vec<String>),
}

// ── Plugin error ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginError {
    pub code:    String,
    pub message: String,
}

impl PluginError {
    pub fn new(code: &str, message: &str) -> Self {
        Self {
            code:    code.to_string(),
            message: message.to_string(),
        }
    }

    pub fn missing_arg(name: &str) -> Self {
        Self::new("MISSING_ARG", &format!("Missing required argument: '{}'", name))
    }

    pub fn invalid_arg(name: &str, reason: &str) -> Self {
        Self::new("INVALID_ARG", &format!("Invalid argument '{}': {}", name, reason))
    }
}

impl std::fmt::Display for PluginError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

// ── PhotonPlugin trait ────────────────────────────────────────────────────────
// Spec §6.4 — all plugins implement this trait

pub trait PhotonPlugin: Send + Sync {
    fn name(&self)        -> &str;
    fn version(&self)     -> &str;
    fn description(&self) -> &str;
    fn parameters(&self)  -> Vec<ParamSpec>;
    fn execute(
        &self,
        ctx:  &mut AppContext,
        args: &ArgMap,
    ) -> Result<PluginOutput, PluginError>;
}
