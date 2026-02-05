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

    # Membership
    vals: list[int] = [1, 2, 3]
    assert 2 in vals
    assert 4 not in vals

    text: str = "abc"
    assert "a" in text
    assert "d" not in text

    d: dict[str, int] = {"a": 1}
    assert "a" in d
    assert "b" not in d

    s: set[int] = {1, 2}
    assert 2 in s
    assert 3 not in s

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

# Set operations
def test_set_ops() -> None:
    a: set[int] = {1, 2, 3}
    b: set[int] = {2, 3, 4}

    # Union
    union_result: set[int] = a | b
    assert 1 in union_result
    assert 2 in union_result
    assert 3 in union_result
    assert 4 in union_result
    assert len(union_result) == 4

    # Intersection
    inter_result: set[int] = a & b
    assert 2 in inter_result
    assert 3 in inter_result
    assert 1 not in inter_result
    assert 4 not in inter_result
    assert len(inter_result) == 2

    # Difference
    diff_result: set[int] = a - b
    assert 1 in diff_result
    assert 2 not in diff_result
    assert 3 not in diff_result
    assert len(diff_result) == 1

    # Symmetric difference
    sym_diff: set[int] = a ^ b
    assert 1 in sym_diff
    assert 4 in sym_diff
    assert 2 not in sym_diff
    assert 3 not in sym_diff
    assert len(sym_diff) == 2

    # Empty set operations
    empty: set[int] = set()
    c: set[int] = {1, 2}

    assert (empty | c) == c
    assert (empty & c) == empty
    assert (c - empty) == c
    assert (empty - c) == empty
    assert (empty ^ c) == c

    # Disjoint sets
    d: set[int] = {1, 2}
    e: set[int] = {3, 4}

    disjoint_inter: set[int] = d & e
    assert len(disjoint_inter) == 0

    disjoint_union: set[int] = d | e
    assert len(disjoint_union) == 4

    # Same sets
    f: set[int] = {1, 2, 3}
    g: set[int] = {1, 2, 3}

    assert (f | g) == f
    assert (f & g) == f
    assert len(f - g) == 0
    assert len(f ^ g) == 0

    # Subset/superset scenarios
    small: set[int] = {2, 3}
    large: set[int] = {1, 2, 3, 4}

    assert (small & large) == small
    assert (large - small) == {1, 4}

    # String sets
    s1: set[str] = {"a", "b", "c"}
    s2: set[str] = {"b", "c", "d"}

    str_union: set[str] = s1 | s2
    assert "a" in str_union
    assert "d" in str_union
    assert len(str_union) == 4

    str_inter: set[str] = s1 & s2
    assert "b" in str_inter
    assert "c" in str_inter
    assert len(str_inter) == 2

# Run all tests
test_arithmetic()
test_comparison()
test_boolean()
test_set_ops()

print("All operator tests passed!")
"#,
        Some("All operator tests passed!"),
    );
}
