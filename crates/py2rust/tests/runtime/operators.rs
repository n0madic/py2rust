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

# Helper classes for augmented assignment tests
class Counter:
    count: int

    def __init__(self, start: int) -> None:
        self.count = start

    def increment(self) -> None:
        self.count += 1

    def add(self, n_add: int) -> None:
        self.count += n_add

class Stats:
    value: int

    def __init__(self, v: int) -> None:
        self.value = v

# Augmented assignments
def test_augmented_assignments() -> None:
    # Integer arithmetic
    x_aug: int = 10
    x_aug += 5
    assert x_aug == 15, "x += 5 failed"

    x_aug -= 3
    assert x_aug == 12, "x -= 3 failed"

    x_aug *= 2
    assert x_aug == 24, "x *= 2 failed"

    y_aug: int = 25
    y_aug //= 3
    assert y_aug == 8, "y //= 3 failed"

    z_aug: int = 17
    z_aug %= 5
    assert z_aug == 2, "z %= 5 failed"

    p_aug: int = 2
    p_aug **= 3
    assert p_aug == 8, "p **= 3 failed"

    # Float arithmetic
    f_aug: float = 10.0
    f_aug += 2.5
    assert f_aug == 12.5, "f += 2.5 failed"

    f_aug -= 3.5
    assert f_aug == 9.0, "f -= 3.5 failed"

    f_aug *= 2.0
    assert f_aug == 18.0, "f *= 2.0 failed"

    f_aug /= 3.0
    assert f_aug == 6.0, "f /= 3.0 failed"

    g_aug: float = 17.0
    g_aug //= 3.0
    assert g_aug == 5.0, "g //= 3.0 failed"

    h_aug: float = 17.5
    h_aug %= 5.0
    assert h_aug == 2.5, "h %= 5.0 failed"

    # Augmented assignment in loops
    total_aug: int = 0
    for i_aug in range(1, 6):
        total_aug += i_aug
    assert total_aug == 15, "loop += failed (1+2+3+4+5=15)"

    product_aug: int = 1
    for i_aug in range(1, 5):
        product_aug *= i_aug
    assert product_aug == 24, "loop *= failed (1*2*3*4=24)"

    # List indexing
    nums_aug: list[int] = [10, 20, 30]
    nums_aug[0] += 5
    assert nums_aug[0] == 15, "nums[0] += 5 failed"

    nums_aug[1] -= 5
    assert nums_aug[1] == 15, "nums[1] -= 5 failed"

    nums_aug[2] *= 2
    assert nums_aug[2] == 60, "nums[2] *= 2 failed"

    idx: int = 1
    nums_aug[idx] += 10
    assert nums_aug[1] == 25, "nums[idx] += 10 failed"

    # Negative values
    neg_aug: int = 5
    neg_aug += -3
    assert neg_aug == 2, "neg += -3 failed"

    neg_aug *= -1
    assert neg_aug == -2, "neg *= -1 failed"

    # Chained augmented operations
    chain: int = 100
    chain -= 10
    chain //= 3
    chain *= 2
    assert chain == 60, "chained ops failed (100-10=90, 90//3=30, 30*2=60)"

    # RHS expressions
    base: int = 10
    base += 2 * 3
    assert base == 16, "base += 2 * 3 failed"

    base -= 4 + 2
    assert base == 10, "base -= 4 + 2 failed"

    # String concatenation and repetition
    s_aug: str = "Hello"
    s_aug += " "
    s_aug += "World"
    assert s_aug == "Hello World", "string += failed"

    repeat: str = "ab"
    repeat *= 3
    assert repeat == "ababab", "string *= failed"

    counter = Counter(0)
    counter.count += 10
    assert counter.count == 10, "counter.count += 10 failed"

    counter.increment()
    assert counter.count == 11, "counter.increment() failed"

    counter.add(5)
    assert counter.count == 16, "counter.add(5) failed"

    stats = Stats(100)
    stats.value -= 20
    assert stats.value == 80, "stats.value -= 20 failed"

    stats.value *= 2
    assert stats.value == 160, "stats.value *= 2 failed"

    stats.value //= 5
    assert stats.value == 32, "stats.value //= 5 failed"

# Bitwise operations (int)
def test_bitwise() -> None:
    assert (12 & 10) == 8
    assert (12 | 3) == 15
    assert (12 ^ 10) == 6
    assert (1 << 4) == 16
    assert (64 >> 2) == 16

# Augmented assignment for bitwise and shifts
def test_augmented_bitwise() -> None:
    a: int = 12
    a &= 10
    assert a == 8

    b: int = 12
    b |= 3
    assert b == 15

    c: int = 12
    c ^= 10
    assert c == 6

    d: int = 1
    d <<= 4
    assert d == 16

    e: int = 64
    e >>= 2
    assert e == 16

# Run all tests
test_arithmetic()
test_comparison()
test_boolean()
test_set_ops()
test_augmented_assignments()
test_bitwise()
test_augmented_bitwise()

print("All operator tests passed!")
"#,
        Some("All operator tests passed!"),
    );
}
