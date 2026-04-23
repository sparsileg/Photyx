// plugins/keywords.rs — Keyword management built-in native plugins
// AddKeyword, DeleteKeyword, ModifyKeyword, CopyKeyword
// Spec §13.1, §6.3

use tracing::info;
use crate::plugin::{PhotonPlugin, ArgMap, ParamSpec, ParamType, PluginOutput, PluginError};
use crate::context::{AppContext, KeywordEntry};

// ── AddKeyword ────────────────────────────────────────────────────────────────

pub struct AddKeyword;

impl PhotonPlugin for AddKeyword {
    fn name(&self)        -> &str { "AddKeyword" }
    fn version(&self)     -> &str { "1.0" }
    fn description(&self) -> &str { "Adds or replaces a keyword on all buffered images" }

    fn parameters(&self) -> Vec<ParamSpec> {
        vec![
            ParamSpec {
                name:        "name".to_string(),
                param_type:  ParamType::String,
                required:    true,
                description: "Keyword name (uppercased automatically)".to_string(),
                default:     None,
            },
            ParamSpec {
                name:        "value".to_string(),
                param_type:  ParamType::String,
                required:    true,
                description: "Keyword value".to_string(),
                default:     None,
            },
            ParamSpec {
                name:        "comment".to_string(),
                param_type:  ParamType::String,
                required:    false,
                description: "Optional keyword comment".to_string(),
                default:     None,
            },
        ]
    }

    fn execute(&self, ctx: &mut AppContext, args: &ArgMap) -> Result<PluginOutput, PluginError> {
        let name = args.get("name")
            .ok_or_else(|| PluginError::missing_arg("name"))?
            .trim()
            .to_uppercase();
        let value = args.get("value")
            .ok_or_else(|| PluginError::missing_arg("value"))?
            .clone();
        let comment = args.get("comment").cloned();

        if name.is_empty() {
            return Err(PluginError::invalid_arg("name", "keyword name cannot be empty"));
        }

        let count = ctx.image_buffers.len();
        if count == 0 {
            return Err(PluginError::new("NO_IMAGES", "No images loaded."));
        }

        for buffer in ctx.image_buffers.values_mut() {
            buffer.keywords.insert(
                name.clone(),
                KeywordEntry::new(&name, &value, comment.as_deref()),
            );
        }

        info!("AddKeyword: {} = {} on {} image(s)", name, value, count);
        Ok(PluginOutput::Message(format!(
            "Keyword {} = {} added to {} image(s)", name, value, count
        )))
    }
}

// ── DeleteKeyword ─────────────────────────────────────────────────────────────

pub struct DeleteKeyword;

impl PhotonPlugin for DeleteKeyword {
    fn name(&self)        -> &str { "DeleteKeyword" }
    fn version(&self)     -> &str { "1.0" }
    fn description(&self) -> &str { "Removes a keyword from all buffered images" }

    fn parameters(&self) -> Vec<ParamSpec> {
        vec![
            ParamSpec {
                name:        "name".to_string(),
                param_type:  ParamType::String,
                required:    true,
                description: "Keyword name to delete".to_string(),
                default:     None,
            },
        ]
    }

    fn execute(&self, ctx: &mut AppContext, args: &ArgMap) -> Result<PluginOutput, PluginError> {
        let name = args.get("name")
            .ok_or_else(|| PluginError::missing_arg("name"))?
            .trim()
            .to_uppercase();

        if name.is_empty() {
            return Err(PluginError::invalid_arg("name", "keyword name cannot be empty"));
        }

        let count = ctx.image_buffers.len();
        if count == 0 {
            return Err(PluginError::new("NO_IMAGES", "No images loaded."));
        }

        let mut removed = 0usize;
        for buffer in ctx.image_buffers.values_mut() {
            if buffer.keywords.remove(&name).is_some() {
                removed += 1;
            }
        }

        info!("DeleteKeyword: {} removed from {}/{} image(s)", name, removed, count);
        Ok(PluginOutput::Message(format!(
            "Keyword {} removed from {} image(s)", name, removed
        )))
    }
}

// ── ModifyKeyword ─────────────────────────────────────────────────────────────

pub struct ModifyKeyword;

impl PhotonPlugin for ModifyKeyword {
    fn name(&self)        -> &str { "ModifyKeyword" }
    fn version(&self)     -> &str { "1.0" }
    fn description(&self) -> &str { "Changes the value of an existing keyword on all buffered images" }

