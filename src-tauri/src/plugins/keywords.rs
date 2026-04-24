// plugins/keywords.rs — Keyword management built-in native plugins
// AddKeyword, DeleteKeyword, ModifyKeyword, CopyKeyword
// Spec §13.1, §6.3

use tracing::info;
use crate::plugin::{PhotonPlugin, ArgMap, ParamSpec, ParamType, PluginOutput, PluginError};
use crate::context::{AppContext, KeywordEntry};

// ── Scope helper ──────────────────────────────────────────────────────────────

/// Parse the optional `scope` argument.
/// Valid values: "all" (default) or "current".
fn parse_scope(args: &ArgMap) -> Result<bool, PluginError> {
    match args.get("scope").map(|s| s.to_lowercase()).as_deref() {
        None | Some("all")     => Ok(false),  // false = all images
        Some("current")        => Ok(true),   // true  = current frame only
        Some(other) => Err(PluginError::new(
            "INVALID_ARG",
            &format!("Invalid scope '{}': must be 'all' or 'current'", other),
        )),
    }
}

// ── AddKeyword ────────────────────────────────────────────────────────────────

pub struct AddKeyword;

impl PhotonPlugin for AddKeyword {
    fn name(&self)        -> &str { "AddKeyword" }
    fn version(&self)     -> &str { "1.0" }
    fn description(&self) -> &str { "Adds or replaces a keyword on loaded images (scope=all|current, default: all)" }

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
            ParamSpec {
                name:        "scope".to_string(),
                param_type:  ParamType::String,
                required:    false,
                description: "Apply to 'all' images or 'current' frame only (default: all)".to_string(),
                default:     Some("all".to_string()),
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
        let current_only = parse_scope(args)?;

        if name.is_empty() {
            return Err(PluginError::invalid_arg("name", "keyword name cannot be empty"));
        }
        if ctx.image_buffers.is_empty() {
            return Err(PluginError::new("NO_IMAGES", "No images loaded."));
        }

        let count = if current_only {
            // Apply to current frame only
            let path = ctx.file_list.get(ctx.current_frame)
                .cloned()
                .ok_or_else(|| PluginError::new("NO_FRAME", "No current frame"))?;
            if let Some(buffer) = ctx.image_buffers.get_mut(&path) {
                buffer.keywords.insert(
                    name.clone(),
                    KeywordEntry::new(&name, &value, comment.as_deref()),
                );
                1
            } else { 0 }
        } else {
            // Apply to all images
            let n = ctx.image_buffers.len();
            for buffer in ctx.image_buffers.values_mut() {
                buffer.keywords.insert(
                    name.clone(),
                    KeywordEntry::new(&name, &value, comment.as_deref()),
                );
            }
            n
        };

        let scope_label = if current_only { "current frame" } else { &format!("{} image(s)", count) };
        info!("AddKeyword: {} = {} on {}", name, value, scope_label);
        Ok(PluginOutput::Message(format!(
            "Keyword {} = {} added to {}", name, value, scope_label
        )))
    }
}

// ── DeleteKeyword ─────────────────────────────────────────────────────────────

pub struct DeleteKeyword;

impl PhotonPlugin for DeleteKeyword {
    fn name(&self)        -> &str { "DeleteKeyword" }
    fn version(&self)     -> &str { "1.0" }
    fn description(&self) -> &str { "Removes a keyword from loaded images (scope=all|current, default: all)" }

    fn parameters(&self) -> Vec<ParamSpec> {
        vec![
            ParamSpec {
                name:        "name".to_string(),
                param_type:  ParamType::String,
                required:    true,
                description: "Keyword name to delete".to_string(),
                default:     None,
            },
            ParamSpec {
                name:        "scope".to_string(),
                param_type:  ParamType::String,
                required:    false,
                description: "Apply to 'all' images or 'current' frame only (default: all)".to_string(),
                default:     Some("all".to_string()),
            },
        ]
    }

