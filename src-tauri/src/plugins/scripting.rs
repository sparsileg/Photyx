// plugins/scripting.rs — Scripting and utility plugins
// Covers: GetKeyword, MoveFile, Print, Assert, CountFiles
// Spec §7.8, §7.12

use std::path::Path;
use std::sync::Arc;

use crate::context::AppContext;
use crate::plugin::{ArgMap, ParamSpec, ParamType, PhotonPlugin, PluginError, PluginOutput};

// ── GetKeyword ────────────────────────────────────────────────────────────────

/// Retrieves a keyword value from the current frame and stores it as a variable.
/// Usage: GetKeyword name=PXFLAG
/// Result is stored in $PXFLAG (uppercase of the name arg) in AppContext.variables.
pub struct GetKeyword;

impl PhotonPlugin for GetKeyword {
    fn name(&self)        -> &str { "GetKeyword" }
    fn version(&self)     -> &str { "1.0.0" }
    fn description(&self) -> &str { "Retrieves a FITS keyword value from the current frame into a script variable" }

    fn parameters(&self) -> Vec<ParamSpec> {
        vec![
            ParamSpec {
                name:        "name".to_string(),
                param_type:  ParamType::String,
                required:    true,
                description: "Keyword name to retrieve".to_string(),
                default:     None,
            },
        ]
    }

    fn execute(&self, ctx: &mut AppContext, args: &ArgMap) -> Result<PluginOutput, PluginError> {
        let name = args.get("name")
            .ok_or_else(|| PluginError::missing_arg("name"))?
            .trim()
            .to_uppercase();

        let file_path = ctx.file_list
            .get(ctx.current_frame)
            .cloned()
            .ok_or_else(|| PluginError::new("NO_FRAME", "GetKeyword: no current frame"))?;

        let buffer = ctx.image_buffers.get(&file_path)
            .ok_or_else(|| PluginError::new("NO_BUFFER", "GetKeyword: no image buffer for current frame"))?;

        let entry = buffer.keywords.iter()
            .find(|(k, _)| k.to_uppercase() == name)
            .map(|(_, v)| v)
            .ok_or_else(|| PluginError::new(
                "NOT_FOUND",
                &format!("GetKeyword: keyword '{}' not found in current frame", name),
            ))?;

        let value = entry.value.trim().to_string();

        ctx.variables.insert(name.clone(), value.clone());

        Ok(PluginOutput::Value(value))
    }
}

// ── MoveFile ──────────────────────────────────────────────────────────────────

/// Moves a file to a destination directory.
/// Usage: MoveFile destination="D:/Rejects"
///        MoveFile source="$NEW_FILE" destination="D:/Heatmaps"
pub struct MoveFile;

impl PhotonPlugin for MoveFile {
    fn name(&self)        -> &str { "MoveFile" }
    fn version(&self)     -> &str { "1.0.0" }
    fn description(&self) -> &str { "Moves a file to a destination directory. Uses current frame if source= is not specified." }

    fn parameters(&self) -> Vec<ParamSpec> {
        vec![
            ParamSpec {
                name:        "source".to_string(),
                param_type:  ParamType::Path,
                required:    false,
                description: "Source file path (default: current frame)".to_string(),
                default:     None,
            },
            ParamSpec {
                name:        "destination".to_string(),
                param_type:  ParamType::Path,
                required:    true,
                description: "Destination directory path".to_string(),
                default:     None,
            },
        ]
    }

    fn execute(&self, ctx: &mut AppContext, args: &ArgMap) -> Result<PluginOutput, PluginError> {
        let destination = args.get("destination")
            .ok_or_else(|| PluginError::missing_arg("destination"))?;

        let dest_dir = crate::utils::resolve_path(
            destination,
            ctx.active_directory.as_deref(),
        );

        // Use explicit source if provided, otherwise use current frame
        let src_path = if let Some(source) = args.get("source") {
            crate::utils::resolve_path(source, ctx.active_directory.as_deref())
        } else {
            ctx.file_list
                .get(ctx.current_frame)
                .cloned()
                .ok_or_else(|| PluginError::new("NO_FRAME", "MoveFile: no current frame"))?
        };

        let src = Path::new(&src_path);
        let filename = src.file_name()
            .ok_or_else(|| PluginError::new("BAD_PATH", "MoveFile: cannot determine filename"))?;

        std::fs::create_dir_all(&dest_dir)
            .map_err(|e| PluginError::new(
                "IO_ERROR",
                &format!("MoveFile: cannot create directory '{}': {}", dest_dir, e),
            ))?;

        let dest_path = Path::new(&dest_dir).join(filename);

        std::fs::rename(&src_path, &dest_path)
            .map_err(|e| PluginError::new(
                "IO_ERROR",
                &format!("MoveFile: cannot move '{}' to '{}': {}", src_path, dest_path.display(), e),
            ))?;

        let dest_str = dest_path.display().to_string();

        // Only remove from session caches if it was a session file
        ctx.file_list.retain(|f| f != &src_path);
        ctx.image_buffers.remove(&src_path);
        ctx.display_cache.remove(&src_path);
        ctx.full_res_cache.remove(&src_path);
        ctx.blink_cache_12.remove(&src_path);
        ctx.blink_cache_25.remove(&src_path);

        if ctx.current_frame >= ctx.file_list.len() && !ctx.file_list.is_empty() {
            ctx.current_frame = ctx.file_list.len() - 1;
        }

        tracing::info!("MoveFile: '{}' -> '{}'", src_path, dest_str);
        Ok(PluginOutput::Message(format!("Moved '{}' to '{}'", src_path, dest_str)))
    }
}

