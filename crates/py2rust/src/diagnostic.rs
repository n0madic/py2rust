#![allow(unused_assignments)]

use crate::span::Span;
use miette::{Diagnostic, NamedSource, SourceSpan};
use thiserror::Error;

/// Compile error with rich source context.
///
/// We use the `miette` crate for error reporting because it provides:
/// 1. Beautiful terminal output with source snippets and underlining
/// 2. Support for multiple labels and help text
/// 3. Integration with the Diagnostic trait for composability
///
/// The #[derive(Diagnostic)] macro generates the reporting boilerplate.
/// Each field is annotated with how it should appear in the error:
/// - #[error] - The main error message
/// - #[label] - Underlines the problematic span in source
/// - #[source_code] - Provides the source text for context
/// - #[help] - Optional suggestion for fixing the error
#[derive(Debug, Error, Diagnostic)]
#[error("{message}")]
pub struct CompileError {
    pub message: String,
    #[label("{label}")]
    pub span: SourceSpan,
    #[source_code]
    pub src: NamedSource,
    #[help]
    pub help: Option<String>,
    pub label: String,
}

impl CompileError {
    pub fn new(message: impl Into<String>, span: Span, source: &str, filename: &str) -> Self {
        let message = message.into();
        let label = "here".to_string();
        let src = NamedSource::new(filename, source.to_string());
        let span = SourceSpan::new(span.start.into(), span.len().into());
        Self {
            message,
            span,
            src,
            help: None,
            label,
        }
    }

    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }
}

/// Compiler warning with rich source context.
///
/// Identical to CompileError but with severity level set to "warning".
/// This affects how miette displays it (typically in yellow instead of red).
///
/// Warnings are for code that compiles but might not behave as expected,
/// such as unused variables or potential type mismatches that we can't
/// definitively prove are errors.
#[derive(Debug, Error, Diagnostic)]
#[error("{message}")]
#[diagnostic(severity(warning))]
pub struct Warning {
    pub message: String,
    #[label("{label}")]
    pub span: SourceSpan,
    #[source_code]
    pub src: NamedSource,
    #[help]
    pub help: Option<String>,
    pub label: String,
}

impl Warning {
    pub fn new(message: impl Into<String>, span: Span, source: &str, filename: &str) -> Self {
        let message = message.into();
        let label = "here".to_string();
        let src = NamedSource::new(filename, source.to_string());
        let span = SourceSpan::new(span.start.into(), span.len().into());
        Self {
            message,
            span,
            src,
            help: None,
            label,
        }
    }

    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }
}
