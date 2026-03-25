//! Shared call-shape validation used by registries, type checking, and codegen.
//!
//! This module centralizes arity and keyword rules so all compiler phases emit the
//! same diagnostics for the same callable surface.

use std::collections::HashSet;

use crate::hir::KeywordArg;

/// Extract keyword names from a slice of keyword arguments for call-shape validation.
pub fn keyword_names(keywords: &[KeywordArg]) -> Vec<Option<&str>> {
    keywords.iter().map(|kw| kw.name.as_deref()).collect()
}

/// Arity policy for a callable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AritySpec {
    /// Accept exactly N positional arguments.
    Exact(usize),
    /// Accept a positional range `[min, max]`.
    Range { min: usize, max: usize },
    /// Accept at least N positional arguments.
    AtLeast(usize),
    /// Accept any number of positional arguments.
    Any,
}

impl AritySpec {
    /// Return true when the provided arity is accepted.
    pub fn accepts(self, arg_count: usize) -> bool {
        match self {
            Self::Exact(expected) => arg_count == expected,
            Self::Range { min, max } => min <= arg_count && arg_count <= max,
            Self::AtLeast(min) => arg_count >= min,
            Self::Any => true,
        }
    }

    /// Render the canonical arity phrase used in diagnostics.
    pub fn describe(self) -> String {
        match self {
            Self::Exact(0) => "no arguments".to_string(),
            Self::Exact(1) => "one argument".to_string(),
            Self::Exact(2) => "two arguments".to_string(),
            Self::Exact(n) => format!("{n} arguments"),
            Self::Range { min: 0, max: 1 } => "zero or one argument".to_string(),
            Self::Range { min: 1, max: 2 } => "one or two arguments".to_string(),
            Self::Range { min, max } => format!("between {min} and {max} arguments"),
            Self::AtLeast(1) => "at least one argument".to_string(),
            Self::AtLeast(min) => format!("at least {min} arguments"),
            Self::Any => "a valid number of arguments".to_string(),
        }
    }
}

/// Keyword-argument policy for a callable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeywordPolicy {
    /// No keyword arguments allowed.
    None,
    /// Any keyword arguments are allowed.
    Any,
    /// Only the listed keyword names are allowed.
    Named(&'static [&'static str]),
}

impl KeywordPolicy {
    /// Return true when the provided keyword name is accepted by policy.
    pub fn allows_name(self, name: &str) -> bool {
        match self {
            Self::None => false,
            Self::Any => true,
            Self::Named(allowed) => allowed.contains(&name),
        }
    }
}

/// Shape metadata attached to a callable spec.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CallShape {
    /// Positional arity policy.
    pub arity: AritySpec,
    /// Keyword policy.
    pub keywords: KeywordPolicy,
}

impl CallShape {
    /// Build an arity diagnostic for the provided callable display name.
    pub fn arity_message(self, callable: &str) -> String {
        format!("{callable} expects {}", self.arity.describe())
    }

    /// Build a keyword-policy diagnostic for the provided callable display name.
    pub fn keyword_message(self, callable: &str) -> String {
        match self.keywords {
            KeywordPolicy::None => {
                format!("Keyword arguments are not supported for {callable}")
            }
            KeywordPolicy::Any => {
                format!("Keyword arguments are supported for {callable}")
            }
            KeywordPolicy::Named(names) => {
                format!(
                    "Keyword arguments for {callable} are limited to: {}",
                    names.join(", ")
                )
            }
        }
    }
}

/// Error from call-shape validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallShapeError {
    /// Arity does not satisfy the callable shape.
    ArityMismatch {
        callable: String,
        expected: String,
        actual: usize,
    },
    /// Keyword arguments are not accepted by policy.
    KeywordsNotAllowed { callable: String },
    /// The call contains a `**kwargs` unpacking entry.
    KeywordUnpackNotSupported { callable: String },
    /// Duplicate keyword argument appears at the call site.
    DuplicateKeyword { callable: String, keyword: String },
    /// A keyword name is not accepted by policy.
    UnknownKeyword { callable: String, keyword: String },
}

impl CallShapeError {
    /// Convert the validation error to a canonical user-facing message.
    pub fn message(&self) -> String {
        match self {
            Self::ArityMismatch {
                callable,
                expected,
                actual,
            } => {
                if expected.starts_with("at least ") {
                    format!("{callable} expects {expected} (got {actual})")
                } else {
                    format!("{callable} expects {expected}")
                }
            }
            Self::KeywordsNotAllowed { callable } => {
                format!("Keyword arguments are not supported for {callable}")
            }
            Self::KeywordUnpackNotSupported { callable } => {
                format!("Call-site **kwargs unpacking is not supported for {callable}")
            }
            Self::DuplicateKeyword { keyword, .. } => {
                format!("Multiple values for keyword argument `{keyword}`")
            }
            Self::UnknownKeyword { callable, keyword } => {
                format!("Unknown keyword argument `{keyword}` for {callable}")
            }
        }
    }
}

/// Validate positional/keyword call shape for a callable.
///
/// `callable` should include the suffix `()` (for example `"sorted()"` or
/// `"list.append()"`) so diagnostics are stable across callers.
pub fn validate_call_shape(
    callable: &str,
    shape: CallShape,
    positional_args: usize,
    keyword_names: &[Option<&str>],
) -> Result<(), CallShapeError> {
    if !shape.arity.accepts(positional_args) {
        return Err(CallShapeError::ArityMismatch {
            callable: callable.to_string(),
            expected: shape.arity.describe(),
            actual: positional_args,
        });
    }

    if keyword_names.is_empty() {
        return Ok(());
    }

    match shape.keywords {
        KeywordPolicy::None => {
            return Err(CallShapeError::KeywordsNotAllowed {
                callable: callable.to_string(),
            });
        }
        KeywordPolicy::Any => {
            // Even with `Any`, duplicate names remain an error for Python-compatible calls.
            let mut seen = HashSet::new();
            for name in keyword_names {
                let Some(name) = name else {
                    return Err(CallShapeError::KeywordUnpackNotSupported {
                        callable: callable.to_string(),
                    });
                };
                if !seen.insert((*name).to_string()) {
                    return Err(CallShapeError::DuplicateKeyword {
                        callable: callable.to_string(),
                        keyword: (*name).to_string(),
                    });
                }
            }
            return Ok(());
        }
        KeywordPolicy::Named(allowed) => {
            let mut seen = HashSet::new();
            for name in keyword_names {
                let Some(name) = name else {
                    return Err(CallShapeError::KeywordUnpackNotSupported {
                        callable: callable.to_string(),
                    });
                };
                if !seen.insert((*name).to_string()) {
                    return Err(CallShapeError::DuplicateKeyword {
                        callable: callable.to_string(),
                        keyword: (*name).to_string(),
                    });
                }
                if !allowed.contains(name) {
                    return Err(CallShapeError::UnknownKeyword {
                        callable: callable.to_string(),
                        keyword: (*name).to_string(),
                    });
                }
            }
        }
    }

    Ok(())
}
