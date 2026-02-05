//! Runtime tests for input/output operations.

use crate::common::run_py;

#[test]
fn runtime_io_comprehensive() {
    run_py(
        "io",
        r#"
# Print different types
print(42)
print("hello")
print(True)
print(False)

# Print with multiple arguments is supported in a dedicated test
# Keep this focused on basic single-argument prints

# Print string operations
s: str = "world"
print(s)

# Print arithmetic results
x: int = 10 + 20
print(x)

# Print from function
def get_message() -> str:
    return "message from function"

print(get_message())

# Print in loop - using while to avoid iterator issues
def print_loop() -> None:
    nums: list[int] = [1, 2, 3]
    i: int = 0
    while i < len(nums):
        print(nums[i])
        i = i + 1

print_loop()
"#,
        Some("42\nhello\ntrue\nfalse\nworld\n30\nmessage from function\n1\n2\n3"),
    );
}

#[test]
fn runtime_print_core_types() {
    run_py(
        "print_core_types",
        r#"
# Print single integer
print(42)

# Print single float
print(3.14)

# Print single bool
print(True)
print(False)

# Print None
print(None)

# Print string
print("Hello, World!")

# Print multiple values of same type
print(1, 2, 3)

# Print multiple values of different types
print(42, 3.14, True, "hello")

# Print expression results
x_print: int = 10
y_print: int = 20
print(x_print + y_print)

# Print in a loop
for i in range(3):
    print(i)

# Nested print (result of expression with function call)
def add_for_print(a: int, b: int) -> int:
    return a + b

print(add_for_print(5, 7))
"#,
        Some(
            "42\n3.14\ntrue\nfalse\nNone\nHello, World!\n1 2 3\n42 3.14 true hello\n30\n0\n1\n2\n12",
        ),
    );
}