    fn parameters(&self) -> Vec<ParamSpec> {
        vec![
            ParamSpec {
                name:        "name".to_string(),
                param_type:  ParamType::String,
                required:    true,
                description: "Keyword name to modify".to_string(),
                default:     None,
            },
            ParamSpec {
                name:        "value".to_string(),
                param_type:  ParamType::String,
                required:    true,
                description: "New keyword value".to_string(),
                default:     None,
            },
            ParamSpec {
                name:        "comment".to_string(),
                param_type:  ParamType::String,
                required:    false,
                description: "Optional keyword comment (max ~65 chars for FITS)".to_string(),
                default:     None,
            },
        ]
    }

    fn execute(&self, ctx: &mut AppContext, args: &ArgMap) -> Result<PluginOutput, PluginError> {
        let name = args.get("name")
            .ok_or_else(|| PluginError::missing_arg("name"))?
            .trim()
            .to_uppercase();
        let value = args.get("value")
            .ok_or_else(|| PluginError::missing_arg("value"))?
            .clone();
        let comment = args.get("comment").cloned();

        if name.is_empty() {
            return Err(PluginError::invalid_arg("name", "keyword name cannot be empty"));
        }

        let count = ctx.image_buffers.len();
        if count == 0 {
            return Err(PluginError::new("NO_IMAGES", "No images loaded."));
        }

        let mut modified = 0usize;
        for buffer in ctx.image_buffers.values_mut() {
            if let Some(kw) = buffer.keywords.get_mut(&name) {
                kw.value = value.clone();
                if let Some(ref c) = comment {
                    kw.comment = Some(c.clone());
                }
                modified += 1;
            }
        }

        if modified == 0 {
            return Err(PluginError::new(
                "NOT_FOUND",
                &format!("Keyword '{}' not found in any loaded image.", name),
            ));
        }

        info!("ModifyKeyword: {} = {} on {}/{} image(s)", name, value, modified, count);
        Ok(PluginOutput::Message(format!(
            "Keyword {} updated to '{}' on {} image(s)", name, value, modified
        )))
    }
}

// ── CopyKeyword ───────────────────────────────────────────────────────────────

pub struct CopyKeyword;

impl PhotonPlugin for CopyKeyword {
    fn name(&self)        -> &str { "CopyKeyword" }
    fn version(&self)     -> &str { "1.0" }
    fn description(&self) -> &str { "Copies a keyword value to a new keyword name on all buffered images" }

    fn parameters(&self) -> Vec<ParamSpec> {
        vec![
            ParamSpec {
                name:        "from".to_string(),
                param_type:  ParamType::String,
                required:    true,
                description: "Source keyword name".to_string(),
                default:     None,
            },
            ParamSpec {
                name:        "to".to_string(),
                param_type:  ParamType::String,
                required:    true,
                description: "Destination keyword name".to_string(),
                default:     None,
            },
        ]
    }

    fn execute(&self, ctx: &mut AppContext, args: &ArgMap) -> Result<PluginOutput, PluginError> {
        let from = args.get("from")
            .ok_or_else(|| PluginError::missing_arg("from"))?
            .trim()
            .to_uppercase();
        let to = args.get("to")
            .ok_or_else(|| PluginError::missing_arg("to"))?
            .trim()
            .to_uppercase();

        if from.is_empty() {
            return Err(PluginError::invalid_arg("from", "keyword name cannot be empty"));
        }
        if to.is_empty() {
            return Err(PluginError::invalid_arg("to", "keyword name cannot be empty"));
        }

        let count = ctx.image_buffers.len();
        if count == 0 {
            return Err(PluginError::new("NO_IMAGES", "No images loaded."));
        }

        let mut copied = 0usize;
        for buffer in ctx.image_buffers.values_mut() {
            if let Some(src) = buffer.keywords.get(&from).cloned() {
                buffer.keywords.insert(
                    to.clone(),
                    KeywordEntry::new(&to, &src.value, src.comment.as_deref()),
                );
                copied += 1;
            }
        }

        if copied == 0 {
            return Err(PluginError::new(
                "NOT_FOUND",
                &format!("Keyword '{}' not found in any loaded image.", from),
            ));
        }

        info!("CopyKeyword: {} → {} on {}/{} image(s)", from, to, copied, count);
        Ok(PluginOutput::Message(format!(
            "Keyword {} copied to {} on {} image(s)", from, to, copied
        )))
    }
}

// ----------------------------------------------------------------------
