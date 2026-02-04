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
