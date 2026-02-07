use py2rust::call_bind::{plan_non_unpacking_bind, BindError, BoundArg};
use py2rust::hir::ParamKind;

#[test]
fn call_bind_resolves_positional_keyword_and_variadics() {
    let names = vec![
        "x".to_string(),
        "y".to_string(),
        "args".to_string(),
        "rest".to_string(),
    ];
    let kinds = vec![
        ParamKind::PositionalOrKeyword,
        ParamKind::KeywordOnly,
        ParamKind::VarArgs,
        ParamKind::VarKeywords,
    ];
    let has_defaults = vec![false, false, false, false];
    let keywords = vec![Some("y"), Some("extra")];

    let plan = plan_non_unpacking_bind(&names, &kinds, &has_defaults, 3, &keywords, false)
        .expect("binding should succeed");

    assert_eq!(plan.bound[0], Some(BoundArg::Positional(0)));
    assert_eq!(plan.bound[1], Some(BoundArg::Keyword(0)));
    assert_eq!(plan.vararg_idx, Some(2));
    assert_eq!(plan.varkw_idx, Some(3));
    assert_eq!(plan.vararg_positional, vec![1, 2]);
    assert_eq!(plan.varkw_keywords, vec![1]);
}

#[test]
fn call_bind_detects_missing_required() {
    let names = vec!["x".to_string(), "y".to_string()];
    let kinds = vec![
        ParamKind::PositionalOrKeyword,
        ParamKind::PositionalOrKeyword,
    ];
    let has_defaults = vec![false, false];
    let err = plan_non_unpacking_bind(&names, &kinds, &has_defaults, 1, &[], false)
        .expect_err("missing required argument expected");
    assert_eq!(
        err,
        BindError::MissingRequired {
            name: "y".to_string()
        }
    );
    assert_eq!(err.message(), "Missing required argument `y`");
}

#[test]
fn call_bind_implicit_first_disallows_keyword_override() {
    let names = vec!["self".to_string(), "x".to_string()];
    let kinds = vec![
        ParamKind::PositionalOrKeyword,
        ParamKind::PositionalOrKeyword,
    ];
    let has_defaults = vec![false, false];
    let err = plan_non_unpacking_bind(&names, &kinds, &has_defaults, 0, &[Some("self")], true)
        .expect_err("unexpected self keyword expected");
    assert_eq!(
        err,
        BindError::UnexpectedKeyword {
            keyword: "self".to_string()
        }
    );
    assert_eq!(err.message(), "Unexpected keyword argument `self`");
}

#[test]
fn call_bind_rejects_duplicate_keyword() {
    let names = vec!["x".to_string()];
    let kinds = vec![ParamKind::PositionalOrKeyword];
    let has_defaults = vec![false];
    let err = plan_non_unpacking_bind(
        &names,
        &kinds,
        &has_defaults,
        0,
        &[Some("x"), Some("x")],
        false,
    )
    .expect_err("duplicate keyword expected");
    assert_eq!(
        err,
        BindError::DuplicateKeyword {
            keyword: "x".to_string()
        }
    );
    assert_eq!(err.message(), "Multiple values for argument `x`");
}

#[test]
fn call_bind_rejects_unknown_keyword_without_varkw() {
    let names = vec!["x".to_string()];
    let kinds = vec![ParamKind::PositionalOrKeyword];
    let has_defaults = vec![false];
    let err = plan_non_unpacking_bind(&names, &kinds, &has_defaults, 0, &[Some("y")], false)
        .expect_err("unknown keyword expected");
    assert_eq!(
        err,
        BindError::UnknownKeyword {
            keyword: "y".to_string()
        }
    );
    assert_eq!(err.message(), "Unknown keyword argument `y`");
}

#[test]
fn call_bind_rejects_keyword_unpack_token() {
    let names = vec!["x".to_string()];
    let kinds = vec![ParamKind::PositionalOrKeyword];
    let has_defaults = vec![false];
    let err = plan_non_unpacking_bind(&names, &kinds, &has_defaults, 0, &[None], false)
        .expect_err("keyword unpack token should be rejected");
    assert_eq!(err, BindError::KeywordUnpackUnsupported);
    assert_eq!(
        err.message(),
        "Call-site **kwargs unpacking is not supported in this context yet"
    );
}

#[test]
fn call_bind_rejects_extra_positionals_without_varargs() {
    let names = vec!["x".to_string()];
    let kinds = vec![ParamKind::PositionalOrKeyword];
    let has_defaults = vec![false];
    let err = plan_non_unpacking_bind(&names, &kinds, &has_defaults, 2, &[], false)
        .expect_err("argument count mismatch expected");
    assert_eq!(err, BindError::ArgumentCountMismatch);
    assert_eq!(err.message(), "Argument count mismatch");
}

#[test]
fn call_bind_rejects_malformed_signature_metadata() {
    let names = vec!["x".to_string()];
    let kinds = vec![ParamKind::PositionalOrKeyword, ParamKind::VarArgs];
    let has_defaults = vec![false];
    let err = plan_non_unpacking_bind(&names, &kinds, &has_defaults, 0, &[], false)
        .expect_err("malformed signature metadata expected");
    assert_eq!(err, BindError::MalformedSignature);
    assert_eq!(
        err.message(),
        "Internal error: malformed function signature"
    );
}
