// pcode/expr.rs — Arithmetic and string expression evaluator
// Spec §7.3, §7.5
//
// Expressions are evaluated after variable substitution has already occurred.
// If the expression is purely a string with no arithmetic operators, it is
// returned unchanged. Arithmetic is only attempted when operands are numeric.
// The + operator concatenates strings when either operand is non-numeric.

/// Evaluate an expression string. Returns Ok(result) or Err(message).
pub fn evaluate_expr(expr: &str) -> Result<String, String> {
    let expr = expr.trim();
    if expr.is_empty() {
        return Ok(String::new());
    }
    match parse_expr(expr) {
        Ok(Value::Num(n))  => Ok(format_number(n)),
        Ok(Value::Str(s))  => Ok(s),
        Err(e)             => Err(e),
    }
}

/// Evaluate an expression and return it as a boolean for use in If conditions.
/// The caller has already done variable substitution.
pub fn evaluate_condition(expr: &str) -> Result<bool, String> {
    // Look for comparison operators (longest first to avoid mis-splitting != as !)
    for op in &["==", "!=", "<=", ">=", "<", ">"] {
        if let Some(pos) = find_comparison_op(expr, op) {
            let lhs = evaluate_expr(expr[..pos].trim())?;
            let rhs = evaluate_expr(expr[pos + op.len()..].trim())?;
            return Ok(compare_values(&lhs, op, &rhs));
        }
    }
    // No comparison operator — evaluate as a value and treat as boolean
    let val = evaluate_expr(expr)?;
    let t = val.trim();
    Ok(!t.is_empty() && t != "false" && t != "0")
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

// ── Top-level parser ──────────────────────────────────────────────────────────

fn parse_expr(input: &str) -> Result<Value, String> {
    let input = input.trim();
    // Quoted string literal — return as-is
    if (input.starts_with('"') && input.ends_with('"'))
        || (input.starts_with('\'') && input.ends_with('\''))
    {
        return Ok(Value::Str(input[1..input.len() - 1].to_string()));
    }
    let tokens = tokenize(input)?;
    let mut pos = 0;
    let val = parse_addition(&tokens, &mut pos)?;
    if pos < tokens.len() {
        return Err(format!("Unexpected token '{}' in expression", tokens[pos].display()));
    }
    Ok(val)
}

// ── Recursive descent ─────────────────────────────────────────────────────────

/// Addition and subtraction (lowest precedence)
fn parse_addition(tokens: &[Token], pos: &mut usize) -> Result<Value, String> {
    let mut left = parse_multiplication(tokens, pos)?;

    while *pos < tokens.len() {
        match &tokens[*pos] {
            Token::Op('+') => {
                *pos += 1;
                let right = parse_multiplication(tokens, pos)?;
                // If either side is non-numeric, concatenate as strings
                match (left.as_num(), right.as_num()) {
                    (Some(l), Some(r)) => left = Value::Num(l + r),
                    _ => left = Value::Str(left.as_str() + &right.as_str()),
                }
            }
            Token::Op('-') => {
                *pos += 1;
                let right = parse_multiplication(tokens, pos)?;
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

/// Multiplication and division
fn parse_multiplication(tokens: &[Token], pos: &mut usize) -> Result<Value, String> {
    let mut left = parse_exponentiation(tokens, pos)?;

    while *pos < tokens.len() {
        match &tokens[*pos] {
            Token::Op('*') => {
                *pos += 1;
                let right = parse_exponentiation(tokens, pos)?;
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
                let right = parse_exponentiation(tokens, pos)?;
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

/// Exponentiation (right-associative)
fn parse_exponentiation(tokens: &[Token], pos: &mut usize) -> Result<Value, String> {
    let base = parse_unary(tokens, pos)?;
    if *pos < tokens.len() {
        if let Token::Op('^') = &tokens[*pos] {
            *pos += 1;
            let exp = parse_exponentiation(tokens, pos)?; // right-associative
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

/// Unary minus
fn parse_unary(tokens: &[Token], pos: &mut usize) -> Result<Value, String> {
    if *pos < tokens.len() {
        if let Token::Op('-') = &tokens[*pos] {
            *pos += 1;
            let val = parse_primary(tokens, pos)?;
            let n = val.as_num().ok_or_else(|| {
                format!("Cannot negate non-numeric value '{}'", val.as_str())
            })?;
            return Ok(Value::Num(-n));
        }
    }
    parse_primary(tokens, pos)
}

/// Literals, function calls, and parenthesised sub-expressions
fn parse_primary(tokens: &[Token], pos: &mut usize) -> Result<Value, String> {
    if *pos >= tokens.len() {
        return Err("Unexpected end of expression".to_string());
    }

    match &tokens[*pos].clone() {
        Token::Num(n) => {
            *pos += 1;
            Ok(Value::Num(*n))
        }
        Token::Str(s) => {
            *pos += 1;
            Ok(Value::Str(s.clone()))
        }
        Token::Ident(name) => {
            *pos += 1;
            // Check for function call
            if *pos < tokens.len() {
                if let Token::LParen = &tokens[*pos] {
                    *pos += 1; // consume '('
                    return call_function(name, tokens, pos);
                }
            }
            // Bare identifier — treat as string (variables already substituted)
            Ok(Value::Str(name.clone()))
        }
        Token::LParen => {
            *pos += 1;
            let val = parse_addition(tokens, pos)?;
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

fn call_function(name: &str, tokens: &[Token], pos: &mut usize) -> Result<Value, String> {
    // Collect comma-separated arguments until ')'
    let mut args: Vec<Value> = Vec::new();

    if *pos < tokens.len() {
        if let Token::RParen = &tokens[*pos] {
            *pos += 1;
            // Zero-argument call — unusual but handle gracefully
        } else {
            loop {
                args.push(parse_addition(tokens, pos)?);
                if *pos >= tokens.len() {
                    return Err(format!("Missing ')' after call to {}()", name));
                }
                match &tokens[*pos] {
                    Token::Comma => { *pos += 1; }
                    Token::RParen => { *pos += 1; break; }
                    t => return Err(format!(
                        "Expected ',' or ')' in {}() but found '{}'", name, t.display()
                    )),
                }
            }
        }
    }

    let n = name.to_lowercase();
    match n.as_str() {
        "sqrt" => {
            let x = one_numeric_arg(&args, "sqrt")?;
            if x < 0.0 { return Err(format!("sqrt() of negative value {}", x)); }
            Ok(Value::Num(x.sqrt()))
        }
        "abs" => {
            let x = one_numeric_arg(&args, "abs")?;
            Ok(Value::Num(x.abs()))
        }
        "round" => {
            let x = one_numeric_arg(&args, "round")?;
            Ok(Value::Num(x.round()))
        }
        "floor" => {
            let x = one_numeric_arg(&args, "floor")?;
            Ok(Value::Num(x.floor()))
        }
        "ceil" => {
            let x = one_numeric_arg(&args, "ceil")?;
            Ok(Value::Num(x.ceil()))
        }
        "min" => {
            two_numeric_args(&args, "min").map(|(a, b)| Value::Num(a.min(b)))
        }
        "max" => {
            two_numeric_args(&args, "max").map(|(a, b)| Value::Num(a.max(b)))
        }
        _ => Err(format!("Unknown function '{}()'", name)),
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
    let a = args[0].as_num().ok_or_else(|| {
        format!("{}() arguments must be numeric", fname)
    })?;
    let b = args[1].as_num().ok_or_else(|| {
        format!("{}() arguments must be numeric", fname)
    })?;
    Ok((a, b))
}

// ── Tokenizer ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
enum Token {
    Num(f64),
    Str(String),
    Ident(String),
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
            Token::Ident(s) => s.clone(),
            Token::Op(c)    => c.to_string(),
            Token::LParen   => "(".to_string(),
            Token::RParen   => ")".to_string(),
            Token::Comma    => ",".to_string(),
        }
    }
}

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
                    return Err(format!("Unterminated string literal starting with {}", quote));
                }
                tokens.push(Token::Str(s));
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
                let n = num.parse::<f64>().map_err(|_| format!("Invalid number '{}'", num))?;
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

            // Path separators and other punctuation — collect as part of a string token
            // so that D:/Astrophotos/heatmaps passes through unchanged
            _ => {
                let mut s = String::new();
                while let Some(&sc) = chars.peek() {
                    if sc == ' ' || sc == '\t' || sc == '+' || sc == '('
                        || sc == ')' || sc == ',' {
                        break;
                    }
                    s.push(sc);
                    chars.next();
                }
                tokens.push(Token::Ident(s));
            }
        }
    }

    Ok(tokens)
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Find a comparison operator in the expression, respecting parentheses depth
/// so that operators inside sub-expressions are not matched.
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
                // Skip quoted strings
                let q = bytes[i];
                i += 1;
                while i < len && bytes[i] != q { i += 1; }
                if i < len { i += 1; }
            }
            _ => {
                if depth == 0 && expr[i..].starts_with(op) {
                    // Make sure we're not matching a prefix of a longer operator
                    // e.g. don't match '<' when the actual op is '<='
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

/// Compare two already-evaluated string values.
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

/// Format a float cleanly — drop the decimal point for whole numbers.
fn format_number(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        format!("{}", n)
    }
}

// ----------------------------------------------------------------------
