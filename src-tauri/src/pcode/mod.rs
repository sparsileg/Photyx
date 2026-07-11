// pcode/mod.rs — pcode interpreter
// Spec §7.6, §7.3

pub mod expr;
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
    pub line_number:    usize,
    pub command:        String,
    pub success:        bool,
    pub message:        Option<String>,
    pub data:           Option<serde_json::Value>,
    pub trace_line:     Option<String>,
    pub client_actions: Vec<String>,
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

// ── Block tree ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
enum Block {
    Line {
        line_number: usize,
        parsed:      PcodeLine,
    },
    If {
        line_number:  usize,
        expr:         String,
        then_blocks:  Vec<Block>,
        else_blocks:  Vec<Block>,
    },
    For {
        line_number: usize,
        var:         String,
        from:        String,
        to:          String,
        body:        Vec<Block>,
    },
    ForIn {
        line_number: usize,
        var:         String,
        pattern:     String,
        body:        Vec<Block>,
    },
}

fn parse_blocks(lines: &[(usize, PcodeLine)]) -> Result<Vec<Block>, String> {
    let mut blocks = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let (line_number, ref parsed) = lines[i];

        match parsed {
            PcodeLine::Skip => { i += 1; }

            PcodeLine::If { expr } => {
                let expr = expr.clone();
                i += 1;
                let mut then_lines: Vec<(usize, PcodeLine)> = Vec::new();
                let mut else_lines: Vec<(usize, PcodeLine)> = Vec::new();
                let mut in_else = false;
                let mut depth   = 1usize;

                while i < lines.len() {
                    let (ln, ref pl) = lines[i];
                    match pl {
                        PcodeLine::If { .. } => {
                            depth += 1;
                            if in_else { &mut else_lines } else { &mut then_lines }
                            .push((ln, pl.clone()));
                            i += 1;
                        }
                        PcodeLine::Else if depth == 1 => {
                            in_else = true;
                            i += 1;
                        }
                        PcodeLine::EndIf => {
                            depth -= 1;
                            if depth == 0 { i += 1; break; }
                            if in_else { &mut else_lines } else { &mut then_lines }
                            .push((ln, pl.clone()));
                            i += 1;
                        }
                        _ => {
                            if in_else { &mut else_lines } else { &mut then_lines }
                            .push((ln, pl.clone()));
                            i += 1;
                        }
                    }
                }

                blocks.push(Block::If {
                    line_number,
                    expr,
                    then_blocks: parse_blocks(&then_lines)?,
                    else_blocks: parse_blocks(&else_lines)?,
                });
            }

            PcodeLine::For { var, from, to } => {
                let (var, from, to) = (var.clone(), from.clone(), to.clone());
                i += 1;
                let mut body_lines: Vec<(usize, PcodeLine)> = Vec::new();
                let mut depth = 1usize;

                while i < lines.len() {
                    let (ln, ref pl) = lines[i];
                    match pl {
                        PcodeLine::For { .. } | PcodeLine::ForIn { .. } => {
                            depth += 1;
                            body_lines.push((ln, pl.clone()));
                            i += 1;
                        }
                        PcodeLine::EndFor => {
                            depth -= 1;
                            if depth == 0 { i += 1; break; }
                            body_lines.push((ln, pl.clone()));
                            i += 1;
                        }
                        _ => {
                            body_lines.push((ln, pl.clone()));
                            i += 1;
                        }
                    }
                }

                blocks.push(Block::For {
                    line_number,
                    var,
                    from,
                    to,
                    body: parse_blocks(&body_lines)?,
                });
            }

            PcodeLine::ForIn { var, pattern } => {
                let (var, pattern) = (var.clone(), pattern.clone());
                i += 1;
                let mut body_lines: Vec<(usize, PcodeLine)> = Vec::new();
                let mut depth = 1usize;

                while i < lines.len() {
                    let (ln, ref pl) = lines[i];
                    match pl {
                        PcodeLine::For { .. } | PcodeLine::ForIn { .. } => {
                            depth += 1;
                            body_lines.push((ln, pl.clone()));
                            i += 1;
                        }
                        PcodeLine::EndFor => {
                            depth -= 1;
                            if depth == 0 { i += 1; break; }
                            body_lines.push((ln, pl.clone()));
                            i += 1;
                        }
                        _ => {
                            body_lines.push((ln, pl.clone()));
                            i += 1;
                        }
                    }
                }

                blocks.push(Block::ForIn {
                    line_number,
                    var,
                    pattern,
                    body: parse_blocks(&body_lines)?,
                });
            }

            PcodeLine::Else   => return Err(format!("Line {}: unexpected Else without If",   line_number)),
            PcodeLine::EndIf  => return Err(format!("Line {}: unexpected EndIf without If",  line_number)),
            PcodeLine::EndFor => return Err(format!("Line {}: unexpected EndFor without For", line_number)),

            _ => {
                blocks.push(Block::Line { line_number, parsed: parsed.clone() });
                i += 1;
            }
        }
    }

    Ok(blocks)
}

