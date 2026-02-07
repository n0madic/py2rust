use py2rust::callspec::{validate_call_shape, AritySpec, CallShape, CallShapeError, KeywordPolicy};

#[test]
fn callspec_arity_exact_range_and_at_least() {
    let exact = CallShape {
        arity: AritySpec::Exact(1),
        keywords: KeywordPolicy::None,
    };
    let range = CallShape {
        arity: AritySpec::Range { min: 1, max: 3 },
        keywords: KeywordPolicy::None,
    };
    let at_least = CallShape {
        arity: AritySpec::AtLeast(2),
        keywords: KeywordPolicy::None,
    };

    assert!(validate_call_shape("f()", exact, 1, &[]).is_ok());
    assert!(validate_call_shape("f()", range, 2, &[]).is_ok());
    assert!(validate_call_shape("f()", at_least, 4, &[]).is_ok());

    let err = validate_call_shape("f()", exact, 2, &[]).expect_err("arity mismatch expected");
    assert_eq!(err.message(), "f() expects one argument");
}

#[test]
fn callspec_keyword_policies_are_enforced() {
    let none = CallShape {
        arity: AritySpec::Any,
        keywords: KeywordPolicy::None,
    };
    let named = CallShape {
        arity: AritySpec::Any,
        keywords: KeywordPolicy::Named(&["sep", "end"]),
    };

    let err =
        validate_call_shape("print()", none, 0, &[Some("sep")]).expect_err("keyword rejected");
    assert_eq!(
        err.message(),
        "Keyword arguments are not supported for print()"
    );

    assert!(validate_call_shape("print()", named, 0, &[Some("sep")]).is_ok());

    let unknown = validate_call_shape("print()", named, 0, &[Some("bad")])
        .expect_err("unknown keyword expected");
    assert_eq!(
        unknown.message(),
        "Unknown keyword argument `bad` for print()"
    );

    let duplicate = validate_call_shape("print()", named, 0, &[Some("sep"), Some("sep")])
        .expect_err("duplicate keyword expected");
    assert_eq!(
        duplicate.message(),
        "Multiple values for keyword argument `sep`"
    );
}

#[test]
fn callspec_rejects_kwargs_unpacking() {
    let shape = CallShape {
        arity: AritySpec::Any,
        keywords: KeywordPolicy::Named(&["key"]),
    };
    let err = validate_call_shape("sorted()", shape, 1, &[None])
        .expect_err("**kwargs unpacking must be rejected");
    assert_eq!(
        err,
        CallShapeError::KeywordUnpackNotSupported {
            callable: "sorted()".to_string()
        }
    );
    assert_eq!(
        err.message(),
        "Call-site **kwargs unpacking is not supported for sorted()"
    );
}
