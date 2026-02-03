#![allow(unused_assignments)]

use crate::span::Span;
use miette::{Diagnostic, NamedSource, SourceSpan};
use thiserror::Error;

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
