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

    # Float arithmetic
    float_x: float = 7.5
    float_y: float = 2.5
    assert float_x + float_y == 10.0
    assert float_x - float_y == 5.0
    assert float_x * float_y == 18.75
    assert float_x / float_y == 3.0
    assert round(float_x / float_y) == 3

    # String multiplication
    str_s: str = "Hello"
    assert str_s * 3 == "HelloHelloHello"
    assert "ab" * 2 == "abab"

    # String multiplication edge cases
    assert "x" * 0 == ""
    assert "hello" * 0 == ""
    assert "x" * -1 == ""

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

# Identity operators
def test_identity() -> None:
    # Lists
    list1: list[int] = [1, 2, 3]
    list2: list[int] = [1, 2, 3]
    list3: list[int] = list1
    assert list1 is list3
    assert list1 is not list2
    assert not (list1 is list2)
    assert not (list1 is not list3)

    # Strings
    str1: str = "hello"
    str2: str = "hello"
    str3: str = str1
    assert str1 is str3

    # None
    none1: None = None
    none2: None = None
    assert none1 is none2
    assert none1 is not list1

    # Dicts
    dict1: dict[str, int] = {"a": 1}
    dict2: dict[str, int] = {"a": 1}
    dict3: dict[str, int] = dict1
    assert dict1 is dict3
    assert dict1 is not dict2

# Chained comparisons
chain_call_count: int = 0

def chain_increment_and_return(val: int) -> int:
    global chain_call_count
    chain_call_count = chain_call_count + 1
    return val

def test_chained_comparisons() -> None:
    # Basic chained comparison
    chain_x: int = 5
    assert 1 < chain_x < 10
    assert not (10 < chain_x < 20)

    # Chained comparison with variables
    chain_a: int = 1
    chain_b: int = 2
    chain_c: int = 3
    assert chain_a < chain_b < chain_c
    assert not (chain_c < chain_b < chain_a)

    # Equality chains
    eq_a: int = 5
    eq_b: int = 5
    eq_c: int = 5
    assert eq_a == eq_b == eq_c

    eq_d: int = 6
    assert not (eq_a == eq_b == eq_d)

    # Mixed operators
    assert 1 < 2 <= 2 < 3
    assert 1 <= 1 < 2 <= 2

    # Four-way chains
    assert 1 < 2 < 3 < 4
    assert not (1 < 2 < 3 < 2)

    # Float comparisons in chains
    f_a: float = 1.0
    f_b: float = 2.5
    f_c: float = 5.0
    assert f_a < f_b < f_c
    assert 0.5 < f_a < f_b

    # Edge cases: boundary conditions
    assert 0 <= 0 < 1
    assert not (0 < 0 < 1)
    assert 1 <= 1 <= 1

    # Mixed int and float in chains
    assert 1 < 2.5 < 4

    # Short-circuit verification
    global chain_call_count
    chain_call_count = 0
    chain_result: bool = 10 < chain_increment_and_return(5) < 3
    assert chain_call_count == 1
    assert not chain_result

    chain_call_count = 0
    chain_result = 1 < chain_increment_and_return(5) < 10
    assert chain_call_count == 1
    assert chain_result

    # Greater-than chains
    assert 10 > 5 > 1
    assert not (1 > 5 > 10)

    # Not-equal chains
    assert 1 != 2 != 3
    assert not (1 != 2 != 2)

    # Multiple function calls in chain
    chain_call_count = 0
    chain_result = (
        chain_increment_and_return(1)
        < chain_increment_and_return(2)
        < chain_increment_and_return(3)
    )
    assert chain_call_count == 3
    assert chain_result

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

