// pcode/expr.rs — Arithmetic and string expression evaluator
// Spec §7.3, §7.5
//
// Variables are resolved inside the evaluator via a variables map.
// This means $varname tokens are never pre-substituted before evaluation,
// so resolved string values (paths, names, etc.) are always known to be
// strings and never re-tokenized as expressions.

use std::collections::HashMap;

/// Evaluate an expression string with variable resolution.
/// Returns Ok(result) or Err(message).
pub fn evaluate_expr(expr: &str, variables: &HashMap<String, String>) -> Result<String, String> {
    let expr = normalize_quotes(expr.trim());
    if expr.is_empty() {
        return Ok(String::new());
    }
    match parse_expr(&expr, variables) {
        Ok(Value::Num(n)) => Ok(format_number(n)),
        Ok(Value::Str(s)) => Ok(s),
        Err(e)            => Err(e),
    }
}

/// Evaluate an expression as a boolean for use in If conditions.
pub fn evaluate_condition(expr: &str, variables: &HashMap<String, String>) -> Result<bool, String> {
    let expr = normalize_quotes(expr.trim());

    // Look for comparison operators outside parentheses
    for op in &["==", "!=", "<=", ">=", "<", ">"] {
        if let Some(pos) = find_comparison_op(&expr, op) {
            let lhs_str = expr[..pos].trim();
            let rhs_str = expr[pos + op.len()..].trim();
            let lhs = resolve_side(lhs_str, variables)?;
            let rhs = resolve_side(rhs_str, variables)?;
            return Ok(compare_values(&lhs, op, &rhs));
        }
    }

    // No comparison operator — evaluate as value and treat as boolean
    let val = evaluate_expr(&expr, variables)?;
    let t = val.trim();
    Ok(!t.is_empty() && t != "false" && t != "0")
}

/// Look up a variable by name, case-insensitively. All variable writes
/// normalize their keys to uppercase (Issue 118), so a single uppercase
/// lookup is sufficient here — the previous "try exact, then uppercase"
/// two-step was only ever needed because write sites were inconsistent.
/// Shared by every $var read site in the interpreter (mod.rs included).
pub(crate) fn lookup_var<'a>(name: &str, variables: &'a HashMap<String, String>) -> Option<&'a String> {
    variables.get(&name.to_uppercase())
}

/// Resolve one side of a comparison. Plain variable references are looked up
/// directly so their resolved string values never hit the tokenizer.
fn resolve_side(s: &str, variables: &HashMap<String, String>) -> Result<String, String> {
    let s = s.trim();
    // Plain variable reference: $varname or ${varname}
    if s.starts_with('$') {
        let name = if s.starts_with("${") && s.ends_with('}') {
            &s[2..s.len() - 1]
        } else {
            &s[1..]
        };
        // Issue 118: undefined variable is a hard error, not empty-string —
        // a typo'd $name previously compared/printed as "" with no signal
        // that anything was wrong, contradicting halt-on-error.
        let val = lookup_var(name, variables)
            .cloned()
            .ok_or_else(|| format!("Undefined variable '${}'", name))?;
        return Ok(val);
    }
    // Otherwise evaluate as expression
    evaluate_expr(s, variables)
}

// ── Quote normalization ───────────────────────────────────────────────────────

fn normalize_quotes(s: &str) -> String {
    s.replace('\u{201C}', "\"")
        .replace('\u{201D}', "\"")
        .replace('\u{2018}', "'")
        .replace('\u{2019}', "'")
}

// ── Value type ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
enum Value {
    Num(f64),
    Str(String),
}

impl Value {
    fn as_str(&self) -> String {
        match self {
            Value::Num(n) => format_number(*n),
            Value::Str(s) => s.clone(),
        }
    }

    fn as_num(&self) -> Option<f64> {
        match self {
            Value::Num(n) => Some(*n),
            Value::Str(s) => s.trim().parse::<f64>().ok(),
        }
    }
}

// ── Token type ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
enum Token {
    Num(f64),
    Str(String),
    Var(String),   // $varname — resolved during parse_primary
    Ident(String), // function name or bare word
    Op(char),
    LParen,
    RParen,
    Comma,
}

