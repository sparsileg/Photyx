// pcode/tokenizer.rs — Line tokenizer for pcode scripts
// Spec §7.6

/// A parsed pcode line — either a command, assignment, or flow control construct.
#[derive(Debug, Clone)]
pub enum PcodeLine {
    /// A command with a name and argument map
    Command {
        command: String,
        args:    std::collections::HashMap<String, String>,
    },
    /// A variable assignment: Set varname = value
    Assignment {
        name:  String,
        value: String,
    },
    /// If <expr> — begins a conditional block
    If {
        expr: String,
    },
    /// Else — optional else branch of an If block
    Else,
    /// EndIf — closes an If block
    EndIf,
    /// For varname = N to M — begins a numeric loop
    For {
        var:  String,
        from: String,
        to:   String,
    },
    /// EndFor — closes a For block
    EndFor,
    /// A comment or blank line — skip silently
    Skip,
}

/// Tokenize a single line of pcode into a PcodeLine.
pub fn tokenize_line(line: &str) -> PcodeLine {
    let line = line.trim();

    // Blank or comment
    if line.is_empty() || line.starts_with('#') {
        return PcodeLine::Skip;
    }

    // Split into command name and rest
    let first_space = line.find(|c: char| c.is_whitespace());
    let command = if let Some(pos) = first_space {
        line[..pos].to_string()
    } else {
        line.to_string()
    };
    let rest = if let Some(pos) = first_space {
        line[pos + 1..].trim().to_string()
    } else {
        String::new()
    };

    match command.to_lowercase().as_str() {
        "set" => {
            if let Some(eq_pos) = rest.find('=') {
                let name  = rest[..eq_pos].trim().to_string();
                let value = strip_quotes(rest[eq_pos + 1..].trim());
                return PcodeLine::Assignment { name, value };
            }
        }
        "if" => {
            return PcodeLine::If { expr: rest.trim().to_string() };
        }
        "else" => {
            return PcodeLine::Else;
        }
        "endif" => {
            return PcodeLine::EndIf;
        }
        "for" => {
            // Form: varname = N to M
            if let Some(eq_pos) = rest.find('=') {
                let var       = rest[..eq_pos].trim().to_string();
                let after_eq  = rest[eq_pos + 1..].trim();
                if let Some(to_pos) = find_word(after_eq, "to") {
                    let from = after_eq[..to_pos].trim().to_string();
                    let to   = after_eq[to_pos + 2..].trim().to_string();
                    return PcodeLine::For { var, from, to };
                }
            }
        }
        "endfor" => {
            return PcodeLine::EndFor;
        }
        "print" => {
            // Accept both bare argument and message= form
            let message = if rest.contains("message=") {
                parse_args(&rest)
                    .remove("message")
                    .unwrap_or_default()
            } else {
                strip_quotes(rest.trim())
            };
            let mut args = std::collections::HashMap::new();
            args.insert("message".to_string(), message);
            return PcodeLine::Command { command, args };
        }
        _ => {}
    }

    let args = parse_args(&rest);
    PcodeLine::Command { command, args }
}

/// Find `needle` as a whole word in `haystack`. Returns byte position or None.
fn find_word(haystack: &str, needle: &str) -> Option<usize> {
    let lower = haystack.to_lowercase();
    let mut start = 0;
    while let Some(pos) = lower[start..].find(needle) {
        let abs_pos  = start + pos;
        let before_ok = abs_pos == 0 || lower.as_bytes()[abs_pos - 1] == b' ';
        let after_pos = abs_pos + needle.len();
        let after_ok  = after_pos >= lower.len() || lower.as_bytes()[after_pos] == b' ';
        if before_ok && after_ok {
            return Some(abs_pos);
        }
        start = abs_pos + 1;
    }
    None
}

/// Parse named arguments from a string.
/// Handles: key=value  key="quoted value"  key='quoted value'
pub fn parse_args(rest: &str) -> std::collections::HashMap<String, String> {
    let mut args  = std::collections::HashMap::new();
    let mut chars = rest.chars().peekable();

    loop {
        while chars.peek().map(|c| c.is_whitespace()).unwrap_or(false) {
            chars.next();
        }
        if chars.peek().is_none() { break; }

        let mut key = String::new();
        while let Some(&c) = chars.peek() {
            if c == '=' || c.is_whitespace() { break; }
            key.push(c);
            chars.next();
        }
        if key.is_empty() { break; }

        if chars.peek() != Some(&'=') {
            args.insert(key.to_lowercase(), String::new());
            continue;
        }
        chars.next(); // consume '='

        let value = if chars.peek() == Some(&'"') || chars.peek() == Some(&'\'') {
            let quote = chars.next().unwrap();
            let mut v = String::new();
            for c in chars.by_ref() {
                if c == quote { break; }
                v.push(c);
            }
            v
        } else {
            let mut v = String::new();
            while let Some(&c) = chars.peek() {
                if c.is_whitespace() { break; }
                v.push(c);
                chars.next();
            }
            v
        };

        args.insert(key.to_lowercase(), value);
    }

    args
}

/// Strip surrounding single or double quotes from a string.
pub fn strip_quotes(s: &str) -> String {
    let s = s.trim();
    if (s.starts_with('"') && s.ends_with('"'))
        || (s.starts_with('\'') && s.ends_with('\''))
    {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

// ----------------------------------------------------------------------
