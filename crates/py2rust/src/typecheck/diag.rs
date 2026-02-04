use super::*;

/// Diagnostic utilities for type checking.
///
/// These helpers create CompileError and Warning objects with proper
/// source location information for display with miette.
///
/// Why separate error/warn methods?
/// - Errors stop compilation immediately
/// - Warnings are collected and shown but don't prevent code generation
/// - Both need access to source text and filename for rich reporting

impl<'a> TypeChecker<'a> {
    /// Create a type checking error.
    ///
    /// Includes source location for miette's fancy error display.
    pub(super) fn error(&self, span: Span, msg: impl Into<String>) -> CompileError {
        CompileError::new(msg, span, self.source, self.filename)
    }

    /// Record a type checking warning.
    ///
    /// Warnings don't stop compilation but are shown to the user.
    /// Example: division by zero (might be intentional for infinity).
    pub(super) fn warn(&mut self, span: Span, msg: impl Into<String>) {
        self.warnings
            .push(Warning::new(msg, span, self.source, self.filename));
    }

    /// Extract all accumulated warnings.
    ///
    /// Called by the main compilation pipeline to display warnings.
    pub fn take_warnings(&mut self) -> Vec<Warning> {
        std::mem::take(&mut self.warnings)
    }
}