impl Token {
    fn display(&self) -> String {
        match self {
            Token::Num(n)   => format!("{}", n),
            Token::Str(s)   => format!("\"{}\"", s),
            Token::Var(s)   => format!("${}", s),
            Token::Ident(s) => s.clone(),
            Token::Op(c)    => c.to_string(),
            Token::LParen   => "(".to_string(),
            Token::RParen   => ")".to_string(),
            Token::Comma    => ",".to_string(),
        }
    }
}

// ── Tokenizer ─────────────────────────────────────────────────────────────────

fn tokenize(input: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let mut chars  = input.chars().peekable();

    while let Some(&c) = chars.peek() {
        match c {
            ' ' | '\t' => { chars.next(); }

            '"' | '\'' => {
                let quote = chars.next().unwrap();
                let mut s = String::new();
                let mut closed = false;
                for ch in chars.by_ref() {
                    if ch == quote { closed = true; break; }
                    s.push(ch);
                }
                if !closed {
                    return Err(format!("Unterminated string literal"));
                }
                tokens.push(Token::Str(s));
            }

            '$' => {
                chars.next(); // consume '$'
                let mut name = String::new();
                if chars.peek() == Some(&'{') {
                    chars.next(); // consume '{'
                    for ch in chars.by_ref() {
                        if ch == '}' { break; }
                        name.push(ch);
                    }
                } else {
                    while let Some(&ic) = chars.peek() {
                        if ic.is_alphanumeric() || ic == '_' {
                            name.push(ic);
                            chars.next();
                        } else {
                            break;
                        }
                    }
                }
                tokens.push(Token::Var(name));
            }

            '0'..='9' | '.' => {
                let mut num = String::new();
                while let Some(&d) = chars.peek() {
                    if d.is_ascii_digit() || d == '.' {
                        num.push(d);
                        chars.next();
                    } else {
                        break;
                    }
                }
                let n = num.parse::<f64>()
                    .map_err(|_| format!("Invalid number '{}'", num))?;
                tokens.push(Token::Num(n));
            }

            'a'..='z' | 'A'..='Z' | '_' => {
                let mut ident = String::new();
                while let Some(&ic) = chars.peek() {
                    if ic.is_alphanumeric() || ic == '_' {
                        ident.push(ic);
                        chars.next();
                    } else {
                        break;
                    }
                }
                tokens.push(Token::Ident(ident));
            }

            '+' | '-' | '*' | '/' | '^' => {
                tokens.push(Token::Op(c));
                chars.next();
            }
            '(' => { tokens.push(Token::LParen); chars.next(); }
            ')' => { tokens.push(Token::RParen); chars.next(); }
            ',' => { tokens.push(Token::Comma);  chars.next(); }

            c => {
                return Err(format!(
                    "Unexpected character '{}' in expression — strings must be quoted",
                    c
                ));
            }
        }
    }

    Ok(tokens)
}

// ── Top-level parser ──────────────────────────────────────────────────────────

fn parse_expr(input: &str, variables: &HashMap<String, String>) -> Result<Value, String> {
    let input = input.trim();
    if input.is_empty() {
        return Ok(Value::Str(String::new()));
    }
    let tokens = tokenize(input)?;
    if tokens.is_empty() {
        return Ok(Value::Str(String::new()));
    }
    let mut pos = 0;
    let val = parse_addition(&tokens, &mut pos, variables)?;
    if pos < tokens.len() {
        return Err(format!("Unexpected token '{}' in expression", tokens[pos].display()));
    }
    Ok(val)
}

// ── Recursive descent ─────────────────────────────────────────────────────────

fn parse_addition(tokens: &[Token], pos: &mut usize, variables: &HashMap<String, String>) -> Result<Value, String> {
    let mut left = parse_multiplication(tokens, pos, variables)?;

    while *pos < tokens.len() {
        match &tokens[*pos] {
            Token::Op('+') => {
                *pos += 1;
                let right = parse_multiplication(tokens, pos, variables)?;
                match (left.as_num(), right.as_num()) {
                    (Some(l), Some(r)) => left = Value::Num(l + r),
                    _ => left = Value::Str(left.as_str() + &right.as_str()),
                }
            }
            Token::Op('-') => {
                *pos += 1;
                let right = parse_multiplication(tokens, pos, variables)?;
                let l = left.as_num().ok_or_else(|| {
                    format!("Cannot subtract non-numeric value '{}'", left.as_str())
                })?;
                let r = right.as_num().ok_or_else(|| {
                    format!("Cannot subtract non-numeric value '{}'", right.as_str())
                })?;
                left = Value::Num(l - r);
            }
            _ => break,
        }
    }
    Ok(left)
}

