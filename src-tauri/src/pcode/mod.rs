// pcode/mod.rs — pcode interpreter
// Spec §7.6, §7.3

pub mod tokenizer;

use std::collections::HashMap;
use tracing::{info, warn};

use crate::context::AppContext;
use crate::plugin::registry::PluginRegistry;
use crate::plugin::PluginOutput;
use tokenizer::{tokenize_line, PcodeLine};

/// A single result from executing one pcode line.
#[derive(Debug, Clone)]
pub struct PcodeResult {
    pub line_number: usize,
    pub command:     String,
    pub success:     bool,
    pub message:     Option<String>,
}

impl PcodeResult {
    pub fn format(&self) -> String {
        let status = if self.success { "OK" } else { "ERR" };
        match &self.message {
            Some(m) => format!("[{}] {}: {}", status, self.command, m),
            None    => format!("[{}] {}", status, self.command),
        }
    }
}

/// Execute a pcode script string against the given context and registry.
/// Returns a Vec of results, one per executed line.
/// Halts on first error by default (halt_on_error = true).
pub fn execute_script(
    script:         &str,
    ctx:            &mut AppContext,
    registry:       &PluginRegistry,
    halt_on_error:  bool,
) -> Vec<PcodeResult> {
    let mut results: Vec<PcodeResult> = Vec::new();
    let mut variables: HashMap<String, String> = HashMap::new();
    let mut last_log_index: usize = 0;

    for (line_num, raw_line) in script.lines().enumerate() {
        let line_number = line_num + 1;

        let parsed = tokenize_line(raw_line);

        match parsed {
            PcodeLine::Skip => continue,

            PcodeLine::Assignment { name, value } => {
                // Substitute variables in the value
                let resolved = substitute_vars(&value, &variables);
                variables.insert(name.clone(), resolved.clone());
                // Also store in AppContext variable store
                ctx.variables.insert(name.clone(), resolved.clone());
                info!("pcode: Set {} = {}", name, resolved);
                results.push(PcodeResult {
                    line_number,
                    command: format!("Set {}", name),
                    success: true,
                    message: Some(format!("{} = {}", name, resolved)),
                });
            }

            PcodeLine::Command { command, mut args } => {
                // Substitute variables in all argument values
                for val in args.values_mut() {
                    *val = substitute_vars(val, &variables);
                }

                // Handle Log command internally — writes results since last Log to file
                if command.to_lowercase() == "log" {
                    let result = handle_log(&args, &results[last_log_index..]);
                    last_log_index = results.len();
                    results.push(PcodeResult {
                        line_number,
                        command: "Log".to_string(),
                        success: result.is_ok(),
                        message: Some(result.unwrap_or_else(|e| e)),
                    });
                    continue;
                }

                // Dispatch to plugin registry
                match registry.dispatch(ctx, &command, &args) {
                    Ok(output) => {
                        let msg = match output {
                            PluginOutput::Success        => None,
                            PluginOutput::Message(m)     => Some(m),
                            PluginOutput::Value(v)       => Some(v),
                            PluginOutput::Values(vs)     => Some(vs.join("\n")),
                        };
                        info!("pcode line {}: {} -> OK", line_number, command);
                        results.push(PcodeResult {
                            line_number,
                            command: command.clone(),
                            success: true,
                            message: msg,
                        });
                    }
                    Err(e) => {
                        warn!("pcode line {}: {} -> ERR: {}", line_number, command, e.message);
                        results.push(PcodeResult {
                            line_number,
                            command: command.clone(),
                            success: false,
                            message: Some(e.message.clone()),
                        });
                        if halt_on_error {
                            break;
                        }
                    }
                }
            }
        }
    }

    results
}

/// Substitute $varname references in a string with their values.
/// Supports $varname and ${varname} syntax.
fn substitute_vars(s: &str, variables: &HashMap<String, String>) -> String {
    let mut result = String::new();
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c != '$' {
            result.push(c);
            continue;
        }

        // Check for ${varname} syntax
        if chars.peek() == Some(&'{') {
            chars.next(); // consume '{'
            let mut name = String::new();
            for c in chars.by_ref() {
                if c == '}' { break; }
                name.push(c);
            }
            if let Some(val) = variables.get(&name) {
                result.push_str(val);
            } else {
                result.push('$');
                result.push('{');
                result.push_str(&name);
                result.push('}');
            }
        } else {
            // Read $varname — alphanumeric + underscore
            let mut name = String::new();
            while let Some(&c) = chars.peek() {
                if c.is_alphanumeric() || c == '_' {
                    name.push(c);
                    chars.next();
                } else {
                    break;
                }
            }
            if let Some(val) = variables.get(&name) {
                result.push_str(val);
            } else {
                result.push('$');
                result.push_str(&name);
            }
        }
    }

    result
}

/// Handle the Log command — write results to a file.
fn handle_log(
    args:    &HashMap<String, String>,
    results: &[PcodeResult],
) -> Result<String, String> {
    let path = args.get("path")
        .ok_or_else(|| "Log: missing path argument".to_string())?;

    let append = args.get("append")
        .map(|v| v == "true")
        .unwrap_or(false);

    let resolved_path = crate::utils::resolve_path(path, None);

    use std::io::Write;
    let file = if append {
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&resolved_path)
    } else {
        std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&resolved_path)
    };

    let mut file = file.map_err(|e| format!("Log: cannot open '{}': {}", resolved_path, e))?;

    let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
    writeln!(file, "# Photyx pcode log — {}", timestamp)
        .map_err(|e| format!("Log: write error: {}", e))?;

    for r in results {
        writeln!(file, "{}", r.format())
            .map_err(|e| format!("Log: write error: {}", e))?;
    }

    let count = results.len();
    Ok(format!("Log written to '{}' ({} entries)", resolved_path, count))
}

// ----------------------------------------------------------------------
