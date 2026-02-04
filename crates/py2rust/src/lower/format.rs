use super::*;

impl<'a> Lowerer<'a> {
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

    pub(super) fn map_format_spec(
        &self,
        spec: &str,
        range: rustpython_parser::text_size::TextRange,
    ) -> Result<String, CompileError> {
        if spec.is_empty() {
            return Ok(String::new());
        }
        if spec.contains('{') || spec.contains('}') {
            return Err(self.error(range, "f-string format spec may not contain braces"));
        }
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
