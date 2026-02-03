use super::*;

impl<'a> TypeChecker<'a> {
    pub(super) fn error(&self, span: Span, msg: impl Into<String>) -> CompileError {
        CompileError::new(msg, span, self.source, self.filename)
    }

    pub(super) fn warn(&mut self, span: Span, msg: impl Into<String>) {
        self.warnings
            .push(Warning::new(msg, span, self.source, self.filename));
    }

    pub fn take_warnings(&mut self) -> Vec<Warning> {
        std::mem::take(&mut self.warnings)
    }
}