fn parse_multiplication(tokens: &[Token], pos: &mut usize, variables: &HashMap<String, String>) -> Result<Value, String> {
    let mut left = parse_exponentiation(tokens, pos, variables)?;

    while *pos < tokens.len() {
        match &tokens[*pos] {
            Token::Op('*') => {
                *pos += 1;
                let right = parse_exponentiation(tokens, pos, variables)?;
                let l = left.as_num().ok_or_else(|| {
                    format!("Cannot multiply non-numeric value '{}'", left.as_str())
                })?;
                let r = right.as_num().ok_or_else(|| {
                    format!("Cannot multiply non-numeric value '{}'", right.as_str())
                })?;
                left = Value::Num(l * r);
            }
            Token::Op('/') => {
                *pos += 1;
                let right = parse_exponentiation(tokens, pos, variables)?;
                let l = left.as_num().ok_or_else(|| {
                    format!("Cannot divide non-numeric value '{}'", left.as_str())
                })?;
                let r = right.as_num().ok_or_else(|| {
                    format!("Cannot divide non-numeric value '{}'", right.as_str())
                })?;
                if r == 0.0 {
                    return Err("Division by zero".to_string());
                }
                left = Value::Num(l / r);
            }
            _ => break,
        }
    }
    Ok(left)
}

fn parse_exponentiation(tokens: &[Token], pos: &mut usize, variables: &HashMap<String, String>) -> Result<Value, String> {
    let base = parse_unary(tokens, pos, variables)?;
    if *pos < tokens.len() {
        if let Token::Op('^') = &tokens[*pos] {
            *pos += 1;
            let exp = parse_exponentiation(tokens, pos, variables)?;
            let b = base.as_num().ok_or_else(|| {
                format!("Cannot exponentiate non-numeric value '{}'", base.as_str())
            })?;
            let e = exp.as_num().ok_or_else(|| {
                format!("Cannot exponentiate non-numeric value '{}'", exp.as_str())
            })?;
            return Ok(Value::Num(b.powf(e)));
        }
    }
    Ok(base)
}

fn parse_unary(tokens: &[Token], pos: &mut usize, variables: &HashMap<String, String>) -> Result<Value, String> {
    if *pos < tokens.len() {
        if let Token::Op('-') = &tokens[*pos] {
            *pos += 1;
            let val = parse_primary(tokens, pos, variables)?;
            let n = val.as_num().ok_or_else(|| {
                format!("Cannot negate non-numeric value '{}'", val.as_str())
            })?;
            return Ok(Value::Num(-n));
        }
    }
    parse_primary(tokens, pos, variables)
}

fn parse_primary(tokens: &[Token], pos: &mut usize, variables: &HashMap<String, String>) -> Result<Value, String> {
    if *pos >= tokens.len() {
        return Err("Unexpected end of expression".to_string());
    }

    match tokens[*pos].clone() {
        Token::Num(n) => {
            *pos += 1;
            Ok(Value::Num(n))
        }
        Token::Str(s) => {
            *pos += 1;
            Ok(Value::Str(s))
        }
        Token::Var(name) => {
            *pos += 1;
            // Issue 118: undefined variable is a hard error, not empty-string
            // — see resolve_side for the matching rationale.
            let val = lookup_var(&name, variables)
                .cloned()
                .ok_or_else(|| format!("Undefined variable '${}'", name))?;
            // Return as number if it parses, otherwise as string
            if let Ok(n) = val.trim().parse::<f64>() {
                Ok(Value::Num(n))
            } else {
                Ok(Value::Str(val))
            }
        }
        Token::Ident(name) => {
            *pos += 1;
            // Check for function call
            if *pos < tokens.len() {
                if let Token::LParen = &tokens[*pos] {
                    *pos += 1;
                    return call_function(&name, tokens, pos, variables);
                }
            }
            // Bare identifier — treat as string
            Ok(Value::Str(name))
        }
        Token::LParen => {
            *pos += 1;
            let val = parse_addition(tokens, pos, variables)?;
            if *pos >= tokens.len() {
                return Err("Missing closing ')'".to_string());
            }
            if let Token::RParen = &tokens[*pos] {
                *pos += 1;
            } else {
                return Err(format!("Expected ')' but found '{}'", tokens[*pos].display()));
            }
            Ok(val)
        }
        t => Err(format!("Unexpected token '{}' in expression", t.display())),
    }
}

