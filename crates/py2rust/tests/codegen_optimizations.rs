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

#[test]
fn exceptions_codegen_avoids_redundant_runtime_patterns() {
    // The comprehensive exception script is large enough to catch codegen bloat
    // regressions in print/lowering/indexing/arithmetic paths.
    let source = include_str!("runtime/exceptions.py");
    let out = compile(source, "exceptions.py", &CompileOptions::default())
        .expect("compile should succeed");

    // Leading docstrings in module/function bodies should not become runtime statements.
    assert!(
        out.rust.lines().all(|line| {
            let trimmed = line.trim();
            !(trimmed.starts_with('"') && trimmed.ends_with(".to_string();"))
        }),
        "Docstring-like string statements should not be emitted as executable code"
    );

    // Single-argument print should not allocate intermediate Vec + join in this script.
    assert!(
        !out.rust.contains("py_print(vec![format!(\"{}\","),
        "Single-argument print path should skip vec![] + join"
    );

    // Checked integer arithmetic should not emit redundant to_owned() for i64 values.
    assert!(
        !out.rust.contains(".to_owned().checked_add(")
            && !out.rust.contains(".to_owned().checked_sub(")
            && !out.rust.contains(".to_owned().checked_mul("),
        "checked_* arithmetic should not use to_owned()"
    );

    // String indexing should use py_str_get helper instead of chars collect + py_list_get.
    assert!(
        out.rust
            .contains("fn py_str_get(s: &str, idx: i64) -> Result<String, PyError>"),
        "Expected dedicated string indexing helper"
    );
    assert!(
        !out.rust.contains("chars().collect(); py_list_get("),
        "String indexing should avoid temporary Vec<char> collection"
    );
}

#[test]
fn list_equality_literal_uses_local_vec_operand() {
    // Fresh list literals in equality should stay local and avoid Arc<Mutex<_>> wrapping.
    let source = r#"

def f() -> bool:
    xs: list[int] = [9, 8, 7]
    return xs == [1, 2, 3]
"#;
    let out =
        compile(source, "test.py", &CompileOptions::default()).expect("compile should succeed");
    assert!(
        out.rust.contains("vec![1i64, 2i64, 3i64]"),
        "Expected local Vec literal for equality operand"
    );
    assert!(
        !out.rust
            .contains("Arc::new(Mutex::new(vec![1i64, 2i64, 3i64]))"),
        "Equality literal should not be wrapped in Arc<Mutex<_>>"
    );
}

#[test]
fn local_list_constructor_assignment_stays_vec_backed() {
    // `result = list(src)` should stay local for local storage targets.
    let source = r#"

def f() -> bool:
    src: list[int] = [1, 2, 3]
    result: list[int] = list(src)
    return result == [1, 2, 3]
"#;
    let out =
        compile(source, "test.py", &CompileOptions::default()).expect("compile should succeed");
    assert!(
        out.rust.contains("collect::<Vec<_>>()"),
        "Expected Vec-based list constructor lowering"
    );
    assert!(
        !out.rust.contains("let result = Arc::new(Mutex::new("),
        "Local list target should not use shared Arc<Mutex<_>> storage"
    );
}

#[test]
fn string_literal_call_arg_avoids_redundant_clone() {
    // String literals are rvalues and should not become `.to_string().clone()` at call sites.
    let source = r#"

def takes(s: str) -> int:
    return len(s)

def f() -> int:
    return takes("hello")
"#;
    let out =
        compile(source, "test.py", &CompileOptions::default()).expect("compile should succeed");
    assert!(
        out.rust.contains("takes(\"hello\".to_string())"),
        "Expected direct String construction for literal call arg"
    );
    assert!(
        !out.rust.contains("\"hello\".to_string().clone()"),
        "Literal call arg should not clone after to_string()"
    );
}

#[test]
fn optional_unwrap_avoids_double_clone() {
    // Optional-to-non-optional coercion should not produce `.clone().clone()`.
    let source = r#"

class Boxed:
    values: list[int]

    def __init__(self, values: list[int] | None = None):
        if values is None:
            values = [1, 2, 3]
        self.values = values
"#;
    let out =
        compile(source, "test.py", &CompileOptions::default()).expect("compile should succeed");
    assert!(
        out.rust
            .contains(".as_ref().expect(\"optional value is None\").clone()"),
        "Expected Optional unwrap clone for list coercion"
    );
    assert!(
        !out.rust.contains(".clone().clone()"),
        "Optional unwrap path should not emit double clone"
    );
}

