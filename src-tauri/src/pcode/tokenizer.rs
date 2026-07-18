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
    /// For varname in "glob_pattern" — begins a glob iterator loop
    ForIn {
        var:     String,
        pattern: String,
    },
    /// EndFor — closes a For or ForIn block
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
                let value = rest[eq_pos + 1..].trim().to_string();
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
            // Form: for varname in "glob_pattern"
            if let Some(in_pos) = find_word(&rest, "in") {
                let var     = rest[..in_pos].trim().to_string();
                let pattern = rest[in_pos + 2..].trim().trim_matches('"').to_string();
                if !var.is_empty() && !pattern.is_empty() {
                    return PcodeLine::ForIn { var, pattern };
                }
            }
            // Form: for varname = N to M
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
            // Store the raw expression so the evaluator can handle
            // variables, operators, and quoted strings correctly
            let mut args = std::collections::HashMap::new();
            args.insert("message".to_string(), rest.trim().to_string());
            return PcodeLine::Command { command, args };
        }
        _ => {}
    }

    let args = parse_args(&rest);
    PcodeLine::Command { command, args }
}

/// Find `needle` as a whole word in `haystack`, case-insensitively.
/// Returns a byte offset valid in the ORIGINAL `haystack` (never a
/// lowercased copy), or None. `needle` must be pure ASCII.
///
/// Issue 119: previously computed the match position on
/// haystack.to_lowercase() and returned that offset directly, but callers
/// (the "for"/"in"/"to" tokenizer branches) sliced the *original*
/// haystack at that offset. to_lowercase() can change UTF-8 byte length
/// for some non-ASCII characters (e.g. Turkish dotted İ), which could
/// shift the returned offset past a char boundary in the original string
/// and panic when a caller sliced it — the same class of bug fixed in
/// stripext() elsewhere in this issue. Also fixes the separate
/// whitespace-separator gap: only a literal space (0x20) was previously
/// accepted as a word boundary, so a tab between "for x" and 'in
/// "pattern"' failed to match the ForIn form.
fn find_word(haystack: &str, needle: &str) -> Option<usize> {
    debug_assert!(needle.is_ascii(), "find_word needle must be ASCII");
    let needle_len = needle.len();
    let is_sep = |c: Option<char>| c.map(|c| c.is_whitespace()).unwrap_or(true);

    for (i, _) in haystack.char_indices() {
        let end = i + needle_len;
        if end > haystack.len() || !haystack.is_char_boundary(end) {
            continue;
        }
        if !haystack[i..end].eq_ignore_ascii_case(needle) {
            continue;
        }
        let before_ok = is_sep(haystack[..i].chars().next_back());
        let after_ok  = is_sep(haystack[end..].chars().next());
        if before_ok && after_ok {
            return Some(i);
        }
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

#[cfg(test)]
mod find_word_tests {
    use super::*;

    #[test]
    fn tab_separator_matches_forin() {
        // Issue 119 acceptance criterion: tab between "in" and the quoted
        // pattern must still parse as ForIn, not fall through to Command.
        let line = "for f in\t\"*.fit\"";
        let parsed = tokenize_line(line);
        match parsed {
            PcodeLine::ForIn { var, pattern } => {
                assert_eq!(var, "f");
                assert_eq!(pattern, "*.fit");
            }
            other => panic!("expected ForIn, got {:?}", other),
        }
    }

    #[test]
    fn tab_before_in_also_matches() {
        let line = "for f\tin \"*.fit\"";
        let parsed = tokenize_line(line);
        assert!(matches!(parsed, PcodeLine::ForIn { .. }), "expected ForIn, got {:?}", parsed);
    }

    #[test]
    fn space_separator_still_matches() {
        // Regression guard: ordinary space-separated form must keep working.
        let line = "for f in \"*.fit\"";
        let parsed = tokenize_line(line);
        match parsed {
            PcodeLine::ForIn { var, pattern } => {
                assert_eq!(var, "f");
                assert_eq!(pattern, "*.fit");
            }
            other => panic!("expected ForIn, got {:?}", other),
        }
    }

    #[test]
    fn substring_match_is_not_a_whole_word() {
        // "in" inside "print" or similar must not be treated as the ForIn
        // keyword — before/after must both be whitespace (or string edge).
        assert_eq!(find_word("printer", "in"), None);
    }

    #[test]
    fn non_ascii_haystack_does_not_panic() {
        // Issue 119: the offset-safety half of this fix — a non-ASCII
        // path/pattern (Turkish dotted İ) anywhere in the haystack must
        // never cause a mid-codepoint slice panic, whether or not "in" is
        // found.
        let line = "for f in \"/data/İstanbul-M31/*.fit\"";
        let parsed = tokenize_line(line);
        match parsed {
            PcodeLine::ForIn { var, pattern } => {
                assert_eq!(var, "f");
                assert_eq!(pattern, "/data/İstanbul-M31/*.fit");
            }
            other => panic!("expected ForIn, got {:?}", other),
        }
    }

    #[test]
    fn numeric_for_with_tab_around_to_still_matches() {
        let line = "for i = 1\tTo\t5";
        let parsed = tokenize_line(line);
        match parsed {
            PcodeLine::For { var, from, to } => {
                assert_eq!(var, "i");
                assert_eq!(from, "1");
                assert_eq!(to, "5");
            }
            other => panic!("expected For, got {:?}", other),
        }
    }
}

// ----------------------------------------------------------------------
