# Advanced operator coverage that is intentionally NOT duplicated in core_types.py.

# Membership operators (in / not in) across built-in container types.
def test_membership() -> None:
    vals: list[int] = [1, 2, 3]
    assert 2 in vals, "2 must be present in list [1, 2, 3]"
    assert 4 not in vals, "4 must be absent from list [1, 2, 3]"

    text: str = "abc"
    assert "a" in text, "'a' must be present in string 'abc'"
    assert "d" not in text, "'d' must be absent from string 'abc'"

    d: dict[str, int] = {"a": 1}
    assert "a" in d, "dictionary key 'a' must exist"
    assert "b" not in d, "dictionary key 'b' must not exist"

    s: set[int] = {1, 2}
    assert 2 in s, "2 must be present in set {1, 2}"
    assert 3 not in s, "3 must be absent from set {1, 2}"


# Operator precedence checks that are not spelled out in core_types.py.
def test_precedence() -> None:
    assert 2 + 3 * 4 == 14, "multiplication must bind tighter than addition"
    assert (2 + 3) * 4 == 20, "parentheses must override default precedence"
    assert 18 // 3 + 2 == 8, "floor-division then addition should evaluate to 8"
    assert 18 // (3 + 2) == 3, "parenthesized divisor should produce 3"


# Set algebra operators.
def test_set_ops() -> None:
    a: set[int] = {1, 2, 3}
    b: set[int] = {2, 3, 4}

    union_result: set[int] = a | b
    assert union_result == {1, 2, 3, 4}, "set union result is incorrect"

    inter_result: set[int] = a & b
    assert inter_result == {2, 3}, "set intersection result is incorrect"

    diff_result: set[int] = a - b
    assert diff_result == {1}, "set difference result is incorrect"

    sym_diff: set[int] = a ^ b
    assert sym_diff == {1, 4}, "set symmetric difference result is incorrect"

    empty: set[int] = set()
    c: set[int] = {1, 2}
    assert (empty | c) == c, "empty union set should be the original set"
    assert (empty & c) == empty, "empty intersection should stay empty"
    assert (c - empty) == c, "subtracting empty set should keep all elements"
    assert (empty - c) == empty, "empty minus any set should stay empty"
    assert (empty ^ c) == c, "empty symmetric difference should be original set"


def test_python_modulo_and_set_ordering() -> None:
    # Python modulo keeps the sign of the divisor.
    assert -7 % 3 == 2, "-7 % 3 should equal 2"
    assert -7.0 % 3.0 == 2.0, "-7.0 % 3.0 should equal 2.0"

    left: set[int] = {1, 2}
    right: set[int] = {1, 2, 3}
    assert left <= right, "left must be subset of right"
    assert not (left >= right), "left must not be superset of right"
    assert left < right, "left must be strict subset of right"
    assert right > left, "right must be strict superset of left"

    # Empty dict/set must be falsy.
    assert bool({}) == False, "bool({}) should be False"
    assert bool(set()) == False, "bool(set()) should be False"


# Helper classes for attribute augmented-assignment tests.
class Counter:
    count: int

    def __init__(self, start: int) -> None:
        self.count = start


class Stats:
    value: int

    def __init__(self, v: int) -> None:
        self.value = v


# Augmented assignment operators on scalars, indexed values, and attributes.
def test_augmented_assignments() -> None:
    x_aug: int = 10
    x_aug += 5
    assert x_aug == 15, "x += 5 should produce 15"

    x_aug -= 3
    assert x_aug == 12, "x -= 3 should produce 12"

    x_aug *= 2
    assert x_aug == 24, "x *= 2 should produce 24"

    y_aug: int = 25
    y_aug //= 3
    assert y_aug == 8, "25 //= 3 should produce 8"

    z_aug: int = 17
    z_aug %= 5
    assert z_aug == 2, "17 %= 5 should produce 2"

    p_aug: int = 2
    p_aug **= 3
    assert p_aug == 8, "2 **= 3 should produce 8"

    f_aug: float = 10.0
    f_aug += 2.5
    assert f_aug == 12.5, "10.0 += 2.5 should produce 12.5"

    f_aug -= 3.5
    assert f_aug == 9.0, "12.5 -= 3.5 should produce 9.0"

    f_aug *= 2.0
    assert f_aug == 18.0, "9.0 *= 2.0 should produce 18.0"

    f_aug /= 3.0
    assert f_aug == 6.0, "18.0 /= 3.0 should produce 6.0"

    g_aug: float = 17.0
    g_aug //= 3.0
    assert g_aug == 5.0, "17.0 //= 3.0 should produce 5.0"

    h_aug: float = 17.5
    h_aug %= 5.0
    assert h_aug == 2.5, "17.5 %= 5.0 should produce 2.5"

    total_aug: int = 0
    for i_aug in range(1, 6):
        total_aug += i_aug
    assert total_aug == 15, "sum of range(1, 6) must be 15"

    product_aug: int = 1
    for i_aug in range(1, 5):
        product_aug *= i_aug
    assert product_aug == 24, "product of range(1, 5) must be 24"

    nums_aug: list[int] = [10, 20, 30]
    nums_aug[0] += 5
    nums_aug[1] -= 5
    nums_aug[2] *= 2
    assert nums_aug == [15, 15, 60], "list element augmented updates are incorrect"

    idx: int = 1
    nums_aug[idx] += 10
    assert nums_aug[1] == 25, "indexed list augmented update is incorrect"

    s_aug: str = "Hello"
    s_aug += " "
    s_aug += "World"
    assert s_aug == "Hello World", "string += concatenation failed"

    repeat: str = "ab"
    repeat *= 3
    assert repeat == "ababab", "string *= repetition failed"

    counter = Counter(0)
    counter.count += 10
    assert counter.count == 10, "attribute += on Counter.count failed"

    stats = Stats(100)
    stats.value -= 20
    stats.value *= 2
    stats.value //= 5
    assert stats.value == 32, "chained attribute augmented assignments failed"


# Augmented bitwise and shift operators.
def test_augmented_bitwise() -> None:
    a: int = 12
    a &= 10
    assert a == 8, "12 &= 10 should produce 8"

    b: int = 12
    b |= 3
    assert b == 15, "12 |= 3 should produce 15"

    c: int = 12
    c ^= 10
    assert c == 6, "12 ^= 10 should produce 6"

    d: int = 1
    d <<= 4
    assert d == 16, "1 <<= 4 should produce 16"

    e: int = 64
    e >>= 2
    assert e == 16, "64 >>= 2 should produce 16"


# Python true division (/) always returns float, even for int operands.
def test_truediv() -> None:
    assert 1 / 2 == 0.5, "1 / 2 should be 0.5"
    assert 10 / 3 == 10.0 / 3.0, "int truediv must match float truediv"
    assert 6 / 2 == 3.0, "6 / 2 should be 3.0 (float)"
    assert 7 / 2 == 3.5, "7 / 2 should be 3.5"
    x: int = 9
    y: int = 4
    result: float = x / y
    assert result == 2.25, "9 / 4 should be 2.25"


# Run all operator tests in this file.
test_membership()
test_precedence()
test_set_ops()
test_python_modulo_and_set_ordering()
test_augmented_assignments()
test_augmented_bitwise()
test_truediv()

print("All operator tests passed!")