// ── Function calls ────────────────────────────────────────────────────────────

fn call_function(name: &str, tokens: &[Token], pos: &mut usize, variables: &HashMap<String, String>) -> Result<Value, String> {
    let mut args: Vec<Value> = Vec::new();

    if *pos < tokens.len() {
        if let Token::RParen = &tokens[*pos] {
            *pos += 1;
        } else {
            loop {
                args.push(parse_addition(tokens, pos, variables)?);
                if *pos >= tokens.len() {
                    return Err(format!("Missing ')' after call to {}()", name));
                }
                match &tokens[*pos] {
                    Token::Comma  => { *pos += 1; }
                    Token::RParen => { *pos += 1; break; }
                    t => return Err(format!(
                        "Expected ',' or ')' in {}() but found '{}'", name, t.display()
                    )),
                }
            }
        }
    }

    match name.to_lowercase().as_str() {
        "sqrt" => {
            let x = one_numeric_arg(&args, "sqrt")?;
            if x < 0.0 { return Err(format!("sqrt() of negative value {}", x)); }
            Ok(Value::Num(x.sqrt()))
        }
        "abs"      => Ok(Value::Num(one_numeric_arg(&args, "abs")?.abs())),
        "basename" => {
            let s = one_string_arg(&args, "basename")?;
            let normalized = s.replace('\\', "/");
            let base = match normalized.rfind('/') {
                Some(pos) => normalized[pos + 1..].to_string(),
                None      => normalized,
            };
            Ok(Value::Str(base))
        }
        "ceil"  => Ok(Value::Num(one_numeric_arg(&args, "ceil")?.ceil())),
        "dirof" => {
            let s = one_string_arg(&args, "dirof")?;
            let normalized = s.replace('\\', "/");
            let dir = match normalized.rfind('/') {
                Some(pos) => normalized[..pos].to_string(),
                None      => ".".to_string(),
            };
            Ok(Value::Str(dir))
        }
        "floor" => Ok(Value::Num(one_numeric_arg(&args, "floor")?.floor())),
        "max"   => two_numeric_args(&args, "max").map(|(a, b)| Value::Num(a.max(b))),
        "min"   => two_numeric_args(&args, "min").map(|(a, b)| Value::Num(a.min(b))),
        "round" => Ok(Value::Num(one_numeric_arg(&args, "round")?.round())),
        "stripext" => {
            let s = one_string_arg(&args, "stripext")?;
            Ok(Value::Str(strip_known_extension(&s)))
        }
        _       => Err(format!("Unknown function '{}()'", name)),
    }
}

fn one_numeric_arg(args: &[Value], fname: &str) -> Result<f64, String> {
    if args.len() != 1 {
        return Err(format!("{}() takes 1 argument, got {}", fname, args.len()));
    }
    args[0].as_num().ok_or_else(|| {
        format!("{}() argument must be numeric, got '{}'", fname, args[0].as_str())
    })
}

fn two_numeric_args(args: &[Value], fname: &str) -> Result<(f64, f64), String> {
    if args.len() != 2 {
        return Err(format!("{}() takes 2 arguments, got {}", fname, args.len()));
    }
    let a = args[0].as_num().ok_or_else(|| format!("{}() arguments must be numeric", fname))?;
    let b = args[1].as_num().ok_or_else(|| format!("{}() arguments must be numeric", fname))?;
    Ok((a, b))
}

fn one_string_arg(args: &[Value], fname: &str) -> Result<String, String> {
    if args.len() != 1 {
        return Err(format!("{}() takes 1 argument, got {}", fname, args.len()));
    }
    Ok(args[0].as_str())
}

//    Helpers

