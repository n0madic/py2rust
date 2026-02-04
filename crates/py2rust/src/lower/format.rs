use super::*;

/// F-string (formatted string literal) lowering.
///
/// Python f-strings: `f"Hello {name}!"`
/// Are lowered to Rust format! macro calls: `format!("Hello {}!", name)`
///
/// This module handles the complex task of converting Python's f-string syntax
/// to Rust's format! macro, which has different rules:
///
/// 1. **Brace escaping**: In Python f-strings, {{ and }} represent literal braces.
///    In Rust format!, the same syntax is used. We preserve this.
///
/// 2. **Format specifiers**: Python uses : for format specs (e.g., {x:.2f})
///    Rust uses : similarly but with different syntax. We map common patterns.
///
/// 3. **Expressions**: Python allows arbitrary expressions in {}
///    Rust's format! only allows variable names and simple paths.
///    We lower the expression separately and bind it to a temp variable if needed.
///
/// Format spec mapping examples:
/// - Python {x:.2f} -> Rust {:.2}
/// - Python {x:d} -> Rust {} (integer formatting is default)
/// - Python {x:x} -> Rust {:x} (hex)
/// - Python {x:b} -> Rust {:b} (binary)

impl<'a> Lowerer<'a> {
    /// Escape literal braces in format strings.
    ///
    /// Both Python and Rust use {{ and }} for literal braces in format strings,
    /// so we convert each { to {{ and each } to }}.
    pub(super) fn escape_format_literal(&self, s: &str) -> String {
        let mut out = String::new();
        for ch in s.chars() {
            match ch {
                '{' => out.push_str("{{"),
                '}' => out.push_str("}}"),
                _ => out.push(ch),
            }
        }
        out
    }

    pub(super) fn format_spec_literal(&self, expr: &ast::Expr) -> Result<String, CompileError> {
        match expr {
            ast::Expr::Constant(cons) => match &cons.value {
                ast::Constant::Str(s) => Ok(s.clone()),
                _ => Err(self.error(expr.range(), "f-string format spec must be a literal")),
            },
            ast::Expr::JoinedStr(joined) => {
                let mut out = String::new();
                for value in &joined.values {
                    match value {
                        ast::Expr::Constant(cons) => match &cons.value {
                            ast::Constant::Str(s) => out.push_str(s),
                            _ => {
                                return Err(self.error(
                                    value.range(),
                                    "f-string format spec must be a literal",
                                ))
                            }
                        },
                        _ => {
                            return Err(
                                self.error(value.range(), "f-string format spec must be a literal")
                            )
                        }
                    }
                }
                Ok(out)
            }
            _ => Err(self.error(expr.range(), "f-string format spec must be a literal")),
        }
    }

    /// Map Python format specifier to Rust format specifier.
    ///
    /// Python format spec syntax: [[fill]align][sign][#][0][width][,][.precision][type]
    /// Rust format spec syntax: [[fill]align][sign][#][0][width][.precision][type]
    ///
    /// We support a subset:
    /// - width: number of characters (both Python and Rust)
    /// - .precision: decimal places for floats (both)
    /// - type: f (float), d (decimal int), x/X (hex), o (octal), b (binary)
    ///
    /// Examples:
    /// - ".2f" -> ".2" (Rust infers float formatting)
    /// - "10d" -> "10" (Rust infers int formatting)
    /// - "x" -> "x" (hex formatting)
    pub(super) fn map_format_spec(
        &self,
        spec: &str,
        range: rustpython_parser::text_size::TextRange,
    ) -> Result<String, CompileError> {
        if spec.is_empty() {
            return Ok(String::new());
        }
        // Format specs can't contain braces (they'd be confused with expressions)
        if spec.contains('{') || spec.contains('}') {
            return Err(self.error(range, "f-string format spec may not contain braces"));
        }

        // Extract type character (last char if it's a type indicator)
        let last = spec.chars().last();
        let (body, ty) = if let Some(ch) = last {
            if matches!(ch, 'f' | 'd' | 'x' | 'X' | 'o' | 'b') {
                let cut = spec.len() - ch.len_utf8();
                (&spec[..cut], Some(ch))
            } else {
                (spec, None)
            }
        } else {
            ("", None)
        };
        let mut out = String::new();
        if !body.is_empty() {
            let mut parts = body.splitn(2, '.');
            let width = parts.next().unwrap_or("");
            if !width.is_empty() && !width.chars().all(|c| c.is_ascii_digit()) {
                return Err(self.error(range, "Unsupported f-string format specifier"));
            }
            out.push_str(width);
            if let Some(prec) = parts.next() {
                if !prec.chars().all(|c| c.is_ascii_digit()) {
                    return Err(self.error(range, "Unsupported f-string format specifier"));
                }
                out.push('.');
                out.push_str(prec);
            }
        }
        if let Some(ty) = ty {
            match ty {
                'f' => {
                    if out.is_empty() {
                        out.push_str(".6");
                    }
                }
                'd' => {}
                'x' | 'X' | 'o' | 'b' => out.push(ty),
                _ => {}
            }
        }
        Ok(out)
    }
}