// ── Main entry point ──────────────────────────────────────────────────────────

pub fn execute_script(
    script:        &str,
    ctx:           &mut AppContext,
    registry:      &PluginRegistry,
    halt_on_error: bool,
) -> Vec<PcodeResult> {
    let tokenized: Vec<(usize, PcodeLine)> = script
        .lines()
        .enumerate()
        .map(|(i, line)| (i + 1, tokenize_line(line)))
        .collect();

    let blocks = match parse_blocks(&tokenized) {
        Ok(b)  => b,
        Err(e) => return vec![PcodeResult {
            line_number: 0,
            command:     "Parse".to_string(),
            success:     false,
            message:     Some(e),
            data:        None,
            trace_line:  None,
            client_actions: vec![],
        }],
    };

    let mut results        = Vec::new();
    let mut variables: HashMap<String, String> = ctx.variables.clone();
    let mut last_log_index = 0usize;
    let mut halted         = false;

    execute_blocks(
        &blocks, ctx, registry, halt_on_error,
        &mut variables, &mut results, &mut last_log_index, &mut halted,
    );

    results
}

// ── Block executor ────────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn execute_blocks(
    blocks:         &[Block],
    ctx:            &mut AppContext,
    registry:       &PluginRegistry,
    halt_on_error:  bool,
    variables:      &mut HashMap<String, String>,
    results:        &mut Vec<PcodeResult>,
    last_log_index: &mut usize,
    halted:         &mut bool,
) {
    'blocks: for block in blocks {
        if *halted { return; }

        match block {
            Block::Line { line_number, parsed } => {
                execute_line(
                    *line_number, parsed, ctx, registry, halt_on_error,
                    variables, results, last_log_index, halted,
                );
            }

            Block::If { line_number, expr, then_blocks, else_blocks } => {
                // Propagate condition errors instead of swallowing to false —
                // a malformed condition now halts (when halt_on_error is set)
                // rather than silently executing the Else branch.
                match expr::evaluate_condition(expr, variables) {
                    Ok(condition) => {
                        info!("pcode line {}: If [{}] -> {}", line_number, expr, condition);
                        let branch = if condition { then_blocks } else { else_blocks };
                        execute_blocks(
                            branch, ctx, registry, halt_on_error,
                            variables, results, last_log_index, halted,
                        );
                    }
                    Err(e) => {
                        results.push(PcodeResult {
                            line_number: *line_number,
                            command:    "If".to_string(),
                            success:    false,
                            message:    Some(format!("If: invalid condition '{}': {}", expr, e)),
                            data:       None,
                            trace_line: None,
            client_actions: vec![],
                        });
                        if halt_on_error { *halted = true; }
                    }
                }
            }

            Block::ForIn { line_number, var, pattern, body } => {
                let pattern_resolved = substitute_vars(pattern, variables);
                info!("pcode line {}: ForIn {} in \"{}\"", line_number, var, pattern_resolved);

                match glob::glob(&pattern_resolved) {
                    Ok(entries) => {
                        let matched: Vec<String> = entries
                            .flatten()
                            .filter_map(|p| p.to_str().map(|s| s.to_string()))
                            .collect();
                        if matched.is_empty() {
                            results.push(PcodeResult {
                                line_number: *line_number,
                                command:    "ForIn".to_string(),
                                success:    true,
                                message:    Some(format!("ForIn: no matches for pattern '{}'", pattern_resolved)),
                                data:       None,
                                trace_line: None,
                                client_actions: vec![],
                            });
                        }
                        for item in matched {
                            if *halted { return; }
                            variables.insert(var.clone(), item.clone());
                            ctx.variables.insert(var.clone(), item.clone());
                            execute_blocks(
                                body, ctx, registry, halt_on_error,
                                variables, results, last_log_index, halted,
                            );
                        }
                    }
                    Err(e) => {
                        results.push(PcodeResult {
                            line_number: *line_number,
                            command:    "ForIn".to_string(),
                            success:    false,
                            message:    Some(format!("ForIn: invalid pattern '{}': {}", pattern_resolved, e)),
                            data:       None,
                            trace_line: None,
                            client_actions: vec![],
                        });
                        if halt_on_error { *halted = true; }
                    }
                }
            }

            Block::For { line_number, var, from, to, body } => {
                // Bounds are expressions ("$filecount - 1"), not bare numbers —
                // evaluate through the shared expression evaluator (same one
                // Set/Print use) before parsing to an integer. Note: raw
                // `from`/`to` are passed in, NOT pre-substituted — evaluate_expr
                // resolves $vars itself (see expr.rs header comment).
                fn eval_bound(expr: &str, variables: &HashMap<String, String>) -> Result<i64, String> {
                    let resolved = expr::evaluate_expr(expr, variables)?;
                    resolved.trim().parse::<i64>().map_err(|_| {
                        match resolved.trim().parse::<f64>() {
                            Ok(n)  => format!("expression evaluated to non-integer value '{}'", n),
                            Err(_) => format!("expression did not evaluate to a number (got '{}')", resolved),
                        }
                    })
                }

                let from_val = match eval_bound(from, variables) {
                    Ok(v)  => v,
                    Err(msg) => {
                        results.push(PcodeResult {
                            line_number: *line_number,
                            command:    "For".to_string(),
                            success:    false,
                            message:    Some(format!("For: cannot parse 'from' value '{}': {}", from, msg)),
                            data:       None,
                            trace_line: None,
            client_actions: vec![],
                        });
                        if halt_on_error { *halted = true; }
                        continue 'blocks;
                    }
                };
                let to_val = match eval_bound(to, variables) {
                    Ok(v)  => v,
                    Err(msg) => {
                        results.push(PcodeResult {
                            line_number: *line_number,
                            command:    "For".to_string(),
                            success:    false,
                            message:    Some(format!("For: cannot parse 'to' value '{}': {}", to, msg)),
                            data:       None,
                            trace_line: None,
            client_actions: vec![],
                        });
                        if halt_on_error { *halted = true; }
                        continue 'blocks;
                    }
                };

                info!("pcode line {}: For {} = {} to {}", line_number, var, from_val, to_val);

                for i in from_val..=to_val {
                    if *halted { return; }
                    variables.insert(var.clone(), i.to_string());
                    ctx.variables.insert(var.clone(), i.to_string());
                    execute_blocks(
                        body, ctx, registry, halt_on_error,
                        variables, results, last_log_index, halted,
                    );
                }
            }
        }
    }
}