fn find_comparison_op(expr: &str, op: &str) -> Option<usize> {
    let bytes = expr.as_bytes();
    let len   = bytes.len();
    let olen  = op.len();
    let mut depth = 0usize;
    let mut i = 0;

    while i + olen <= len {
        match bytes[i] {
            b'(' => { depth += 1; i += 1; }
            b')' => { depth = depth.saturating_sub(1); i += 1; }
            b'"' | b'\'' => {
                let q = bytes[i];
                i += 1;
                while i < len && bytes[i] != q { i += 1; }
                if i < len { i += 1; }
            }
            _ => {
                if depth == 0 && expr[i..].starts_with(op) {
                    let end = i + olen;
                    let next_is_eq = end < len && bytes[end] == b'=';
                    if olen == 1 && (op == "<" || op == ">") && next_is_eq {
                        i += 1;
                        continue;
                    }
                    return Some(i);
                }
                i += 1;
            }
        }
    }
    None
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

fn format_number(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        format!("{}", n)
    }
}

/// Strips a trailing suffix appended after a known image extension
/// (.fit, .fits, .fts, .xisf) from a path — e.g. the `.session`/`.project`
/// suffix CommitAnalysis appends. Case-insensitive on the extension.
fn strip_known_extension(s: &str) -> String {
    let normalized = s.replace('\\', "/");
    // Issue 119: search directly on the original string with ASCII
    // case-insensitive comparison at each valid char boundary, rather than
    // computing a match position on a to_lowercase() copy and slicing the
    // original at that position — to_lowercase() can change UTF-8 byte
    // length for some non-ASCII characters (e.g. 'İ' -> "i̇"), which could
    // land the slice mid-codepoint and panic. The known extensions are
    // pure ASCII, so eq_ignore_ascii_case against the original bytes
    // avoids the mismatched-length lowercase copy entirely. Iterates in
    // ascending order and keeps the last match to preserve the original
    // rfind() (rightmost-occurrence) semantics.
    for ext in &[".xisf", ".fits", ".fts", ".fit"] {
        let ext_len = ext.len();
        let mut last_match_end: Option<usize> = None;
        for (i, _) in normalized.char_indices() {
            let end = i + ext_len;
            if end <= normalized.len()
                && normalized.is_char_boundary(end)
                && normalized[i..end].eq_ignore_ascii_case(ext)
            {
                last_match_end = Some(end);
            }
        }
        if let Some(end) = last_match_end {
            return normalized[..end].to_string();
        }
    }
// No known extension found — return as-is (original, not normalized,
    // to preserve the caller's original separator style)
    s.to_string()
}

#[cfg(test)]
mod strip_known_extension_tests {
    use super::*;

    // Issue 119: the original bug — to_lowercase() can change UTF-8 byte
    // length for some non-ASCII characters (Turkish dotted İ, U+0130,
    // lowercases to "i̇" — an "i" plus a combining dot above, U+0307 — one
    // extra byte), which could shift a match position computed on the
    // lowercased copy past a char boundary in the original string and
    // panic on slicing. This path never panics regardless of extension
    // match outcome.
    #[test]
    fn turkish_dotted_i_path_does_not_panic() {
        let path = "/data/İstanbul-M31/lights/frame001.fit.session";
        let result = strip_known_extension(path);
        assert_eq!(result, "/data/İstanbul-M31/lights/frame001.fit");
    }

    #[test]
    fn turkish_dotted_i_with_no_known_extension_returns_unchanged() {
        let path = "/data/İstanbul-M31/notes.txt";
        let result = strip_known_extension(path);
        assert_eq!(result, path);
    }

    #[test]
    fn ordinary_path_strips_session_suffix() {
        assert_eq!(strip_known_extension("frame001.fit.session"), "frame001.fit");
        assert_eq!(strip_known_extension("frame001.FIT.session"), "frame001.FIT");
        assert_eq!(strip_known_extension("stack.xisf.project"), "stack.xisf");
    }

    #[test]
    fn no_known_extension_returns_original_unchanged() {
        let path = "readme.txt";
        assert_eq!(strip_known_extension(path), path);
    }

    #[test]
    fn backslash_paths_are_normalized() {
        assert_eq!(
            strip_known_extension(r"D:\data\frame001.fit.session"),
            "D:/data/frame001.fit"
        );
    }

    #[test]
    fn picks_rightmost_extension_occurrence() {
        // A directory segment that itself contains an extension-like
        // substring shouldn't be mistaken for the real one — rfind()
        // semantics (rightmost match) preserved.
        assert_eq!(
            strip_known_extension("/data/old.fit.backups/frame002.fits.session"),
            "/data/old.fit.backups/frame002.fits"
        );
    }
}

// ----------------------------------------------------------------------
