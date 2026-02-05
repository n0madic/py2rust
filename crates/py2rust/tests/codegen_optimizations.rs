use py2rust::{compile, CompileOptions};

#[test]
fn print_list_literal_uses_vec_repr() {
    // List literals printed directly should use the Vec-based repr helper.
    let source = r#"
print([1, 2, 3])
"#;
    let out =
        compile(source, "test.py", &CompileOptions::default()).expect("compile should succeed");
    assert!(
        out.rust.contains("py_list_str_vec"),
        "Expected Vec-based list repr"
    );
    assert!(
        !out.rust
            .contains("Arc::new(Mutex::new(vec![1i64, 2i64, 3i64])"),
        "List literal should not be wrapped in Arc<Mutex<..>>"
    );
}

#[test]
fn enumerate_local_list_uses_index_iter() {
    // Local lists passed to enumerate() should use index-based iteration.
    let source = r#"

def f() -> int:
    lst: list[int] = [1, 2, 3]
    s: int = 0
    for i, v in enumerate(lst):
        s = s + i + v
    return s
"#;
    let out =
        compile(source, "test.py", &CompileOptions::default()).expect("compile should succeed");
    assert!(
        out.rust.contains("std::iter::from_fn"),
        "Expected index-based iterator"
    );
    assert!(
        out.rust.contains("&lst"),
        "Expected iterator to borrow the list"
    );
    assert!(
        !out.rust.contains("lst.clone()"),
        "Local list should not be cloned for enumerate"
    );
}

#[test]
fn reversed_local_list_uses_index_iter() {
    // reversed() on a local list should avoid collecting into a new Vec.
    let source = r#"

def f() -> int:
    lst: list[int] = [1, 2, 3]
    s: int = 0
    for v in reversed(lst):
        s = s + v
    return s
"#;
    let out =
        compile(source, "test.py", &CompileOptions::default()).expect("compile should succeed");
    assert!(
        out.rust.contains("std::iter::from_fn"),
        "Expected index-based reverse iterator"
    );
    assert!(
        !out.rust
            .contains("iter().rev().cloned().collect::<Vec<_>>()"),
        "Should not collect reversed list into a new Vec"
    );
}

#[test]
fn range_for_loop_avoids_into_iter() {
    // Range iterators should not add redundant .into_iter().
    let source = r#"

def f() -> int:
    s: int = 0
    for i in range(5):
        s = s + i
    return s
"#;
    let out =
        compile(source, "test.py", &CompileOptions::default()).expect("compile should succeed");
    assert!(
        out.rust.contains("for i in py_range(5i64)"),
        "Expected direct range iteration"
    );
    assert!(
        !out.rust.contains("py_range(5i64).into_iter()"),
        "Should not add .into_iter() to ranges"
    );
}

#[test]
fn all_any_copy_types_skip_clone() {
    // all/any over Copy types should avoid cloning in the predicate.
    let source = r#"

def f() -> bool:
    return all([1, 2, 3]) and any([0, 1])
"#;
    let out =
        compile(source, "test.py", &CompileOptions::default()).expect("compile should succeed");
    assert!(
        out.rust.contains(".all(|v|"),
        "Expected all() to use iterator predicate"
    );
    assert!(
        out.rust.contains(".any(|v|"),
        "Expected any() to use iterator predicate"
    );
    assert!(
        !out.rust.contains("let v = v.clone()"),
        "Copy types should not be cloned in all/any"
    );
}

#[test]
fn format_literal_prefix_in_string_add() {
    // String concatenation with a literal should emit a literal-prefixed format!.
    let source = r#"

def greet(name: str) -> str:
    return "hello " + name
"#;
    let out =
        compile(source, "test.py", &CompileOptions::default()).expect("compile should succeed");
    assert!(
        out.rust.contains("format!(\"hello {}\""),
        "Expected literal-prefixed format!"
    );
    assert!(
        !out.rust.contains("String::from(\""),
        "Should avoid String::from for literals"
    );
}
