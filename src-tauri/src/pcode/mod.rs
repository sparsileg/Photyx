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
                        PcodeLine::For { .. } => {
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
        }],
    };

    let mut results         = Vec::new();
    let mut variables       = HashMap::new();
    let mut last_log_index  = 0usize;
    let mut halted          = false;

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
    for block in blocks {
        if *halted { return; }

        match block {
            Block::Line { line_number, parsed } => {
                execute_line(
                    *line_number, parsed, ctx, registry, halt_on_error,
                    variables, results, last_log_index, halted,
                );
            }

            Block::If { line_number, expr, then_blocks, else_blocks } => {
                let resolved = substitute_vars(expr, variables);
                let condition = evaluate_condition(&resolved);
                info!("pcode line {}: If [{}] -> {}", line_number, resolved, condition);
                let branch = if condition { then_blocks } else { else_blocks };
                execute_blocks(
                    branch, ctx, registry, halt_on_error,
                    variables, results, last_log_index, halted,
                );
            }

            Block::For { line_number, var, from, to, body } => {
                let from_str = substitute_vars(from, variables);
                let to_str   = substitute_vars(to,   variables);

                let from_val = match from_str.parse::<i64>() {
                    Ok(v)  => v,
                    Err(_) => {
                        results.push(PcodeResult {
                            line_number: *line_number,
                            command: "For".to_string(),
                            success: false,
                            message: Some(format!("For: cannot parse 'from' value '{}'", from_str)),
                        });
                        if halt_on_error { *halted = true; }
                        return;
                    }
                };
                let to_val = match to_str.parse::<i64>() {
                    Ok(v)  => v,
                    Err(_) => {
                        results.push(PcodeResult {
                            line_number: *line_number,
                            command: "For".to_string(),
                            success: false,
                            message: Some(format!("For: cannot parse 'to' value '{}'", to_str)),
                        });
                        if halt_on_error { *halted = true; }
                        return;
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
            let resolved = substitute_vars(value, variables);
            variables.insert(name.clone(), resolved.clone());
            ctx.variables.insert(name.clone(), resolved.clone());
            info!("pcode: Set {} = {}", name, resolved);
            results.push(PcodeResult {
                line_number,
                command: format!("Set {}", name),
                success: true,
                message: Some(format!("{} = {}", name, resolved)),
            });
        }
        PcodeLine::Command { command, args } => {
            let mut resolved_args = args.clone();
            for val in resolved_args.values_mut() {
                *val = substitute_vars(val, variables);
            }
            // Handle Log internally
            if command.to_lowercase() == "log" {
                let result = handle_log(&resolved_args, &results[*last_log_index..]);
                *last_log_index = results.len();
                results.push(PcodeResult {
                    line_number,
                    command: "Log".to_string(),
                    success: result.is_ok(),
                    message: Some(result.unwrap_or_else(|e| e)),
                });
                return;
            }
            match registry.dispatch(ctx, command, &resolved_args) {
                Ok(output) => {
                    // Sync any variables the plugin wrote to ctx.variables
                    for (k, v) in &ctx.variables {
                        variables.insert(k.clone(), v.clone());
                    }
                    let msg = match output {
                        PluginOutput::Success      => None,
                        PluginOutput::Message(m)   => Some(m),
                        PluginOutput::Value(v)     => {
                            // Auto-store single values into a variable named after the name arg
                            if let Some(varname) = resolved_args.get("name")
                                .or_else(|| resolved_args.get("varname"))
                            {
                                let key = varname.to_uppercase();
                                variables.insert(key.clone(), v.clone());
                                ctx.variables.insert(key, v.clone());
                            }
                            Some(v)
                        }
                        PluginOutput::Values(vs)   => Some(vs.join("\n")),
                        PluginOutput::Data(d)       => Some(
                            d.get("message")
                                .and_then(|m| m.as_str())
                                .unwrap_or("Done")
                                .to_string()
                        ),
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
                        *halted = true;
                    }
                }
            }
        }
        // Flow control variants are never stored as bare Lines — handled by parse_blocks
        _ => {}
    }
}

// ── Condition evaluator ───────────────────────────────────────────────────────

fn evaluate_condition(expr: &str) -> bool {
    // Variables already substituted before this call
    for op in &["==", "!=", "<=", ">=", "<", ">"] {
        if let Some(op_pos) = expr.find(op) {
            let lhs = expr[..op_pos].trim();
            let rhs = strip_quotes_str(expr[op_pos + op.len()..].trim());
            return compare_values(lhs, op, &rhs);
        }
    }
    // No operator — treat non-empty, non-"false", non-"0" as true
    let t = expr.trim();
    !t.is_empty() && t != "false" && t != "0"
}

fn compare_values(lhs: &str, op: &str, rhs: &str) -> bool {
    if let (Ok(l), Ok(r)) = (lhs.parse::<f64>(), rhs.parse::<f64>()) {
        return match op {
            "==" => (l - r).abs() < f64::EPSILON,
            "!=" => (l - r).abs() >= f64::EPSILON,
            "<"  => l < r,
            ">"  => l > r,
            "<=" => l <= r,
            ">=" => l >= r,
            _    => false,
        };
    }
    let l = lhs.to_uppercase();
    let r = rhs.to_uppercase();
    match op {
        "==" => l == r,
        "!=" => l != r,
        "<"  => l <  r,
        ">"  => l >  r,
        "<=" => l <= r,
        ">=" => l >= r,
        _    => false,
    }
}

fn strip_quotes_str(s: &str) -> String {
    let s = s.trim();
    if (s.starts_with('"') && s.ends_with('"'))
        || (s.starts_with('\'') && s.ends_with('\''))
    {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

// ── Variable substitution ─────────────────────────────────────────────────────

fn substitute_vars(s: &str, variables: &HashMap<String, String>) -> String {
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