// ── CopyFile ──────────────────────────────────────────────────────────────────

/// Copies a file to a destination directory.
/// Usage: CopyFile destination="D:/Backups"
///        CopyFile source="$NEW_FILE" destination="D:/Heatmaps"
pub struct CopyFile;

impl PhotonPlugin for CopyFile {
    fn name(&self)        -> &str { "CopyFile" }
    fn version(&self)     -> &str { "1.0.0" }
    fn description(&self) -> &str { "Copies a file to a destination directory. Uses current frame if source= is not specified." }

    fn parameters(&self) -> Vec<ParamSpec> {
        vec![
            ParamSpec {
                name:        "source".to_string(),
                param_type:  ParamType::Path,
                required:    false,
                description: "Source file path (default: current frame)".to_string(),
                default:     None,
            },
            ParamSpec {
                name:        "destination".to_string(),
                param_type:  ParamType::Path,
                required:    true,
                description: "Destination directory path".to_string(),
                default:     None,
            },
        ]
    }

    fn execute(&self, ctx: &mut AppContext, args: &ArgMap) -> Result<PluginOutput, PluginError> {
        let destination = args.get("destination")
            .ok_or_else(|| PluginError::missing_arg("destination"))?;

        let dest_dir = crate::utils::resolve_path(
            destination,
            ctx.active_directory.as_deref(),
        );

        let src_path = if let Some(source) = args.get("source") {
            crate::utils::resolve_path(source, ctx.active_directory.as_deref())
        } else {
            ctx.file_list
                .get(ctx.current_frame)
                .cloned()
                .ok_or_else(|| PluginError::new("NO_FRAME", "CopyFile: no current frame"))?
        };

        let src = Path::new(&src_path);
        let filename = src.file_name()
            .ok_or_else(|| PluginError::new("BAD_PATH", "CopyFile: cannot determine filename"))?;

        std::fs::create_dir_all(&dest_dir)
            .map_err(|e| PluginError::new(
                "IO_ERROR",
                &format!("CopyFile: cannot create directory '{}': {}", dest_dir, e),
            ))?;

        let dest_path = Path::new(&dest_dir).join(filename);

        std::fs::copy(&src_path, &dest_path)
            .map_err(|e| PluginError::new(
                "IO_ERROR",
                &format!("CopyFile: cannot copy '{}' to '{}': {}", src_path, dest_path.display(), e),
            ))?;

        let dest_str = dest_path.display().to_string();
        ctx.variables.insert("NEW_FILE".to_string(), dest_str.clone());

        tracing::info!("CopyFile: '{}' -> '{}'", src_path, dest_str);
        Ok(PluginOutput::Message(format!("Copied '{}' to '{}'", src_path, dest_str)))
    }
}

// ── Print ─────────────────────────────────────────────────────────────────────

/// Outputs a literal message to the pcode console.
/// Usage: Print message="Hello, world!"
pub struct Print;

impl PhotonPlugin for Print {
    fn name(&self)        -> &str { "Print" }
    fn version(&self)     -> &str { "1.0.0" }
    fn description(&self) -> &str { "Outputs a literal message to the console" }

    fn parameters(&self) -> Vec<ParamSpec> {
        vec![
            ParamSpec {
                name:        "message".to_string(),
                param_type:  ParamType::String,
                required:    true,
                description: "Message text to print".to_string(),
                default:     None,
            },
        ]
    }

