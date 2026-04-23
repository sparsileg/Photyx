// pcode/tokenizer.rs — Line tokenizer for pcode scripts
// Spec §7.6

/// A parsed pcode line — either a command or a variable assignment.
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

    // Handle Set command specially: Set varname = value
    if command.to_lowercase() == "set" {
        if let Some(eq_pos) = rest.find('=') {
            let name = rest[..eq_pos].trim().to_string();
            let value = rest[eq_pos + 1..].trim().to_string();
            // Strip surrounding quotes if present
            let value = strip_quotes(&value);
            return PcodeLine::Assignment { name, value };
        }
    }

    // Parse named arguments: key=value or key="quoted value"
    let args = parse_args(&rest);

    PcodeLine::Command { command, args }
}

/// Parse named arguments from a string.
/// Handles: key=value  key="quoted value"  key='quoted value'
pub fn parse_args(rest: &str) -> std::collections::HashMap<String, String> {
    let mut args = std::collections::HashMap::new();
    let mut chars = rest.chars().peekable();

    loop {
        // Skip whitespace
        while chars.peek().map(|c| c.is_whitespace()).unwrap_or(false) {
            chars.next();
        }
        if chars.peek().is_none() { break; }

        // Read key
        let mut key = String::new();
        while let Some(&c) = chars.peek() {
            if c == '=' || c.is_whitespace() { break; }
            key.push(c);
            chars.next();
        }
        if key.is_empty() { break; }

        // Expect '='
        if chars.peek() != Some(&'=') {
            // Positional arg without value — store as empty
            args.insert(key.to_lowercase(), String::new());
            continue;
        }
        chars.next(); // consume '='

        // Read value — quoted or unquoted
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
