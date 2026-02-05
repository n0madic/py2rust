use super::*;

impl<'a> Lowerer<'a> {
    /// Create a CompileError with source context for error reporting.
    ///
    /// This helper is used throughout lowering to generate user-friendly errors
    /// that include the source location and a snippet of the problematic code.
    pub(super) fn error(
        &self,
        range: rustpython_parser::text_size::TextRange,
        msg: &str,
    ) -> CompileError {
        CompileError::new(msg, Span::from(range), self.source, self.filename)
    }

    /// Escape identifiers that collide with Rust reserved keywords.
    ///
    /// We preserve Python naming semantics by using Rust raw identifiers (`r#name`).
    pub(super) fn ident(&self, raw: &str) -> String {
        if matches!(raw, "self" | "super" | "type") {
            // Keep special Python names unescaped so typecheck can resolve
            // method/self semantics and builtins like super()/type().
            return raw.to_string();
        }
        if Self::is_rust_keyword(raw) {
            format!("r#{raw}")
        } else {
            raw.to_string()
        }
    }

    fn is_rust_keyword(raw: &str) -> bool {
        matches!(
            raw,
            "as" | "break"
                | "const"
                | "continue"
                | "crate"
                | "else"
                | "enum"
                | "extern"
                | "false"
                | "fn"
                | "for"
                | "if"
                | "impl"
                | "in"
                | "let"
                | "loop"
                | "match"
                | "mod"
                | "move"
                | "mut"
                | "pub"
                | "ref"
                | "return"
                | "self"
                | "Self"
                | "static"
                | "struct"
                | "super"
                | "trait"
                | "true"
                | "type"
                | "unsafe"
                | "use"
                | "where"
                | "while"
                | "async"
                | "await"
                | "dyn"
                | "abstract"
                | "become"
                | "box"
                | "do"
                | "final"
                | "macro"
                | "override"
                | "priv"
                | "typeof"
                | "unsized"
                | "virtual"
                | "yield"
                | "try"
        )
    }
}