    fn execute(&self, ctx: &mut AppContext, args: &ArgMap) -> Result<PluginOutput, PluginError> {
        let message = args.get("message")
            .cloned()
            .unwrap_or_default();
        let evaluated = crate::pcode::expr::evaluate_expr(&message, &ctx.variables)
            .unwrap_or(message);
        Ok(PluginOutput::Message(evaluated))
    }
}

// ── Assert ────────────────────────────────────────────────────────────────────

/// Halts execution with an error if the condition is false.
/// Usage: Assert expression="$filecount > 0"
pub struct Assert;

impl PhotonPlugin for Assert {
    fn name(&self)        -> &str { "Assert" }
    fn version(&self)     -> &str { "1.0.0" }
    fn description(&self) -> &str { "Halts execution if the expression evaluates to false" }

    fn parameters(&self) -> Vec<ParamSpec> {
        vec![
            ParamSpec {
                name:        "expression".to_string(),
                param_type:  ParamType::String,
                required:    true,
                description: "Boolean expression to test".to_string(),
                default:     None,
            },
        ]
    }

    fn execute(&self, _ctx: &mut AppContext, args: &ArgMap) -> Result<PluginOutput, PluginError> {
        let expression = args.get("expression")
            .ok_or_else(|| PluginError::missing_arg("expression"))?;

        let empty = std::collections::HashMap::new();
        match crate::pcode::expr::evaluate_condition(expression, &empty) {
            Ok(true)  => Ok(PluginOutput::Success),
            Ok(false) => Err(PluginError::new("ASSERT_FAILED", &format!("Assertion failed: {}", expression))),
            Err(e)    => Err(PluginError::new("EXPR_ERROR", &format!("Assert expression error: {}", e))),
        }
    }
}

// ── CountFiles ────────────────────────────────────────────────────────────────

/// Returns the number of files currently loaded in the session.
/// Stores result in $filecount variable.
/// Usage: CountFiles
pub struct CountFiles;

impl PhotonPlugin for CountFiles {
    fn name(&self)        -> &str { "CountFiles" }
    fn version(&self)     -> &str { "1.0.0" }
    fn description(&self) -> &str { "Returns the number of loaded files; stores result in $filecount" }

    fn parameters(&self) -> Vec<ParamSpec> { vec![] }

    fn execute(&self, ctx: &mut AppContext, _args: &ArgMap) -> Result<PluginOutput, PluginError> {
        let count = ctx.file_list.len();
        ctx.variables.insert("filecount".to_string(), count.to_string());
        Ok(PluginOutput::Value(count.to_string()))
    }
}

// ── Registration ──────────────────────────────────────────────────────────────

// ── LoadFile ──────────────────────────────────────────────────────────────────

/// Loads a single file from disk and displays it in the viewer without
/// adding it to the session file list.
/// Usage: LoadFile path="D:/images/my_heatmap.xisf"
pub struct LoadFile;

impl PhotonPlugin for LoadFile {
    fn name(&self)        -> &str { "LoadFile" }
    fn version(&self)     -> &str { "1.0.0" }
    fn description(&self) -> &str { "Loads a single file for display without adding it to the session" }

    fn parameters(&self) -> Vec<ParamSpec> {
        vec![
            ParamSpec {
                name:        "path".to_string(),
                param_type:  ParamType::Path,
                required:    true,
                description: "Path to the file to load".to_string(),
                default:     None,
            },
        ]
    }

    fn execute(&self, ctx: &mut AppContext, args: &ArgMap) -> Result<PluginOutput, PluginError> {
        let path = args.get("path")
            .ok_or_else(|| PluginError::missing_arg("path"))?;

        let resolved = crate::utils::resolve_path(path, ctx.active_directory.as_deref());

        if !std::path::Path::new(&resolved).exists() {
            return Err(PluginError::new(
                "FILE_NOT_FOUND",
                &format!("File not found: '{}'", resolved),
            ));
        }

        // Store path for frontend to retrieve via get_variable
        ctx.variables.insert("LOAD_FILE_PATH".to_string(), resolved.clone());

        Ok(PluginOutput::Data(serde_json::json!({
            "plugin":  "LoadFile",
            "path":    resolved,
            "message": format!("LoadFile: {}", resolved),
        })))
    }
}

// ── Registration ──────────────────────────────────────────────────────────────

pub fn register_all(registry: &crate::plugin::registry::PluginRegistry) {
    registry.register(Arc::new(Assert));
    registry.register(Arc::new(CopyFile));
    registry.register(Arc::new(CountFiles));
    registry.register(Arc::new(GetKeyword));
    registry.register(Arc::new(LoadFile));
    registry.register(Arc::new(MoveFile));
    registry.register(Arc::new(Print));

}
// ----------------------------------------------------------------------
