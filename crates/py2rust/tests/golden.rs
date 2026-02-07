use py2rust::{compile, CompileOptions};

#[test]
fn emits_union_match() {
    let source = r#"
class Circle:
    def __init__(self, r: float) -> None:
        self.r: float = r

class Rect:
    def __init__(self, w: float, h: float) -> None:
        self.w: float = w
        self.h: float = h

Shape = Circle | Rect

def area(s: Shape) -> float:
    match s:
        case Circle(r):
            return 3.14 * r * r
        case Rect(w, h):
            return w * h
"#;
    let out =
        compile(source, "test.py", &CompileOptions::default()).expect("compile should succeed");
    assert!(out.rust.contains("enum Shape"));
    assert!(out.rust.contains("match s"));
}

#[test]
fn emits_union_method_call() {
    let source = r#"
class Circle:
    r: float
    def __init__(self, r: float) -> None:
        self.r = r
    def describe(self) -> str:
        return "Circle"

class Rect:
    w: float
    h: float
    def __init__(self, w: float, h: float) -> None:
        self.w = w
        self.h = h
    def describe(self) -> str:
        return "Rect"

Shape = Circle | Rect

def print_shape(s: Shape) -> str:
    return s.describe()
"#;
    let out =
        compile(source, "test.py", &CompileOptions::default()).expect("compile should succeed");
    // Union method calls should generate match expression with ref patterns.
    assert!(out.rust.contains("match s"));
    assert!(out.rust.contains("Shape::Circle(ref _x) => _x.describe()"));
    assert!(out.rust.contains("Shape::Rect(ref _x) => _x.describe()"));
}

#[test]
fn emits_iterator() {
    let source = r#"
class CountTo:
    def __init__(self, n: int) -> None:
        self.n: int = n
        self.i: int = 0

    def __iter__(self) -> "CountToIter":
        return CountToIter(self.n)

class CountToIter:
    def __init__(self, n: int) -> None:
        self.n: int = n
        self.i: int = 0

    def next(self) -> int | None:
        if self.i < self.n:
            v: int = self.i
            self.i = self.i + 1
            return v
        else:
            return None

def sum_n(n: int) -> int:
    s: int = 0
    for x in CountTo(n):
        s = s + x
    return s
"#;
    let out =
        compile(source, "test.py", &CompileOptions::default()).expect("compile should succeed");
    assert!(out.rust.contains("impl IntoIterator for CountTo"));
    assert!(out.rust.contains("impl Iterator for CountToIter"));
}

#[test]
fn top_level_local_not_globalized() {
    let source = r#"
x = 1

def f() -> int:
    y: int = 2
    return y
"#;
    let out =
        compile(source, "test.py", &CompileOptions::default()).expect("compile should succeed");
    // Unused module variables should stay local to main.
    assert!(!out.rust.contains("__GLOBAL_X"));
}

#[test]
fn top_level_used_in_function_globalized() {
    let source = r#"
x = 1

def f() -> int:
    return x
"#;
    let out =
        compile(source, "test.py", &CompileOptions::default()).expect("compile should succeed");
    // Access from a function should force a global storage slot.
    assert!(out.rust.contains("__GLOBAL_X"));
}

#[test]
fn emits_keyword_argument_reordering() {
    let source = r#"
def f(a: int, b: int) -> int:
    return a - b

x: int = f(b=1, a=3)
"#;
    let out =
        compile(source, "test.py", &CompileOptions::default()).expect("compile should succeed");
    assert!(out.rust.contains("f(3i64, 1i64)"));
}

#[test]
fn renames_user_main_definition_and_calls() {
    let source = r#"
def main() -> int:
    return 7

def wrapper() -> int:
    return main()

top: int = main()
"#;
    let out =
        compile(source, "test.py", &CompileOptions::default()).expect("compile should succeed");

    // User `def main()` must be renamed to avoid colliding with generated Rust entrypoint.
    assert!(out.rust.contains("fn __py_main("));
    assert!(out.rust.contains("fn main() {"));

    // We expect one definition + at least two call sites (`wrapper` + top-level).
    let renamed_refs = out.rust.matches("__py_main(").count();
    assert!(
        renamed_refs >= 3,
        "expected renamed function definition and call sites, got {renamed_refs} refs"
    );

    // A direct nested return call should not target `main(...)` anymore.
    assert!(!out.rust.contains("return main("));
}

#[test]
fn main_rename_preserves_assignment_target_legacy_behavior() {
    let source = r#"
def main() -> int:
    return 0

items: list[int] = [0]
items[main()] = 1
"#;
    let error = compile(source, "test.py", &CompileOptions::default())
        .expect_err("legacy assignment-target traversal should not rename main()")
        .to_string();
    assert!(
        error.contains("Unknown call target"),
        "Error should reflect unresolved main() inside assignment target: {error}"
    );
}

#[test]
fn main_rename_preserves_comp_generator_legacy_behavior() {
    let source = r#"
def main() -> int:
    return 1

values: list[int] = [x for x in [1] for y in range(main())]
"#;
    let error = compile(source, "test.py", &CompileOptions::default())
        .expect_err("legacy comprehension-generator traversal should not rename main()")
        .to_string();
    assert!(
        error.contains("Unknown call target"),
        "Error should reflect unresolved main() inside comprehension generators: {error}"
    );
}
