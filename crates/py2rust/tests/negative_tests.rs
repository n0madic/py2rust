use py2rust::{compile, CompileOptions};

fn expect_error(source: &str) -> String {
    compile(source, "test.py", &CompileOptions::default())
        .expect_err("Expected compilation error")
        .to_string()
}

fn expect_success(source: &str) {
    compile(source, "test.py", &CompileOptions::default()).expect("Expected compilation success");
}

#[test]
fn supports_star_args() {
    let source = r#"
def bad(*args: int) -> int:
    return len(args)

value: int = bad(1, 2, 3)
"#;
    expect_success(source);
}

#[test]
fn supports_kwargs() {
    let source = r#"
def bad(**kwargs: int) -> int:
    total: int = 0
    for key in kwargs:
        total = total + 1
    return total
"#;
    expect_success(source);
}

#[test]
fn rejects_async_function() {
    let source = r#"
async def bad() -> None:
    pass
"#;
    let error = expect_error(source);
    assert!(error.contains("Unsupported"), "Error: {}", error);
}

#[test]
fn rejects_multiple_inheritance() {
    let source = r#"
class Base1:
    pass

class Base2:
    pass

class Child(Base1, Base2):
    pass
"#;
    let error = expect_error(source);
    assert!(error.contains("inheritance"), "Error: {}", error);
}

#[test]
fn rejects_class_decorators() {
    let source = r#"
@dataclass
class MyClass:
    x: int
"#;
    let error = expect_error(source);
    assert!(error.contains("decorator"), "Error: {}", error);
}

#[test]
fn rejects_decorator_calls() {
    let source = r#"
@decorator()
def bad() -> None:
    pass
"#;
    let error = expect_error(source);
    assert!(
        error.contains("Unknown call target") || error.contains("decorator"),
        "Error: {}",
        error
    );
}

#[test]
fn rejects_type_parameters() {
    let source = r#"
def bad[T](x: T) -> T:
    return x
"#;
    let error = expect_error(source);
    assert!(error.contains("Type parameters"), "Error: {}", error);
}

#[test]
fn rejects_unknown_keyword_arguments() {
    let source = r#"
def good(x: int, y: int) -> int:
    return x + y

z: int = good(x=5, unknown=1)
"#;
    let error = expect_error(source);
    assert!(
        error.contains("Unknown keyword argument"),
        "Error: {}",
        error
    );
}

#[test]
fn rejects_slice_step_zero() {
    let source = r#"
def bad(lst: list[int]) -> list[int]:
    return lst[::0]
"#;
    let error = expect_error(source);
    assert!(
        error.contains("step cannot be zero") || error.contains("Slice step"),
        "Error: {}",
        error
    );
}

#[test]
fn rejects_async_comprehensions() {
    let source = r#"
async def bad() -> list[int]:
    return [x async for x in some_iter()]
"#;
    let error = expect_error(source);
    // Either async or comprehensions not supported
    assert!(
        error.contains("Unsupported") || error.contains("Async"),
        "Error: {}",
        error
    );
}

#[test]
fn rejects_match_guards() {
    let source = r#"
class Foo:
    def __init__(self) -> None:
        pass

def bad(x: Foo) -> int:
    match x:
        case Foo() if True:
            return 1
"#;
    let error = expect_error(source);
    assert!(error.contains("guard"), "Error: {}", error);
}

#[test]
fn rejects_lambda_star_args() {
    let source = r#"
f = lambda *args: 0
"#;
    let error = expect_error(source);
    assert!(
        error.contains("*args") || error.contains("kwargs") || error.contains("Lambda"),
        "Error: {}",
        error
    );
}

#[test]
fn rejects_lambda_defaults() {
    let source = r#"
f = lambda x=5: x
"#;
    let error = expect_error(source);
    assert!(
        error.contains("default") || error.contains("Lambda"),
        "Error: {}",
        error
    );
}

#[test]
fn rejects_unsupported_binary_ops() {
    let source = r#"
x: int = 5 @ 3
"#;
    let error = expect_error(source);
    assert!(
        error.contains("Unsupported") || error.contains("binary") || error.contains("operator"),
        "Error: {}",
        error
    );
}

#[test]
fn rejects_os_remove_without_import() {
    let source = r#"
os.remove("/tmp/py2rust_missing_import.txt")
"#;
    let error = expect_error(source);
    assert!(
        error.contains("module 'os' used without import"),
        "Error: {}",
        error
    );
}

#[test]
fn rejects_from_os_import_unknown_member() {
    let source = r#"
from os import unknown
"#;
    let error = expect_error(source);
    assert!(
        error.contains("os has no supported member 'unknown'"),
        "Error: {}",
        error
    );
}

#[test]
fn rejects_from_os_import_wildcard() {
    let source = r#"
from os import *
"#;
    let error = expect_error(source);
    assert!(
        error.contains("from os import * is not supported"),
        "Error: {}",
        error
    );
}

#[test]
fn rejects_wrong_arity_for_imported_os_remove() {
    let source = r#"
from os import remove
remove()
"#;
    let error = expect_error(source);
    assert!(
        error.contains("os.remove() expects one argument"),
        "Error: {}",
        error
    );
}

#[test]
fn rejects_wrong_arity_for_sys_exit() {
    let source = r#"
import sys
sys.exit(1, 2)
"#;
    let error = expect_error(source);
    assert!(
        error.contains("sys.exit() expects zero or one argument"),
        "Error: {}",
        error
    );
}