# Short-circuit semantics and truthiness returns
def test_short_circuit() -> None:
    x: int = 15
    y: int = 10
    z: int = 5

    assert x > y and y > z
    assert x > y or y < z
    assert not (x < y)

    assert x > 10 and y < 20
    assert x > 10 and y < 20 or z == 5

    sc_and_1: int = 5 and 10
    assert sc_and_1 == 10

    sc_and_2: int = 0 and 10
    assert sc_and_2 == 0

    sc_and_3: int = 42 and 99
    assert sc_and_3 == 99

    sc_or_1: int = 5 or 10
    assert sc_or_1 == 5

    sc_or_2: int = 0 or 10
    assert sc_or_2 == 10

    sc_or_3: int = 42 or 99
    assert sc_or_3 == 42

    sc_str_or: str = "" or "default"
    assert sc_str_or == "default"

    sc_str_and: str = "hello" and "world"
    assert sc_str_and == "world"

    sc_method: str = ("" or "hello").upper()
    assert sc_method == "HELLO"

    sc_none_or_1: int = 0 or 42
    assert sc_none_or_1 == 42

    sc_none_and_1: int = 42 and 0
    assert sc_none_and_1 == 0

    a: int = 42
    neg_a: int = -a
    assert neg_a == -42
    assert neg_a + a == 0

    b: int = 5
    neg_b: int = -b
    pos_b: int = -neg_b
    assert pos_b == b

    flag1: bool = True
    flag2: bool = False
    assert not (not flag1)
    assert not flag2
    assert flag1 or flag2
    assert flag1 and not flag2

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

# Power operator
def test_power() -> None:
    # Integer powers
    pow_x: int = 2 ** 3
    assert pow_x == 8

    pow_y: int = 3 ** 4
    assert pow_y == 81

    pow_z: int = 5 ** 0
    assert pow_z == 1

    pow_w: int = 2 ** 10
    assert pow_w == 1024

    # Negative base
    pow_a: int = (-2) ** 3
    assert pow_a == -8

    pow_b: int = (-2) ** 4
    assert pow_b == 16

    # Float powers
    f1: float = 2.0 ** 3.0
    assert f1 == 8.0

    f2: float = 4.0 ** 0.5
    assert f2 == 2.0

    f3: float = 2.5 ** 2.0
    assert f3 == 6.25

    # Mixed int/float
    m1: float = 2 ** 3.0
    assert m1 == 8.0

    m2: float = 2.0 ** 3
    assert m2 == 8.0

    # Edge cases
    e1: int = 1 ** 100
    assert e1 == 1

    e2: int = 0 ** 5
    assert e2 == 0

    e3: int = 10 ** 1
    assert e3 == 10

    # Larger values
    big: int = 2 ** 20
    assert big == 1048576

    # Additional power checks
    assert 2 ** 3 == 8
    assert 3 ** 2 == 9
    assert 2 ** 0 == 1

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
    assert (~0) == -1
    assert (~(-1)) == 0
    assert (~5) == -6

    bit_a: int = 0b1100
    bit_b: int = 0b1010
    bit_result: int = bit_a & bit_b
    assert bit_result == 0b1000
    assert (15 & 7) == 7

    bit_result = bit_a | bit_b
    assert bit_result == 0b1110
    assert (8 | 4) == 12

    bit_result = bit_a ^ bit_b
    assert bit_result == 0b0110
    assert (12 ^ 10) == 6

    bit_x: int = 1
    bit_result = bit_x << 4
    assert bit_result == 16
    assert (3 << 2) == 12

    bit_y: int = 16
    bit_result = bit_y >> 2
    assert bit_result == 4
    assert (32 >> 3) == 4

    bit_z: int = 0
    bit_result = ~bit_z
    assert bit_result == -1
    bit_result = ~(-1)
    assert bit_result == 0
    n: int = 5
    bit_result = ~n
    assert bit_result == -6

    combined: int = (bit_a & bit_b) | (bit_a ^ bit_b)
    assert combined == (bit_a | bit_b)

    val: int = 1
    val = val << 3
    val = val << 1
    assert val == 16
    val = val >> 2
    assert val == 4

    neg: int = -8
    bit_result = neg >> 2
    assert bit_result == -2

    byte: int = 255
    mask: int = 0x0F
    lower: int = byte & mask
    assert lower == 15
    upper: int = (byte >> 4) & mask
    assert upper == 15

    flags: int = 0
    flags = flags | (1 << 0)
    assert flags == 1
    flags = flags | (1 << 2)
    assert flags == 5

    bit1: int = (flags >> 1) & 1
    assert bit1 == 0
    bit2: int = (flags >> 2) & 1
    assert bit2 == 1

    flags = flags & ~(1 << 0)
    assert flags == 4

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
test_power()
test_comparison()
test_identity()
test_boolean()
test_short_circuit()
test_set_ops()
test_augmented_assignments()
test_bitwise()
test_augmented_bitwise()
test_chained_comparisons()

print("All operator tests passed!")
