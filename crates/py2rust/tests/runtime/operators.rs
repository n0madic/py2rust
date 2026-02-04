//! Runtime tests for operators: arithmetic, comparison, logical.

use crate::common::run_py;

#[test]
fn runtime_operators_comprehensive() {
    run_py(
        "operators",
        r#"
# Arithmetic operations
def test_arithmetic() -> None:
    # Addition
    assert 1 + 2 == 3
    assert 10 + 20 == 30

    # Subtraction
    assert 5 - 3 == 2
    assert 100 - 50 == 50

    # Multiplication
    assert 3 * 4 == 12
    assert 7 * 8 == 56

    # Division (integer)
    assert 10 // 2 == 5
    assert 21 // 4 == 5

    # Modulo
    assert 10 % 3 == 1
    assert 17 % 5 == 2

    # Negation
    assert -5 == 0 - 5
    assert -(-10) == 10

    # Mixed operations
    assert 2 + 3 * 4 == 14
    assert (2 + 3) * 4 == 20

# Comparison operations
def test_comparison() -> None:
    # Less than
    assert 1 < 2
    assert not (2 < 2)
    assert not (3 < 2)

    # Less than or equal
    assert 1 <= 2
    assert 2 <= 2
    assert not (3 <= 2)

    # Greater than
    assert 3 > 2
    assert not (2 > 2)
    assert not (1 > 2)

    # Greater than or equal
    assert 3 >= 2
    assert 3 >= 3
    assert not (2 >= 3)

    # Equality
    assert 5 == 5
    assert not (5 == 6)

    # Inequality
    assert 5 != 6
    assert not (5 != 5)

# Boolean logic
def test_boolean() -> None:
    # Basic boolean values
    assert True
    assert not False

    # AND
    assert True and True
    assert not (True and False)
    assert not (False and True)
    assert not (False and False)

    # OR
    assert True or True
    assert True or False
    assert False or True
    assert not (False or False)

    # NOT
    assert not False
    assert not (not True)

    # Combined
    assert not (True and False)
    assert (True or False) and True
    assert not ((False or False) and True)

# Run all tests
test_arithmetic()
test_comparison()
test_boolean()

print("All operator tests passed!")
"#,
        Some("All operator tests passed!"),
    );
}
