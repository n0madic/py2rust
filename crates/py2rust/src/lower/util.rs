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
}
