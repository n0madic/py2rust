use py2rust::{compile, CompileOptions};

#[test]
fn empty_list_max_has_error_message() {
    let source = r#"
def test() -> int:
    lst: list[int] = []
    return max(lst)
"#;
    let out =
        compile(source, "test.py", &CompileOptions::default()).expect("compile should succeed");
    // Should use py_max helper that panics with descriptive message
    assert!(out.rust.contains("py_max("), "Should use py_max helper");
}

#[test]
fn empty_list_min_has_error_message() {
    let source = r#"
def test() -> int:
    lst: list[int] = []
    return min(lst)
"#;
    let out =
        compile(source, "test.py", &CompileOptions::default()).expect("compile should succeed");
    // Should use py_min helper that panics with descriptive message
    assert!(out.rust.contains("py_min("), "Should use py_min helper");
}

#[test]
fn int_parse_has_error_message() {
    let source = r#"
def parse(s: str) -> int:
    return int(s)
"#;
    let out =
        compile(source, "test.py", &CompileOptions::default()).expect("compile should succeed");
    // Should use py_parse_int helper that panics with descriptive message
    assert!(
        out.rust.contains("py_parse_int("),
        "Should use py_parse_int helper"
    );
}

#[test]
fn float_parse_has_error_message() {
    let source = r#"
def parse(s: str) -> float:
    return float(s)
"#;
    let out =
        compile(source, "test.py", &CompileOptions::default()).expect("compile should succeed");
    // Should use py_parse_float helper that panics with descriptive message
    assert!(
        out.rust.contains("py_parse_float("),
        "Should use py_parse_float helper"
    );
}

#[test]
fn dict_missing_key_error() {
    let source = r#"
def get(d: dict[str, int], key: str) -> int:
    return d[key]
"#;
    let out =
        compile(source, "test.py", &CompileOptions::default()).expect("compile should succeed");
    // Should use .expect("KeyError") in generated code
    assert!(
        out.rust.contains("KeyError"),
        "Should include KeyError message"
    );
}

#[test]
fn negative_index_literal() {
    let source = r#"
def last(lst: list[int]) -> int:
    return lst[-1]
"#;
    let out =
        compile(source, "test.py", &CompileOptions::default()).expect("compile should succeed");
    // Literal -1 should trigger py_list_get usage
    assert!(
        out.rust.contains("py_list_get("),
        "Should use py_list_get for negative literal"
    );
}

#[test]
fn negative_index_variable() {
    let source = r#"
def get_at(lst: list[int], idx: int) -> int:
    return lst[idx]
"#;
    let out =
        compile(source, "test.py", &CompileOptions::default()).expect("compile should succeed");
    // Variable index might be negative, should use py_list_get
    assert!(
        out.rust.contains("py_list_get("),
        "Should use py_list_get for variable index"
    );
}

#[test]
fn positive_index_literal() {
    let source = r#"
def first(lst: list[int]) -> int:
    return lst[0]
"#;
    let out =
        compile(source, "test.py", &CompileOptions::default()).expect("compile should succeed");
    // Positive literal should use py_list_get and not py_index
    assert!(
        !out.rust.contains("py_index("),
        "Should not use py_index for positive literal"
    );
    assert!(
        out.rust.contains("py_list_get("),
        "Should use py_list_get for positive literal"
    );
}

#[test]
fn negative_index_assignment() {
    let source = r#"
def set_last(lst: list[int], val: int) -> None:
    lst[-1] = val
"#;
    let out =
        compile(source, "test.py", &CompileOptions::default()).expect("compile should succeed");
    // Assignment with negative index should use py_index
    assert!(
        out.rust.contains("py_index("),
        "Should use py_index for negative index assignment"
    );
}

#[test]
fn dict_literal_uses_builder_ops() {
    let source = r#"
def make_dict() -> dict[str, int]:
    return {"a": 1, "b": 2}
"#;
    let out =
        compile(source, "test.py", &CompileOptions::default()).expect("compile should succeed");
    // Dict literals are emitted through a builder (`new` + `insert/extend`)
    // so source order and unpack override semantics are preserved.
    assert!(
        out.rust.contains("IndexMap::new()") && out.rust.contains(".insert("),
        "Should build dict literals with IndexMap::new() + insert()"
    );
    assert!(
        !out.rust.contains("IndexMap::from(["),
        "Should not use IndexMap::from for dict literal codegen"
    );
}

