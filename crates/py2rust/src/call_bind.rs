//! Shared call-argument binding planner.
//!
//! The planner resolves positional/keyword argument placement against parameter
//! metadata for the non-unpacking path. Type checking and codegen both consume
//! this to avoid drifting call-binding semantics.

use crate::hir::ParamKind;
use std::collections::HashSet;

/// Source location for a bound argument.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoundArg {
    /// Bound from positional argument index.
    Positional(usize),
    /// Bound from keyword argument index.
    Keyword(usize),
}

/// Fully resolved binding plan for a call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundCallPlan {
    /// Direct bindings for regular parameters (same length as parameter list).
    pub bound: Vec<Option<BoundArg>>,
    /// Parameter indices that accept positional values (positional-only and positional-or-keyword).
    pub positional_params: Vec<usize>,
    /// Index of `*args` parameter, if present.
    pub vararg_idx: Option<usize>,
    /// Index of `**kwargs` parameter, if present.
    pub varkw_idx: Option<usize>,
    /// Positional argument indices consumed by `*args`.
    pub vararg_positional: Vec<usize>,
    /// Keyword argument indices consumed by `**kwargs`.
    pub varkw_keywords: Vec<usize>,
}

/// Binding-plan error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BindError {
    /// Parameter metadata vectors are not aligned.
    MalformedSignature,
    /// Non-unpacking path received a `**kwargs` unpacking token.
    KeywordUnpackUnsupported,
    /// Same keyword appears multiple times.
    DuplicateKeyword { keyword: String },
    /// Keyword does not match any parameter and there is no `**kwargs`.
    UnknownKeyword { keyword: String },
    /// Unexpected keyword for implicit-first-arg path.
    UnexpectedKeyword { keyword: String },
    /// Keyword was used for a positional-only parameter.
    PositionalOnlyAsKeyword { keyword: String },
    /// Too many positional arguments with no `*args`.
    ArgumentCountMismatch,
    /// Required argument is missing.
    MissingRequired { name: String },
}

impl BindError {
    /// Render stable diagnostics used by both typecheck and codegen.
    pub fn message(&self) -> String {
        match self {
            Self::MalformedSignature => "Internal error: malformed function signature".to_string(),
            Self::KeywordUnpackUnsupported => {
                "Call-site **kwargs unpacking is not supported in this context yet".to_string()
            }
            Self::DuplicateKeyword { keyword } => {
                format!("Multiple values for argument `{keyword}`")
            }
            Self::UnknownKeyword { keyword } => format!("Unknown keyword argument `{keyword}`"),
            Self::UnexpectedKeyword { keyword } => {
                format!("Unexpected keyword argument `{keyword}`")
            }
            Self::PositionalOnlyAsKeyword { keyword } => {
                format!("Positional-only argument passed as keyword: `{keyword}`")
            }
            Self::ArgumentCountMismatch => "Argument count mismatch".to_string(),
            Self::MissingRequired { name } => format!("Missing required argument `{name}`"),
        }
    }
}

/// Build a non-unpacking call-binding plan.
///
/// `keyword_names` must align with call-site keyword argument order.
pub fn plan_non_unpacking_bind(
    param_names: &[String],
    param_kinds: &[ParamKind],
    has_defaults: &[bool],
    positional_len: usize,
    keyword_names: &[Option<&str>],
    allow_implicit_first: bool,
) -> Result<BoundCallPlan, BindError> {
    let param_len = param_names.len();
    if param_kinds.len() != param_len || has_defaults.len() != param_len {
        return Err(BindError::MalformedSignature);
    }

    let mut positional_params = Vec::new();
    let mut vararg_idx = None;
    let mut varkw_idx = None;
    for (idx, kind) in param_kinds.iter().enumerate() {
        match kind {
            ParamKind::PositionalOnly | ParamKind::PositionalOrKeyword => {
                positional_params.push(idx)
            }
            ParamKind::VarArgs => vararg_idx = Some(idx),
            ParamKind::KeywordOnly => {}
            ParamKind::VarKeywords => varkw_idx = Some(idx),
        }
    }

    let mut bound = vec![None; param_len];
    let mut positional_cursor = 0usize;
    if allow_implicit_first && !positional_params.is_empty() && positional_params[0] == 0 {
        positional_cursor = 1;
    }

    let mut vararg_positional = Vec::new();
    for pos_idx in 0..positional_len {
        if positional_cursor < positional_params.len() {
            let param_idx = positional_params[positional_cursor];
            bound[param_idx] = Some(BoundArg::Positional(pos_idx));
            positional_cursor += 1;
        } else if vararg_idx.is_some() {
            vararg_positional.push(pos_idx);
        } else {
            return Err(BindError::ArgumentCountMismatch);
        }
    }

    let mut varkw_keywords = Vec::new();
    let mut seen_kw = HashSet::new();
    for (kw_idx, kw_name) in keyword_names.iter().enumerate() {
        let Some(kw_name) = kw_name else {
            return Err(BindError::KeywordUnpackUnsupported);
        };
        if !seen_kw.insert((*kw_name).to_string()) {
            return Err(BindError::DuplicateKeyword {
                keyword: (*kw_name).to_string(),
            });
        }
        if param_names.iter().enumerate().any(|(idx, name)| {
            *name == *kw_name && matches!(param_kinds[idx], ParamKind::PositionalOnly)
        }) {
            return Err(BindError::PositionalOnlyAsKeyword {
                keyword: (*kw_name).to_string(),
            });
        }
        let direct_param = param_names
            .iter()
            .enumerate()
            .find(|(idx, name)| {
                **name == *kw_name
                    && matches!(
                        param_kinds[*idx],
                        ParamKind::PositionalOrKeyword | ParamKind::KeywordOnly
                    )
            })
            .map(|(idx, _)| idx);
        if let Some(param_idx) = direct_param {
            if allow_implicit_first && param_idx == 0 && positional_params.first() == Some(&0) {
                return Err(BindError::UnexpectedKeyword {
                    keyword: (*kw_name).to_string(),
                });
            }
            if bound[param_idx].is_some() {
                return Err(BindError::DuplicateKeyword {
                    keyword: (*kw_name).to_string(),
                });
            }
            bound[param_idx] = Some(BoundArg::Keyword(kw_idx));
        } else if varkw_idx.is_some() {
            varkw_keywords.push(kw_idx);
        } else {
            return Err(BindError::UnknownKeyword {
                keyword: (*kw_name).to_string(),
            });
        }
    }

    for idx in 0..param_len {
        if !matches!(
            param_kinds[idx],
            ParamKind::PositionalOnly | ParamKind::PositionalOrKeyword | ParamKind::KeywordOnly
        ) {
            continue;
        }
        if allow_implicit_first && idx == 0 && positional_params.first() == Some(&0) {
            continue;
        }
        if bound[idx].is_none() && !has_defaults[idx] {
            let name = param_names
                .get(idx)
                .cloned()
                .unwrap_or_else(|| format!("arg{idx}"));
            return Err(BindError::MissingRequired { name });
        }
    }

    Ok(BoundCallPlan {
        bound,
        positional_params,
        vararg_idx,
        varkw_idx,
        vararg_positional,
        varkw_keywords,
    })
}