// ── Line executor ─────────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn execute_line(
    line_number:    usize,
    parsed:         &PcodeLine,
    ctx:            &mut AppContext,
    registry:       &PluginRegistry,
    halt_on_error:  bool,
    variables:      &mut HashMap<String, String>,
    results:        &mut Vec<PcodeResult>,
    last_log_index: &mut usize,
    halted:         &mut bool,
) {
    match parsed {
        PcodeLine::Skip => {}

        PcodeLine::Assignment { name, value } => {
            let resolved = match expr::evaluate_expr(value, variables) {
                Ok(v)  => v,
                Err(e) => {
                    results.push(PcodeResult {
                        line_number,
                        command:    format!("Set {}", name),
                        success:    false,
                        message:    Some(format!("Expression error: {}", e)),
                        data:       None,
                        trace_line: None,
            client_actions: vec![],
                    });
                    if halt_on_error { *halted = true; }
                    return;
                }
            };
            variables.insert(name.clone(), resolved.clone());
            ctx.variables.insert(name.clone(), resolved.clone());
            info!("pcode: Set {} = {}", name, resolved);
            results.push(PcodeResult {
                line_number,
                command:    format!("Set {}", name),
                success:    true,
                message:    Some(format!("{} = {}", name, resolved)),
                data:       None,
                trace_line: Some(format!("Set {} = {}", name, resolved)),
            client_actions: vec![],
            });
        }

        PcodeLine::Command { command, args } => {
            let mut resolved_args = args.clone();
            for val in resolved_args.values_mut() {
                *val = substitute_vars(val, variables);
            }

            // Handle client-only commands — no Rust-side effect; intercepted here
            // so the frontend can act on them via data.client_command.
            // Must stay in sync with CLIENT_COMMAND_NAMES in clientCommands.ts.
            const CLIENT_COMMANDS: &[&str] = &[
                "showanalysisgraph",
                "showanalysisresults",
                "clearannotations",
                "version",
                "pwd",
            ];
            if CLIENT_COMMANDS.contains(&command.to_lowercase().as_str()) {
                info!("pcode line {}: {} -> client command", line_number, command);
                results.push(PcodeResult {
                    line_number,
                    command:    command.clone(),
                    success:    true,
                    message:    None,
                    data:       Some(serde_json::json!({ "client_command": command.to_lowercase() })),
                    trace_line: Some(command.clone()),
            client_actions: vec![],
                });
                return;
            }

            // Handle Assert internally so variable references evaluate correctly
            if command.to_lowercase() == "assert" {
                let raw_expr = args.get("expression").cloned().unwrap_or_default();
                match crate::pcode::expr::evaluate_condition(&raw_expr, variables) {
                    Ok(true) => {
                        results.push(PcodeResult {
                            line_number,
                            command:    "Assert".to_string(),
                            success:    true,
                            message:    None,
                            data:       None,
                            trace_line: Some(format!("Assert {}", raw_expr)),
            client_actions: vec![],
                        });
                    }
                    Ok(false) => {
                        results.push(PcodeResult {
                            line_number,
                            command:    "Assert".to_string(),
                            success:    false,
                            message:    Some(format!("Assertion failed: {}", raw_expr)),
                            data:       None,
                            trace_line: None,
            client_actions: vec![],
                        });
                        if halt_on_error { *halted = true; }
                    }
                    Err(e) => {
                        results.push(PcodeResult {
                            line_number,
                            command:    "Assert".to_string(),
                            success:    false,
                            message:    Some(format!("Assert expression error: {}", e)),
                            data:       None,
                            trace_line: None,
            client_actions: vec![],
                        });
                        if halt_on_error { *halted = true; }
                    }
                }
                return;
            }
            // Handle Print internally so expressions with variables evaluate correctly
            if command.to_lowercase() == "print" {
                let raw_message = args.get("message").cloned().unwrap_or_default();
                let evaluated = crate::pcode::expr::evaluate_expr(&raw_message, variables)
                    .unwrap_or_else(|_| substitute_vars(&raw_message, variables));
                results.push(PcodeResult {
                    line_number,
                    command:    "Print".to_string(),
                    success:    true,
                    message:    Some(evaluated),
                    data:       None,
                    trace_line: Some(format!("Print {}", raw_message)),
            client_actions: vec![],
                });
                return;
            }

            // Handle Log internally
            if command.to_lowercase() == "log" {
                let result = handle_log(&resolved_args, &results[*last_log_index..]);
                *last_log_index = results.len();
                results.push(PcodeResult {
                    line_number,
                    command:    "Log".to_string(),
                    success:    result.is_ok(),
                    message:    Some(result.unwrap_or_else(|e| e)),
                    data:       None,
                    trace_line: None,
            client_actions: vec![],
                });
                return;
            }

            match registry.dispatch(ctx, command, &resolved_args) {
                        Ok(output) => {
                            // Sync any variables the plugin wrote to ctx.variables
                            for (k, v) in &ctx.variables {
                                variables.insert(k.clone(), v.clone());
                            }
                            let (msg, data, plugin_actions) = match output {
                                PluginOutput::Success      => (None, None, vec![]),
                                PluginOutput::Message(m)   => (Some(m), None, vec![]),
                                PluginOutput::Value(v)     => {
                                    if let Some(varname) = resolved_args.get("name")
                                        .or_else(|| resolved_args.get("varname"))
                                    {
                                        let key = varname.to_uppercase();
                                        variables.insert(key.clone(), v.clone());
                                        ctx.variables.insert(key, v.clone());
                                    }
                                    (Some(v), None, vec![])
                                }
                                PluginOutput::Values(vs)   => (Some(vs.join("\n")), None, vec![]),
                                PluginOutput::Data(d)       => {
                                    let msg = d.get("message")
                                        .and_then(|m| m.as_str())
                                        .unwrap_or("Done")
                                        .to_string();
                                    let actions: Vec<String> = d.get("client_action")
                                        .and_then(|v| v.as_str())
                                        .map(|s| vec![s.to_string()])
                                        .unwrap_or_default();
                                    (Some(msg), Some(d), actions)
                                }
                            };
                    info!("pcode line {}: {} -> OK", line_number, command);
                    results.push(PcodeResult {
                        line_number,
                        command:    command.clone(),
                        success:    true,
                        message:    msg,
                        data,
                        trace_line: Some(format!("{} {}",
                                                 command,
                                                 resolved_args.iter()
                                                 .map(|(k, v)| format!("{}={}", k, v))
                                                 .collect::<Vec<_>>()
                                                 .join(" ")
                        )),
                        client_actions: plugin_actions,
                    });
                }
                Err(e) => {
                    warn!("pcode line {}: {} -> ERR: {}", line_number, command, e.message);
                    results.push(PcodeResult {
                        line_number,
                        command:    command.clone(),
                        success:    false,
                        message:    Some(e.message.clone()),
                        data:       None,
                        trace_line: None,
            client_actions: vec![],
                    });
                    if halt_on_error {
                        *halted = true;
                    }
                }
            }
        }

        // Flow control variants are never stored as bare Lines — handled by parse_blocks
        _ => {}
    }
}


