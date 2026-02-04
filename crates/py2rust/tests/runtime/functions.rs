//! Runtime tests for functions and recursion.

use crate::common::run_py;

#[test]
fn runtime_functions_comprehensive() {
    run_py(
        "functions",
        r#"
# Simple function
def add(a: int, b: int) -> int:
    return a + b

# Recursive factorial
def factorial(n: int) -> int:
    if n <= 1:
        return 1
    else:
        return n * factorial(n - 1)

# Recursive fibonacci
def fib(n: int) -> int:
    if n <= 1:
        return n
    else:
        return fib(n - 1) + fib(n - 2)

# Function with multiple returns
def classify(x: int) -> str:
    if x < 0:
        return "negative"
    elif x == 0:
        return "zero"
    else:
        return "positive"

# Test simple function
result: int = add(2, 3)
assert result == 5, "add(2, 3) should be 5"

# Test factorial
assert factorial(0) == 1
assert factorial(1) == 1
assert factorial(5) == 120
assert factorial(10) == 3628800

# Test fibonacci
assert fib(0) == 0
assert fib(1) == 1
assert fib(10) == 55
assert fib(15) == 610

# Test classification
assert classify(-5) == "negative"
assert classify(0) == "zero"
assert classify(42) == "positive"

print("All function tests passed!")
"#,
        Some("All function tests passed!"),
    );
}