    fn execute(&self, ctx: &mut AppContext, args: &ArgMap) -> Result<PluginOutput, PluginError> {
        let name = args.get("name")
            .ok_or_else(|| PluginError::missing_arg("name"))?
            .trim()
            .to_uppercase();
        let current_only = parse_scope(args)?;

        if name.is_empty() {
            return Err(PluginError::invalid_arg("name", "keyword name cannot be empty"));
        }
        if ctx.image_buffers.is_empty() {
            return Err(PluginError::new("NO_IMAGES", "No images loaded."));
        }

        let removed = if current_only {
            let path = ctx.file_list.get(ctx.current_frame)
                .cloned()
                .ok_or_else(|| PluginError::new("NO_FRAME", "No current frame"))?;
            if let Some(buffer) = ctx.image_buffers.get_mut(&path) {
                if buffer.keywords.remove(&name).is_some() { 1 } else { 0 }
            } else { 0 }
        } else {
            let mut n = 0usize;
            for buffer in ctx.image_buffers.values_mut() {
                if buffer.keywords.remove(&name).is_some() { n += 1; }
            }
            n
        };

        let scope_label = if current_only { "current frame".to_string() } else { format!("{} image(s)", removed) };
        info!("DeleteKeyword: {} removed from {}", name, scope_label);
        Ok(PluginOutput::Message(format!(
            "Keyword {} removed from {}", name, scope_label
        )))
    }
}

// ── ModifyKeyword ─────────────────────────────────────────────────────────────

pub struct ModifyKeyword;

impl PhotonPlugin for ModifyKeyword {
    fn name(&self)        -> &str { "ModifyKeyword" }
    fn version(&self)     -> &str { "1.0" }
    fn description(&self) -> &str { "Changes the value of an existing keyword (scope=all|current, default: all)" }

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
                description: "Optional keyword comment".to_string(),
                default:     None,
            },
            ParamSpec {
                name:        "scope".to_string(),
                param_type:  ParamType::String,
                required:    false,
                description: "Apply to 'all' images or 'current' frame only (default: all)".to_string(),
                default:     Some("all".to_string()),
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
        let current_only = parse_scope(args)?;

        if name.is_empty() {
            return Err(PluginError::invalid_arg("name", "keyword name cannot be empty"));
        }
        if ctx.image_buffers.is_empty() {
            return Err(PluginError::new("NO_IMAGES", "No images loaded."));
        }

        let modified = if current_only {
            let path = ctx.file_list.get(ctx.current_frame)
                .cloned()
                .ok_or_else(|| PluginError::new("NO_FRAME", "No current frame"))?;
            if let Some(buffer) = ctx.image_buffers.get_mut(&path) {
                if let Some(kw) = buffer.keywords.get_mut(&name) {
                    kw.value = value.clone();
                    if let Some(ref c) = comment { kw.comment = Some(c.clone()); }
                    1
                } else { 0 }
            } else { 0 }
        } else {
            let mut n = 0usize;
            for buffer in ctx.image_buffers.values_mut() {
                if let Some(kw) = buffer.keywords.get_mut(&name) {
                    kw.value = value.clone();
                    if let Some(ref c) = comment { kw.comment = Some(c.clone()); }
                    n += 1;
                }
            }
            n
        };

        if modified == 0 {
            return Err(PluginError::new(
                "NOT_FOUND",
                &format!("Keyword '{}' not found.", name),
            ));
        }

        let scope_label = if current_only { "current frame".to_string() } else { format!("{} image(s)", modified) };
        info!("ModifyKeyword: {} = {} on {}", name, value, scope_label);
        Ok(PluginOutput::Message(format!(
            "Keyword {} updated to '{}' on {}", name, value, scope_label
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
        if ctx.image_buffers.is_empty() {
            return Err(PluginError::new("NO_IMAGES", "No images loaded."));
        }

        let mut copied = 0usize;
        let count = ctx.image_buffers.len();
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
