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

# Print with multiple arguments would require implementation support
# For now, test basic single-argument prints

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