// ── Variable substitution ─────────────────────────────────────────────────────

pub(crate) fn substitute_vars(s: &str, variables: &HashMap<String, String>) -> String {
    let mut result = String::new();
    let mut chars  = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c != '$' {
            result.push(c);
            continue;
        }

        if chars.peek() == Some(&'{') {
            chars.next();
            let mut name = String::new();
            for c in chars.by_ref() {
                if c == '}' { break; }
                name.push(c);
            }
            let val = variables.get(&name)
                .or_else(|| variables.get(&name.to_uppercase()));
            if let Some(val) = val {
                result.push_str(val);
            } else {
                result.push_str(&format!("${{{}}}", name));
            }
        } else {
            let mut name = String::new();
            while let Some(&c) = chars.peek() {
                if c.is_alphanumeric() || c == '_' {
                    name.push(c);
                    chars.next();
                } else {
                    break;
                }
            }
            let val = variables.get(&name)
                .or_else(|| variables.get(&name.to_uppercase()));
            if let Some(val) = val {
                result.push_str(val);
            } else {
                result.push('$');
                result.push_str(&name);
            }
        }
    }

    result
}

// ── Log handler ───────────────────────────────────────────────────────────────

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
        std::fs::OpenOptions::new().create(true).append(true).open(&resolved_path)
    } else {
        std::fs::OpenOptions::new().create(true).write(true).truncate(true).open(&resolved_path)
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