#[test]
fn empty_dict_literal() {
    let source = r#"
def make_empty() -> dict[str, int]:
    return {}
"#;
    let out =
        compile(source, "test.py", &CompileOptions::default()).expect("compile should succeed");
    // Empty dict may require explicit key/value generic args in Rust when there is
    // no contextual type information at the expression site.
    assert!(
        out.rust.contains("IndexMap::new()") || out.rust.contains("IndexMap::<"),
        "Should use IndexMap::new() (possibly with explicit generics) for empty dict"
    );
}

#[test]
fn set_literal_uses_from() {
    let source = r#"
def make_set() -> set[int]:
    return {1, 2, 3}
"#;
    let out =
        compile(source, "test.py", &CompileOptions::default()).expect("compile should succeed");
    // Should use HashSet::from([...]) instead of repeated .insert()
    assert!(
        out.rust.contains("HashSet::from(["),
        "Should use HashSet::from"
    );
}

#[test]
fn py_max_helper_is_emitted() {
    let source = r#"
def test() -> int:
    return max([1, 2, 3])
"#;
    let out =
        compile(source, "test.py", &CompileOptions::default()).expect("compile should succeed");
    // Should contain the py_max helper definition
    assert!(
        out.rust.contains("fn py_max<T: PartialOrd"),
        "Should emit py_max helper"
    );
    assert!(
        out.rust.contains("empty sequence"),
        "Should have error message in py_max"
    );
}

#[test]
fn py_min_helper_is_emitted() {
    let source = r#"
def test() -> int:
    return min([1, 2, 3])
"#;
    let out =
        compile(source, "test.py", &CompileOptions::default()).expect("compile should succeed");
    // Should contain the py_min helper definition
    assert!(
        out.rust.contains("fn py_min<T: PartialOrd"),
        "Should emit py_min helper"
    );
    assert!(
        out.rust.contains("empty sequence"),
        "Should have error message in py_min"
    );
}

#[test]
fn py_parse_int_helper_is_emitted() {
    let source = r#"
def test(s: str) -> int:
    return int(s)
"#;
    let out =
        compile(source, "test.py", &CompileOptions::default()).expect("compile should succeed");
    // Should contain the py_parse_int helper definition
    assert!(
        out.rust.contains("fn py_parse_int("),
        "Should emit py_parse_int helper"
    );
    assert!(
        out.rust.contains("invalid literal for int()"),
        "Should have error message"
    );
}

#[test]
fn py_index_helper_is_emitted() {
    let source = r#"
def test(lst: list[int], i: int, v: int) -> None:
    lst[i] = v
"#;
    let out =
        compile(source, "test.py", &CompileOptions::default()).expect("compile should succeed");
    // Should contain the py_index helper definition
    assert!(
        out.rust.contains("fn py_index("),
        "Should emit py_index helper"
    );
}

#[test]
fn py_list_get_helper_is_emitted() {
    let source = r#"
def test(lst: list[int], i: int) -> int:
    return lst[i]
"#;
    let out =
        compile(source, "test.py", &CompileOptions::default()).expect("compile should succeed");
    // Should contain the py_list_get helper definition
    assert!(
        out.rust.contains("fn py_list_get"),
        "Should emit py_list_get helper"
    );
}

#[test]
fn dict_pop_key_named_guard_is_preserved() {
    let source = r#"
def run() -> int:
    data: dict[str, int] = {"{guard}": 7}
    return data.pop("{guard}")
"#;
    let out =
        compile(source, "test.py", &CompileOptions::default()).expect("compile should succeed");
    // Ensure dict.pop lowering preserves literal "{guard}" text and does not do template replacement.
    assert!(
        out.rust.contains("{guard}"),
        "Generated Rust should preserve the literal key text"
    );
}

#[test]
fn try_else_binding_uses_nameerror_path_without_expect() {
    let source = r#"
def run() -> int:
    try:
        x: int = 1
    except Exception:
        return 0
    else:
        return x
"#;
    let out =
        compile(source, "test.py", &CompileOptions::default()).expect("compile should succeed");
    // Else-binding extraction should emit explicit NameError diagnostics, not panic-prone .expect text.
    assert!(
        !out.rust.contains("try block did not initialize"),
        "Generated Rust must not contain legacy expect panic text"
    );
    assert!(
        out.rust.contains("NameError: x is not defined"),
        "Generated Rust should include the controlled NameError path"
    );
}
