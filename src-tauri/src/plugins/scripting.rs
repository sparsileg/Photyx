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
    fn version(&self)     -> &str { "1.1.0" }
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
            ParamSpec {
                name:        "default".to_string(),
                param_type:  ParamType::String,
                required:    false,
                description: "Fallback value to use if the keyword is not found on the current frame, \
                              instead of halting the script (e.g. default=\"\" or default=\"NULL\"). \
                              Does not apply to no-frame-loaded or no-buffer errors.".to_string(),
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
            .map(|(_, v)| v);

        let value = match entry {
            Some(kw) => kw.value.trim().to_string(),
            None => match args.get("default") {
                Some(default) => default.clone(),
                None => return Err(PluginError::new(
                    "NOT_FOUND",
                    &format!("GetKeyword: keyword '{}' not found in current frame", name),
                )),
            },
        };

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
            ctx.common_parent().as_ref().and_then(|p| p.to_str()),
        );

        // Use explicit source if provided, otherwise use current frame
        let src_path = if let Some(source) = args.get("source") {
            crate::utils::resolve_path(source, ctx.common_parent().as_ref().and_then(|p| p.to_str()))
        } else {
            ctx.file_list
                .get(ctx.current_frame)
                .cloned()
                .ok_or_else(|| PluginError::new("NO_FRAME", "MoveFile: no current frame"))?
        };

        let src = Path::new(&src_path);
        let filename = src.file_name()
            .ok_or_else(|| PluginError::new("BAD_PATH", "MoveFile: cannot determine filename"))?;

        // If destination is an existing directory or ends with a separator,
        // move into it preserving the filename. Otherwise treat destination
        // as a full file path (mv semantics — allows rename during move).
        let dest_path = if Path::new(&dest_dir).is_dir() || dest_dir.ends_with('/') || dest_dir.ends_with('\\') {
            std::fs::create_dir_all(&dest_dir)
                .map_err(|e| PluginError::new(
                    "IO_ERROR",
                    &format!("MoveFile: cannot create directory '{}': {}", dest_dir, e),
                ))?;
            Path::new(&dest_dir).join(filename)
        } else {
            // Full file path — create parent directory if needed
            let parent = Path::new(&dest_dir).parent()
                .ok_or_else(|| PluginError::new("BAD_PATH", "MoveFile: cannot determine destination parent directory"))?;
            std::fs::create_dir_all(parent)
                .map_err(|e| PluginError::new(
                    "IO_ERROR",
                    &format!("MoveFile: cannot create directory '{}': {}", parent.display(), e),
                ))?;
            Path::new(&dest_dir).to_path_buf()
        };

        if let Err(rename_err) = std::fs::rename(&src_path, &dest_path) {
            // rename() only works within a single filesystem. Fall back to
            // copy + delete for cross-device moves (e.g. external drive -> local disk).
            std::fs::copy(&src_path, &dest_path)
                .map_err(|copy_err| PluginError::new(
                    "IO_ERROR",
                    &format!(
                        "MoveFile: cannot move '{}' to '{}': rename failed ({}), copy fallback also failed ({})",
                        src_path, dest_path.display(), rename_err, copy_err,
                    ),
                ))?;
            std::fs::remove_file(&src_path)
                .map_err(|e| PluginError::new(
                    "IO_ERROR",
                    &format!(
                        "MoveFile: copied '{}' to '{}' but could not remove the original: {}",
                        src_path, dest_path.display(), e,
                    ),
                ))?;
        }

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

        ctx.variables.insert("NEW_FILE".to_string(), dest_str.clone());

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
            ctx.common_parent().as_ref().and_then(|p| p.to_str()),
        );

        let src_path = if let Some(source) = args.get("source") {
            crate::utils::resolve_path(source, ctx.common_parent().as_ref().and_then(|p| p.to_str()))
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

//    CountMatches

/// Counts filesystem entries matching a glob pattern.
/// Stores result in $matchcount variable.
/// Usage: CountMatches pattern="J:/projects/M82/*-duo-*"
pub struct CountMatches;

impl PhotonPlugin for CountMatches {
    fn name(&self)        -> &str { "CountMatches" }
    fn version(&self)     -> &str { "1.0.0" }
    fn description(&self) -> &str { "Counts filesystem entries matching a glob pattern; stores result in $matchcount" }

    fn parameters(&self) -> Vec<ParamSpec> {
        vec![
            ParamSpec {
                name:        "pattern".to_string(),
                param_type:  ParamType::String,
                required:    true,
                description: "Glob pattern to match against".to_string(),
                default:     None,
            },
        ]
    }

    fn execute(&self, ctx: &mut AppContext, args: &ArgMap) -> Result<PluginOutput, PluginError> {
        let pattern = args.get("pattern")
            .ok_or_else(|| PluginError::missing_arg("pattern"))?;

        let resolved = crate::pcode::expr::evaluate_expr(pattern, &ctx.variables)
            .unwrap_or_else(|_| pattern.clone());

        let count = match glob::glob(&resolved) {
            Ok(entries) => entries.flatten().count(),
            Err(e) => return Err(PluginError::new(
                "INVALID_PATTERN",
                &format!("CountMatches: invalid glob pattern '{}': {}", resolved, e),
            )),
        };

        ctx.variables.insert("matchcount".to_string(), count.to_string());
        Ok(PluginOutput::Value(count.to_string()))
    }
}

//    GetSystemPath

/// Retrieves a well-known system directory path and stores it in a variable.
/// Usage: GetSystemPath name=downloads
/// Supported names: downloads, documents, desktop, temp
/// Result is stored in $<name> (e.g. $downloads).
pub struct GetSystemPath;

impl PhotonPlugin for GetSystemPath {
    fn name(&self)        -> &str { "GetSystemPath" }
    fn version(&self)     -> &str { "1.0.0" }
    fn description(&self) -> &str { "Retrieves a well-known system directory path; stores result in $<name>" }

    fn parameters(&self) -> Vec<ParamSpec> {
        vec![
            ParamSpec {
                name:        "name".to_string(),
                param_type:  ParamType::String,
                required:    true,
                description: "System path to retrieve: downloads, documents, desktop, or temp".to_string(),
                default:     None,
            },
        ]
    }

    fn execute(&self, ctx: &mut AppContext, args: &ArgMap) -> Result<PluginOutput, PluginError> {
        let name = args.get("name")
            .ok_or_else(|| PluginError::missing_arg("name"))?
            .trim()
            .to_lowercase();

        let path = match name.as_str() {
            "downloads" => dirs_next::download_dir(),
            "documents" => dirs_next::document_dir(),
            "desktop"   => dirs_next::desktop_dir(),
            "temp"      => Some(std::env::temp_dir()),
            _ => return Err(PluginError::new(
                "UNKNOWN_PATH",
                &format!("GetSystemPath: unknown path name '{}'. Use: downloads, documents, desktop, temp", name),
            )),
        };

        let path_str = path
            .ok_or_else(|| PluginError::new(
                "NOT_FOUND",
                &format!("GetSystemPath: could not resolve '{}' on this system", name),
            ))?
            .to_string_lossy()
            .replace('\\', "/")
            .to_string();

        ctx.variables.insert(name.clone(), path_str.clone());
        Ok(PluginOutput::Value(path_str))
    }
}

//    LoadFile

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

        let resolved = crate::utils::resolve_path(path, ctx.common_parent().as_ref().and_then(|p| p.to_str()));

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
    registry.register(Arc::new(CountMatches));
    registry.register(Arc::new(GetKeyword));
    registry.register(Arc::new(GetSystemPath));
    registry.register(Arc::new(LoadFile));
    registry.register(Arc::new(MoveFile));
    registry.register(Arc::new(Print));
}

// ----------------------------------------------------------------------
