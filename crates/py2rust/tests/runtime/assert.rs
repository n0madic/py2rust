//! Runtime tests for assertions.

use crate::common::run_py;

#[test]
fn runtime_assert_comprehensive() {
    run_py(
        "assert",
        r#"
# Simple assertions
assert True
assert not False
assert 1 == 1
assert 2 + 2 == 4

# Assertions with messages
value: int = 42
assert value == 42, "value should be 42"
assert value != 0, "value should not be zero"

# Assertions in functions
def validate_positive(val: int) -> None:
    assert val > 0, "number must be positive"

validate_positive(5)
validate_positive(1)
validate_positive(100)

# Complex assertion conditions
def check_range(num: int, min_val: int, max_val: int) -> None:
    assert num >= min_val and num <= max_val, "value out of range"

check_range(5, 0, 10)
check_range(0, 0, 10)
check_range(10, 0, 10)

print("All assertions passed!")
"#,
        Some("All assertions passed!"),
    );
}
