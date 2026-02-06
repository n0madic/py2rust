// Python format() literal rewrite helpers.

use super::super::*;

/// Parsed format-literal rewrite output:
/// rewritten Rust format string + placeholder spec metadata.
pub(super) type FormatLiteralRewrite = (
    String,
    std::collections::HashSet<usize>,
    std::collections::HashMap<String, bool>,
);

impl<'a> Codegen<'a> {
    /// Render one `.format(...)` argument with Python-like default string conversion.
    pub(super) fn gen_format_arg_expr(
        &mut self,
        arg: &Expr,
        has_spec: bool,
    ) -> Result<String, CompileError> {
        if has_spec {
            return self.gen_expr(arg);
        }
        if matches!(arg.ty.as_ref(), Some(Type::List(_))) {
            return self.list_str_expr(arg);
        }
        if matches!(arg.ty.as_ref(), Some(Type::Tuple(_))) {
            self.uses.py_list_str = true;
            return Ok(format!("{}.py_repr()", self.gen_expr(arg)?));
        }
        if matches!(arg.ty.as_ref(), Some(Type::Float)) {
            self.uses.py_float_str = true;
            return Ok(format!("py_float_str({})", self.gen_expr(arg)?));
        }
        if matches!(arg.ty.as_ref(), Some(Type::Bool)) {
            let rendered = self.gen_expr(arg)?;
            return Ok(format!(
                "if {} {{ \"True\".to_string() }} else {{ \"False\".to_string() }}",
                rendered
            ));
        }
        if matches!(arg.ty.as_ref(), Some(Type::None)) {
            return Ok("\"None\".to_string()".to_string());
        }
        if self.print_needs_debug(arg) {
            let arg_expr = self.debug_arg_expr(arg)?;
            return Ok(format!("format!(\"{{:?}}\", {})", arg_expr));
        }
        self.gen_expr(arg)
    }

    /// Rewrite a Python format literal to Rust format syntax and collect spec usage metadata.
    pub(super) fn rewrite_python_format_literal(
        &self,
        fmt: &str,
        span: Span,
    ) -> Result<FormatLiteralRewrite, CompileError> {
        let chars: Vec<char> = fmt.chars().collect();
        let mut out = String::new();
        let mut i = 0usize;
        let mut auto_index = 0usize;
        let mut positional_with_spec = std::collections::HashSet::new();
        let mut named_with_spec = std::collections::HashMap::new();

        while i < chars.len() {
            let ch = chars[i];
            if ch == '{' {
                if i + 1 < chars.len() && chars[i + 1] == '{' {
                    out.push_str("{{");
                    i += 2;
                    continue;
                }
                i += 1;
                let mut field = String::new();
                while i < chars.len() && chars[i] != '}' {
                    field.push(chars[i]);
                    i += 1;
                }
                if i >= chars.len() {
                    return Err(self.error(span, "Unmatched '{' in format string"));
                }
                i += 1;

                let (field_name, spec_raw) = if let Some((name, spec)) = field.split_once(':') {
                    (name, Some(spec))
                } else {
                    (field.as_str(), None)
                };
                let mapped_spec = spec_raw
                    .map(Self::map_python_format_spec)
                    .unwrap_or_default();
                let has_spec = !mapped_spec.is_empty();

                if field_name.is_empty() {
                    if has_spec {
                        positional_with_spec.insert(auto_index);
                    }
                    auto_index += 1;
                } else if field_name.chars().all(|c| c.is_ascii_digit()) {
                    if let Ok(pos) = field_name.parse::<usize>() {
                        if has_spec {
                            positional_with_spec.insert(pos);
                        }
                    }
                } else if Self::is_simple_format_name(field_name) {
                    named_with_spec
                        .entry(field_name.to_string())
                        .and_modify(|v| *v |= has_spec)
                        .or_insert(has_spec);
                }

                out.push('{');
                out.push_str(field_name);
                if spec_raw.is_some() {
                    out.push(':');
                    out.push_str(&mapped_spec);
                }
                out.push('}');
                continue;
            }
            if ch == '}' {
                if i + 1 < chars.len() && chars[i + 1] == '}' {
                    out.push_str("}}");
                    i += 2;
                    continue;
                }
                return Err(self.error(span, "Unmatched '}' in format string"));
            }
            out.push(ch);
            i += 1;
        }

        Ok((out, positional_with_spec, named_with_spec))
    }

    /// Map Python `str.format` type suffixes to Rust's formatting syntax.
    fn map_python_format_spec(spec: &str) -> String {
        if spec.is_empty() {
            return String::new();
        }
        let Some(last) = spec.chars().last() else {
            return String::new();
        };
        if !matches!(last, 'd' | 'f' | 'x' | 'X' | 'o' | 'b') {
            return spec.to_string();
        }
        let cut = spec.len() - last.len_utf8();
        let body = &spec[..cut];
        match last {
            'd' => body.to_string(),
            'f' => {
                if body.is_empty() {
                    ".6".to_string()
                } else {
                    body.to_string()
                }
            }
            'x' | 'X' | 'o' | 'b' => spec.to_string(),
            _ => spec.to_string(),
        }
    }

    /// Decide whether a format field name can be treated as a named keyword.
    fn is_simple_format_name(name: &str) -> bool {
        let mut chars = name.chars();
        let Some(first) = chars.next() else {
            return false;
        };
        if !(first == '_' || first.is_ascii_alphabetic()) {
            return false;
        }
        chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
    }
}