#[test]
fn non_throwing_assert_uses_assert_macro() {
    // Non-throwing assert lowering should use Rust's idiomatic assert! macro.
    let source = r#"

def f() -> int:
    assert 1 == 1, "ok"
    return 1
"#;
    let out =
        compile(source, "test.py", &CompileOptions::default()).expect("compile should succeed");
    assert!(
        out.rust.contains("assert!("),
        "Expected non-throwing assert to use assert! macro"
    );
    assert!(
        !out.rust.contains("panic!(\"AssertionError: {}\""),
        "Non-throwing assert should not lower to panic! branch"
    );
}

#[test]
fn local_aliasing_lists_and_dicts_use_rc_refcell_storage() {
    // Escaping locals that never touch globals should stay on Rc<RefCell<...>>.
    let source = r#"

def f() -> int:
    cell_list: list[int] = [1, 2]
    alias_list: list[int] = cell_list
    alias_list.append(3)
    cell_dict: dict[str, int] = {"a": 1}
    alias_dict: dict[str, int] = cell_dict
    alias_dict["b"] = 2
    return len(cell_list) + len(cell_dict)
"#;
    let out =
        compile(source, "test.py", &CompileOptions::default()).expect("compile should succeed");
    assert!(
        out.rust.contains("let cell_list: Rc<RefCell<Vec<i64>>>"),
        "Expected local shared list storage to use Rc<RefCell<Vec<_>>>"
    );
    assert!(
        out.rust
            .contains("let mut alias_list: Rc<RefCell<Vec<i64>>>"),
        "Expected local list aliases to remain Rc<RefCell<Vec<_>>>"
    );
    assert!(
        out.rust
            .contains("let cell_dict: Rc<RefCell<IndexMap<String, i64>>>"),
        "Expected local shared dict storage to use Rc<RefCell<IndexMap<_, _>>>"
    );
    assert!(
        out.rust
            .contains("let mut alias_dict: Rc<RefCell<IndexMap<String, i64>>>"),
        "Expected local dict aliases to remain Rc<RefCell<IndexMap<_, _>>>"
    );
}

#[test]
fn global_alias_chains_promote_local_bindings_to_arc_mutex_storage() {
    // Any alias chain that touches globals must be sync-backed for static correctness.
    let source = r#"
g_list: list[int] = [1]
g_dict: dict[str, int] = {"x": 1}

def f() -> int:
    alias_list: list[int] = g_list
    alias_dict: dict[str, int] = g_dict
    alias_list.append(2)
    alias_dict["y"] = 2
    return len(g_list) + len(g_dict)
"#;
    let out =
        compile(source, "test.py", &CompileOptions::default()).expect("compile should succeed");
    assert!(
        out.rust
            .contains("static __GLOBAL_G_LIST: OnceLock<Mutex<Arc<Mutex<Vec<i64>>>>>"),
        "Expected global list storage to stay Arc<Mutex<_>>"
    );
    assert!(
        out.rust
            .contains("static __GLOBAL_G_DICT: OnceLock<Mutex<Arc<Mutex<IndexMap<String, i64>>>>>"),
        "Expected global dict storage to stay Arc<Mutex<_>>"
    );
    assert!(
        out.rust
            .contains("let mut alias_list: Arc<Mutex<Vec<i64>>>"),
        "Expected local alias of global list to be promoted to Arc<Mutex<_>>"
    );
    assert!(
        out.rust
            .contains("let mut alias_dict: Arc<Mutex<IndexMap<String, i64>>>"),
        "Expected local alias of global dict to be promoted to Arc<Mutex<_>>"
    );
    assert!(
        !out.rust
            .contains("let mut alias_list: Rc<RefCell<Vec<i64>>>")
            && !out
                .rust
                .contains("let mut alias_dict: Rc<RefCell<IndexMap<String, i64>>>"),
        "Global alias chains must not stay on Rc<RefCell<_>> storage"
    );
}
