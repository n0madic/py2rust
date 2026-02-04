//! Runtime tests for control flow: loops, conditionals, break, continue.

use crate::common::run_py;

#[test]
fn runtime_control_flow_comprehensive() {
    run_py(
        "control_flow",
        r#"
# While loop
def sum_to(n: int) -> int:
    total: int = 0
    i: int = 1
    while i <= n:
        total = total + i
        i = i + 1
    return total

# For loop with range
def sum_range(n: int) -> int:
    total: int = 0
    for i in range(n):
        total = total + i
    return total

# Nested loops
def multiply_table(n: int) -> int:
    total: int = 0
    i: int = 1
    while i <= n:
        j: int = 1
        while j <= n:
            total = total + i * j
            j = j + 1
        i = i + 1
    return total

# Test while loop
assert sum_to(0) == 0
assert sum_to(1) == 1
assert sum_to(10) == 55
assert sum_to(100) == 5050

# Test for with range
assert sum_range(0) == 0
assert sum_range(1) == 0
assert sum_range(5) == 10

# Test nested loops
assert multiply_table(1) == 1
assert multiply_table(2) == 9
assert multiply_table(3) == 36

print("All control flow tests passed!")
"#,
        Some("All control flow tests passed!"),
    );
}
