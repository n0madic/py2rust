use super::*;

impl<'a> Lowerer<'a> {
    pub(super) fn error(
        &self,
        range: rustpython_parser::text_size::TextRange,
        msg: &str,
    ) -> CompileError {
        CompileError::new(msg, Span::from(range), self.source, self.filename)
    }
}
