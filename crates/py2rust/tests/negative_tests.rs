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
fn supports_positional_only_parameters() {
    let source = r#"
def add(x: int, /, y: int) -> int:
    return x + y

value: int = add(1, y=2)
assert value == 3
"#;
    expect_success(source);
}

#[test]
fn rejects_keyword_for_positional_only_parameter() {
    let source = r#"
def add(x: int, /, y: int) -> int:
    return x + y

value: int = add(x=1, y=2)
"#;
    let error = expect_error(source);
    assert!(
        error.contains("Positional-only argument passed as keyword"),
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
fn supports_set_extend_method() {
    let source = r#"
values: set[int] = {1, 2, 3}
values.extend([4, 5])
"#;
    expect_success(source);
}

#[test]
fn supports_set_pop_method() {
    let source = r#"
values: set[int] = {1, 2, 3}
popped: int = values.pop()
assert popped > 0
"#;
    expect_success(source);
}

#[test]
fn rejects_set_extend_wrong_arity() {
    let source = r#"
values: set[int] = {1, 2, 3}
values.extend()
"#;
    let error = expect_error(source);
    assert!(
        error.contains("set.extend() expects one argument"),
        "Error: {}",
        error
    );
}

#[test]
fn rejects_set_pop_with_argument() {
    let source = r#"
values: set[int] = {1, 2, 3}
values.pop(0)
"#;
    let error = expect_error(source);
    assert!(
        error.contains("set.pop() expects no arguments"),
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
fn supports_from_os_import_wildcard() {
    let source = r#"
from os import *
"#;
    expect_success(source);
}

#[test]
fn rejects_unknown_name_at_compile_time() {
    let source = r#"
value: int = missing_name
"#;
    let error = expect_error(source);
    assert!(error.contains("NameError"), "Error: {}", error);
    assert!(error.contains("missing_name"), "Error: {}", error);
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

#[test]
fn rejects_time_call_without_import() {
    let source = r#"
time.time()
"#;
    let error = expect_error(source);
    assert!(
        error.contains("module 'time' used without import"),
        "Error: {}",
        error
    );
}

#[test]
fn rejects_from_time_import_unknown_member() {
    let source = r#"
from time import unknown
"#;
    let error = expect_error(source);
    assert!(
        error.contains("time has no supported member 'unknown'"),
        "Error: {}",
        error
    );
}

#[test]
fn rejects_wrong_arity_for_time_localtime() {
    let source = r#"
import time
time.localtime(1, 2)
"#;
    let error = expect_error(source);
    assert!(
        error.contains("time.localtime() expects zero or one argument"),
        "Error: {}",
        error
    );
}

#[test]
fn rejects_wrong_arity_for_time_strftime() {
    let source = r#"
import time
time.strftime("%Y")
"#;
    let error = expect_error(source);
    assert!(
        error.contains("time.strftime() expects two arguments"),
        "Error: {}",
        error
    );
}

#[test]
fn rejects_wrong_tuple_shape_for_time_strftime() {
    let source = r#"
import time
time.strftime("%Y", (2024, 1, 1))
"#;
    let error = expect_error(source);
    assert!(
        error.contains("time.strftime() expects a 9-item time tuple"),
        "Error: {}",
        error
    );
}

#[test]
fn rejects_wrong_arity_for_time_strptime() {
    let source = r#"
import time
time.strptime("2024-01-01")
"#;
    let error = expect_error(source);
    assert!(
        error.contains("time.strptime() expects two arguments"),
        "Error: {}",
        error
    );
}

#[test]
fn rejects_subprocess_call_without_import() {
    let source = r#"
subprocess.run(["echo", "hello"])
"#;
    let error = expect_error(source);
    assert!(
        error.contains("module 'subprocess' used without import"),
        "Error: {}",
        error
    );
}

#[test]
fn rejects_from_subprocess_import_unknown_member() {
    let source = r#"
from subprocess import unknown
"#;
    let error = expect_error(source);
    assert!(
        error.contains("subprocess has no supported member 'unknown'"),
        "Error: {}",
        error
    );
}

#[test]
fn rejects_wrong_arity_for_subprocess_run() {
    let source = r#"
import subprocess
subprocess.run()
"#;
    let error = expect_error(source);
    assert!(
        error.contains("subprocess.run() expects"),
        "Error: {}",
        error
    );
}

#[test]
fn rejects_subprocess_run_duplicate_capture_output() {
    let source = r#"
import subprocess
subprocess.run(["echo", "hello"], True, capture_output=False)
"#;
    let error = expect_error(source);
    assert!(
        error.contains("Multiple values for keyword argument `capture_output`"),
        "Error: {}",
        error
    );
}

#[test]
fn rejects_urllib_parse_call_without_import() {
    let source = r#"
urllib.parse.quote("x")
"#;
    let error = expect_error(source);
    assert!(
        error.contains("module 'urllib' used without import"),
        "Error: {}",
        error
    );
}

#[test]
fn rejects_from_urllib_parse_import_unknown_member() {
    let source = r#"
from urllib.parse import unknown
"#;
    let error = expect_error(source);
    assert!(
        error.contains("urllib.parse has no supported member 'unknown'"),
        "Error: {}",
        error
    );
}

#[test]
fn rejects_wrong_arity_for_urllib_parse_quote() {
    let source = r#"
from urllib.parse import quote
quote()
"#;
    let error = expect_error(source);
    assert!(
        error.contains("urllib.parse.quote() expects"),
        "Error: {}",
        error
    );
}

#[test]
fn rejects_urllib_request_urlopen_duplicate_timeout() {
    let source = r#"
from urllib.request import urlopen
urlopen("data:text/plain,ok", None, 1.0, timeout=2.0)
"#;
    let error = expect_error(source);
    assert!(
        error.contains("Multiple values for keyword argument `timeout`"),
        "Error: {}",
        error
    );
}

#[test]
fn rejects_implicit_str_coercion_for_user_functions() {
    let source = r#"
def takes_text(x: str) -> str:
    return x

value: str = takes_text(42)
"#;
    let error = expect_error(source);
    assert!(
        error.contains("str") && error.contains("int"),
        "Error: {}",
        error
    );
}

#[test]
fn rejects_recursive_lambda_inference_cycles() {
    let source = r#"
f = lambda x: x
f = lambda x: f(x)
f(1)
"#;
    let error = expect_error(source);
    assert!(
        error.contains("Recursive lambda type inference cycle for 'f'"),
        "Error: {}",
        error
    );
}

#[test]
fn rejects_invalid_property_setter_signature() {
    let source = r#"
class Box:
    @property
    def value(self) -> int:
        return 1

    @value.setter
    def value(self, new_value: int, extra: int) -> None:
        self.hidden = new_value
"#;
    let error = expect_error(source);
    assert!(
        error.contains("Property setter must have signature (self, value)"),
        "Error: {}",
        error
    );
}

#[test]
fn rejects_duplicate_union_match_cases() {
    let source = r#"
class A:
    def __init__(self) -> None:
        pass

class B:
    def __init__(self) -> None:
        pass

U = A | B

def classify(v: U) -> int:
    match v:
        case A():
            return 1
        case A():
            return 2
        case B():
            return 3
"#;
    let error = expect_error(source);
    assert!(
        error.contains("Duplicate match case for variant 'A'"),
        "Error: {}",
        error
    );
}
