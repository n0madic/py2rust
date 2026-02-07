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
}
