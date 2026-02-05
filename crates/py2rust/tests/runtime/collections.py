# List operations
def test_lists() -> None:
    # Creation and indexing (including negative indices)
    nums: list[int] = [1, 2, 3, 4, 5]
    assert len(nums) == 5
    assert nums[0] == 1
    # Extra index coverage from list_tuple script.
    assert nums[2] == 3
    assert nums[4] == 5
    assert nums[-1] == 5
    assert nums[-2] == 4

    # Single element list
    single_list: list[int] = [42]
    assert len(single_list) == 1
    assert single_list[0] == 42

    # Larger list
    big_list: list[int] = [10, 20, 30, 40, 50, 60, 70, 80, 90, 100]
    assert len(big_list) == 10
    assert big_list[0] == 10
    assert big_list[5] == 60
    assert big_list[9] == 100
    assert big_list[-1] == 100

    # Append
    nums.append(6)
    assert len(nums) == 6
    assert nums[5] == 6

    # Empty list type inference via append.
    inferred: list[str] = []
    inferred.append(str(1))
    inferred.append(str(2))
    assert inferred == ["1", "2"]

    # Append sequence coverage (mirrors list_tuple script).
    items: list[int] = [1, 2]
    items.append(3)
    assert len(items) == 3
    assert items[2] == 3
    items.append(4)
    assert len(items) == 4
    assert items[3] == 4

    # Extend
    extend_list: list[int] = [1, 2, 3]
    extend_list.extend([4, 5, 6])
    assert len(extend_list) == 6
    assert extend_list == [1, 2, 3, 4, 5, 6]
    assert extend_list[3] == 4
    assert extend_list[5] == 6

    # Pop
    values: list[int] = [10, 20, 30, 40]
    last: int = values.pop()
    assert last == 40
    assert len(values) == 3
    first: int = values.pop(0)
    assert first == 10
    assert len(values) == 2
    assert values[0] == 20

    # Empty list
    # Match list_tuple coverage for empty list length.
    empty_list: list[int] = []
    assert len(empty_list) == 0
    empty: list[int] = []
    assert len(empty) == 0
    empty.append(42)
    assert len(empty) == 1
    assert empty[0] == 42

    # Slicing (positive, negative, step)
    slice_nums: list[int] = [0, 1, 2, 3, 4, 5]
    slice1: list[int] = slice_nums[1:4]
    assert len(slice1) == 3
    assert slice1[0] == 1
    assert slice1[1] == 2
    assert slice1[2] == 3

    slice2: list[int] = slice_nums[:3]
    assert len(slice2) == 3
    assert slice2[0] == 0
    assert slice2[1] == 1
    assert slice2[2] == 2

    slice3: list[int] = slice_nums[3:]
    assert len(slice3) == 3
    assert slice3[0] == 3
    assert slice3[1] == 4
    assert slice3[2] == 5

    evens: list[int] = slice_nums[::2]
    assert len(evens) == 3
    assert evens[0] == 0
    assert evens[1] == 2
    assert evens[2] == 4

    every_third: list[int] = slice_nums[::3]
    assert len(every_third) == 2
    assert every_third[0] == 0
    assert every_third[1] == 3

    last_two: list[int] = slice_nums[-2:]
    assert len(last_two) == 2
    assert last_two[0] == 4
    assert last_two[1] == 5

    without_last: list[int] = slice_nums[:-1]
    assert len(without_last) == 5
    assert without_last == [0, 1, 2, 3, 4]

    middle_neg: list[int] = slice_nums[-4:-1]
    assert middle_neg == [2, 3, 4]

    empty_slice: list[int] = slice_nums[3:3]
    assert len(empty_slice) == 0

    # Insert
    data: list[int] = [1, 3, 4]
    data.insert(1, 2)
    assert len(data) == 4
    assert data[0] == 1
    assert data[1] == 2
    assert data[2] == 3
    assert data[3] == 4

    # Clear
    to_clear: list[int] = [1, 2, 3, 4, 5]
    to_clear.clear()
    assert len(to_clear) == 0

    # Copy
    original: list[int] = [1, 2, 3]
    copied: list[int] = original.copy()
    assert len(copied) == 3
    assert copied[0] == 1
    assert copied[1] == 2
    assert copied[2] == 3
    copied.append(4)
    assert len(copied) == 4
    assert len(original) == 3

    # Reverse
    nums2: list[int] = [1, 2, 3, 4, 5]
    nums2.reverse()
    assert len(nums2) == 5
    assert nums2[0] == 5
    assert nums2[1] == 4
    assert nums2[2] == 3
    assert nums2[3] == 2
    assert nums2[4] == 1

    # Single element reverse
    single_rev: list[int] = [42]
    single_rev.reverse()
    assert len(single_rev) == 1
    assert single_rev[0] == 42

    # Combined operations
    build: list[int] = []
    build.append(1)
    build.append(2)
    build.append(3)
    assert len(build) == 3

    subset: list[int] = build[1:].copy()
    assert len(subset) == 2
    assert subset[0] == 2
    assert subset[1] == 3

    subset.reverse()
    assert subset[0] == 3
    assert subset[1] == 2

    # list.index()
    idx_list: list[int] = [10, 20, 30, 20, 40]
    assert idx_list.index(10) == 0
    assert idx_list.index(20) == 1
    assert idx_list.index(40) == 4

    # list.count()
    count_list: list[int] = [1, 2, 2, 3, 2, 4]
    assert count_list.count(2) == 3
    assert count_list.count(1) == 1
    assert count_list.count(99) == 0

    # List equality and min/max
    list_a: list[int] = [1, 2, 3]
    list_b: list[int] = [1, 2, 3]
    list_c: list[int] = [1, 2, 4]
    assert list_a == list_b
    assert list_a != list_c

    empty_a: list[int] = []
    empty_b: list[int] = []
    assert empty_a == empty_b

    str_list_a: list[str] = ["hello", "world"]
    str_list_b: list[str] = ["hello", "world"]
    assert str_list_a == str_list_b

    numbers_for_minmax: list[int] = [10, 20, 5, 40, 15]
    assert min(numbers_for_minmax) == 5
    assert max(numbers_for_minmax) == 40

    floats: list[float] = [1.5, 2.7, 3.14, 4.0, 5.5]
    assert floats == [1.5, 2.7, 3.14, 4.0, 5.5]
    assert min(floats) == 1.5
    assert max(floats) == 5.5
    assert len(floats) == 5

    # List modification (element assignment)
    fruits: list[str] = ["apple", "banana", "cherry"]
    assert fruits == ["apple", "banana", "cherry"]
    fruits[1] = "blueberry"
    assert fruits == ["apple", "blueberry", "cherry"]

    numbers_mod: list[int] = [1, 2, 3, 4, 5]
    assert numbers_mod == [1, 2, 3, 4, 5]
    numbers_mod[0] = 10
    assert numbers_mod == [10, 2, 3, 4, 5]

    # list.sort()
    sort_nums: list[int] = [3, 1, 4, 1, 5, 9, 2, 6]
    sort_nums.sort()
    assert sort_nums == [1, 1, 2, 3, 4, 5, 6, 9]

    sorted_nums: list[int] = [1, 2, 3, 4, 5]
    sorted_nums.sort()
    assert sorted_nums == [1, 2, 3, 4, 5]

    reverse_nums: list[int] = [5, 4, 3, 2, 1]
    reverse_nums.sort()
    assert reverse_nums == [1, 2, 3, 4, 5]

    # List unpacking
    pairs: list[list[int]] = [[10, 20], [30, 40]]
    pair_first, pair_second = pairs
    assert pair_first[0] == 10
    assert pair_first[1] == 20
    assert pair_second[0] == 30
    assert pair_second[1] == 40

# String operations
def test_strings() -> None:
    s: str = "hello"
    assert len(s) == 5

    s2: str = "world"
    assert len(s2) == 5

    # Unicode length counts codepoints, not bytes
    uni: str = "\u00e9"
    assert len(uni) == 1
    uni2: str = "\u00e9\u00e9"
    assert len(uni2) == 2

    # String concatenation
    combined: str = s + " " + s2
    assert len(combined) == 11

# Tuple operations
def test_tuples() -> None:
    # Creation and indexing (including negative indices)
    point: tuple[int, int] = (10, 20)
    assert point[0] == 10
    assert point[1] == 20
    assert point[-1] == 20
    assert point[-2] == 10
    assert len(point) == 2

    single_tuple: tuple[int] = (99,)
    assert single_tuple[0] == 99
    assert len(single_tuple) == 1

    triple: tuple[int, int, int] = (1, 2, 3)
    assert triple[0] == 1
    assert triple[1] == 2
    assert triple[2] == 3
    assert len(triple) == 3

    # Mixed-type tuple indexing
    mixed_tuple1: tuple[str, int] = ("hello", 42)
    assert mixed_tuple1[0] == "hello"
    assert mixed_tuple1[1] == 42

    mixed_tuple2: tuple[int, str, bool] = (100, "world", True)
    assert mixed_tuple2[0] == 100
    assert mixed_tuple2[1] == "world"
    assert mixed_tuple2[2] == True
    assert mixed_tuple2[-1] == True
    assert mixed_tuple2[-2] == "world"
    assert mixed_tuple2[-3] == 100

    mixed_tuple3: tuple[str, float, int] = ("pi", 3.14, 7)
    assert mixed_tuple3[0] == "pi"
    assert mixed_tuple3[1] == 3.14
    assert mixed_tuple3[2] == 7

    # Nested tuple
    nested: tuple[int, tuple[int, int]] = (1, (2, 3))
    assert nested[0] == 1
    inner: tuple[int, int] = nested[1]
    assert inner[0] == 2
    assert inner[1] == 3

    # Tuple slicing
    tuple_nums: tuple[int, int, int, int, int, int] = (0, 1, 2, 3, 4, 5)

    tuple_slice1: tuple[int, int, int, int, int, int] = tuple_nums[1:4]
    assert len(tuple_slice1) == 3
    assert tuple_slice1[0] == 1
    assert tuple_slice1[1] == 2
    assert tuple_slice1[2] == 3

    tuple_slice2: tuple[int, int, int, int, int, int] = tuple_nums[:3]
    assert len(tuple_slice2) == 3
    assert tuple_slice2[0] == 0
    assert tuple_slice2[1] == 1
    assert tuple_slice2[2] == 2

    tuple_slice3: tuple[int, int, int, int, int, int] = tuple_nums[3:]
    assert len(tuple_slice3) == 3
    assert tuple_slice3[0] == 3
    assert tuple_slice3[1] == 4
    assert tuple_slice3[2] == 5

    full: tuple[int, int, int, int, int, int] = tuple_nums[:]
    assert len(full) == 6
    assert full[0] == 0
    assert full[5] == 5

    tuple_evens: tuple[int, int, int, int, int, int] = tuple_nums[::2]
    assert len(tuple_evens) == 3
    assert tuple_evens[0] == 0
    assert tuple_evens[1] == 2
    assert tuple_evens[2] == 4

    tuple_every_third: tuple[int, int, int, int, int, int] = tuple_nums[::3]
    assert len(tuple_every_third) == 2
    assert tuple_every_third[0] == 0
    assert tuple_every_third[1] == 3

    stepped: tuple[int, int, int, int, int, int] = tuple_nums[1:5:2]
    assert len(stepped) == 2
    assert stepped[0] == 1
    assert stepped[1] == 3

    tuple_last_two: tuple[int, int, int, int, int, int] = tuple_nums[-2:]
    assert len(tuple_last_two) == 2
    assert tuple_last_two[0] == 4
    assert tuple_last_two[1] == 5

    all_but_last: tuple[int, int, int, int, int, int] = tuple_nums[:-1]
    assert len(all_but_last) == 5
    assert all_but_last[0] == 0
    assert all_but_last[4] == 4

    reversed_tuple: tuple[int, int, int, int, int, int] = tuple_nums[::-1]
    assert len(reversed_tuple) == 6
    assert reversed_tuple[0] == 5
    assert reversed_tuple[1] == 4
    assert reversed_tuple[2] == 3
    assert reversed_tuple[3] == 2
    assert reversed_tuple[4] == 1
    assert reversed_tuple[5] == 0

    reverse_evens: tuple[int, int, int, int, int, int] = tuple_nums[::-2]
    assert len(reverse_evens) == 3
    assert reverse_evens[0] == 5
    assert reverse_evens[1] == 3
    assert reverse_evens[2] == 1

    tuple_empty_slice: tuple[int, int, int, int, int, int] = tuple_nums[3:3]
    assert len(tuple_empty_slice) == 0

    empty_range: tuple[int, int, int, int, int, int] = tuple_nums[4:2]
    assert len(empty_range) == 0

    single_t: tuple[int] = (42,)
    single_t_slice: tuple[int] = single_t[:]
    assert len(single_t_slice) == 1
    assert single_t_slice[0] == 42

    beyond_end: tuple[int, int, int, int, int, int] = tuple_nums[3:100]
    assert len(beyond_end) == 3
    assert beyond_end[0] == 3
    assert beyond_end[1] == 4
    assert beyond_end[2] == 5

    start_beyond: tuple[int, int, int, int, int, int] = tuple_nums[100:]
    assert len(start_beyond) == 0

    # Tuple equality
    tuple_eq_a: tuple[int, int, int] = (1, 2, 3)
    tuple_eq_b: tuple[int, int, int] = (1, 2, 3)
    tuple_eq_c: tuple[int, int, int] = (1, 2, 4)
    assert tuple_eq_a == tuple_eq_b
    assert tuple_eq_a != tuple_eq_c

    empty_tuple_a: tuple[()] = ()
    empty_tuple_b: tuple[()] = ()
    assert empty_tuple_a == empty_tuple_b

    str_tuple_a: tuple[str, str, str] = ("a", "b", "c")
    str_tuple_b: tuple[str, str, str] = ("a", "b", "c")
    str_tuple_c: tuple[str, str, str] = ("a", "b", "d")
    assert str_tuple_a == str_tuple_b
    assert str_tuple_a != str_tuple_c

    mixed_tuple_a: tuple[int, str, int] = (1, "hello", 3)
    mixed_tuple_b: tuple[int, str, int] = (1, "hello", 3)
    mixed_tuple_c: tuple[int, str, int] = (1, "world", 3)
    assert mixed_tuple_a == mixed_tuple_b
    assert mixed_tuple_a != mixed_tuple_c

    nested_tuple_a: tuple[tuple[int, int], tuple[int, int]] = ((1, 2), (3, 4))
    nested_tuple_b: tuple[tuple[int, int], tuple[int, int]] = ((1, 2), (3, 4))
    nested_tuple_c: tuple[tuple[int, int], tuple[int, int]] = ((1, 2), (3, 5))
    assert nested_tuple_a == nested_tuple_b
    assert nested_tuple_a != nested_tuple_c

    len_tuple_a: tuple[int, int, int] = (1, 2, 3)
    len_tuple_b: tuple[int, int] = (1, 2)
    assert len_tuple_a != len_tuple_b

    single_tuple_eq_a: tuple[int] = (42,)
    single_tuple_eq_b: tuple[int] = (42,)
    single_tuple_eq_c: tuple[int] = (99,)
    assert single_tuple_eq_a == single_tuple_eq_b
    assert single_tuple_eq_a != single_tuple_eq_c

    # Tuple ordering
    tuple_ord_1: tuple[int, int, int] = (1, 2, 3)
    tuple_ord_2: tuple[int, int, int] = (1, 2, 4)
    tuple_ord_3: tuple[int, int, int] = (1, 3, 0)
    tuple_ord_4: tuple[int, int, int] = (1, 2, 3)

    assert tuple_ord_1 < tuple_ord_2
    assert not tuple_ord_2 < tuple_ord_1
    assert tuple_ord_1 < tuple_ord_3
    assert not tuple_ord_3 < tuple_ord_1
    assert not tuple_ord_1 < tuple_ord_4

    assert tuple_ord_1 <= tuple_ord_2
    assert not tuple_ord_2 <= tuple_ord_1
    assert tuple_ord_1 <= tuple_ord_4

    assert tuple_ord_2 > tuple_ord_1
    assert not tuple_ord_1 > tuple_ord_2
    assert tuple_ord_3 > tuple_ord_1
    assert not tuple_ord_1 > tuple_ord_3
    assert not tuple_ord_1 > tuple_ord_4

    assert tuple_ord_2 >= tuple_ord_1
    assert not tuple_ord_1 >= tuple_ord_2
    assert tuple_ord_1 >= tuple_ord_4

    tuple_short: tuple[int, int] = (1, 2)
    tuple_long: tuple[int, int, int] = (1, 2, 3)
    tuple_short_larger: tuple[int, int] = (1, 3)

    assert tuple_short < tuple_long
    assert not tuple_long < tuple_short
    assert tuple_short_larger > tuple_long
    assert not tuple_long > tuple_short_larger

    assert tuple_short <= tuple_long
    assert not tuple_long <= tuple_short
    assert tuple_short_larger >= tuple_long
    assert not tuple_long >= tuple_short_larger

    tuple_empty: tuple[()] = ()
    tuple_nonempty: tuple[int] = (1,)

    assert tuple_empty < tuple_nonempty
    assert not tuple_nonempty < tuple_empty
    assert tuple_empty <= tuple_nonempty
    assert not tuple_nonempty <= tuple_empty
    assert tuple_nonempty > tuple_empty
    assert not tuple_empty > tuple_nonempty
    assert tuple_nonempty >= tuple_empty
    assert not tuple_empty >= tuple_nonempty

    tuple_empty2: tuple[()] = ()
    assert not tuple_empty < tuple_empty2
    assert tuple_empty <= tuple_empty2
    assert not tuple_empty > tuple_empty2
    assert tuple_empty >= tuple_empty2

    tuple_str_1: tuple[str, str] = ("a", "b")
    tuple_str_2: tuple[str, str] = ("a", "c")
    tuple_str_3: tuple[str, str] = ("b", "a")

    assert tuple_str_1 < tuple_str_2
    assert not tuple_str_2 < tuple_str_1
    assert tuple_str_1 < tuple_str_3
    assert not tuple_str_3 < tuple_str_1

    tuple_nested_1: tuple[int, tuple[int, int]] = (1, (2, 3))
    tuple_nested_2: tuple[int, tuple[int, int]] = (1, (2, 4))
    tuple_nested_3: tuple[int, tuple[int, int]] = (1, (3, 0))

    assert tuple_nested_1 < tuple_nested_2
    assert not tuple_nested_2 < tuple_nested_1
    assert tuple_nested_1 < tuple_nested_3
    assert not tuple_nested_3 < tuple_nested_1

    tuple_single_1: tuple[int] = (5,)
    tuple_single_2: tuple[int] = (10,)
    tuple_single_3: tuple[int] = (5,)

    assert tuple_single_1 < tuple_single_2
    assert not tuple_single_2 < tuple_single_1
    assert not tuple_single_1 < tuple_single_3
    assert tuple_single_1 <= tuple_single_3
    assert tuple_single_2 > tuple_single_1
    assert tuple_single_2 >= tuple_single_1

    tuple_float_1: tuple[float, float] = (1.5, 2.5)
    tuple_float_2: tuple[float, float] = (1.5, 3.0)
    tuple_float_3: tuple[float, float] = (2.0, 1.0)

    assert tuple_float_1 < tuple_float_2
    assert not tuple_float_2 < tuple_float_1
    assert tuple_float_1 < tuple_float_3
    assert not tuple_float_3 < tuple_float_1

    tuple_bool_1: tuple[bool, bool] = (False, False)
    tuple_bool_2: tuple[bool, bool] = (False, True)
    tuple_bool_3: tuple[bool, bool] = (True, False)

    assert tuple_bool_1 < tuple_bool_2
    assert not tuple_bool_2 < tuple_bool_1
    assert tuple_bool_1 < tuple_bool_3
    assert not tuple_bool_3 < tuple_bool_1

    tuple_mixed_1: tuple[int, str, float] = (1, "a", 2.5)
    tuple_mixed_2: tuple[int, str, float] = (1, "a", 3.0)
    tuple_mixed_3: tuple[int, str, float] = (1, "b", 1.0)

    assert tuple_mixed_1 < tuple_mixed_2
    assert not tuple_mixed_2 < tuple_mixed_1
    assert tuple_mixed_1 < tuple_mixed_3
    assert not tuple_mixed_3 < tuple_mixed_1

    # Nested tuple unpacking
    nested1: tuple[int, tuple[int, int]] = (1, (2, 3))
    a1, (b1, c1) = nested1
    assert a1 == 1
    assert b1 == 2
    assert c1 == 3

    # Deeper nesting (3 levels)
    nested2: tuple[int, tuple[int, tuple[int, int]]] = (10, (20, (30, 40)))
    x, (y, (z, w)) = nested2
    assert x == 10
    assert y == 20
    assert z == 30
    assert w == 40

    # Mixed tuple/list nested unpacking
    mixed_nested: tuple[int, list[int]] = (1, [2, 3])
    g, [h, i] = mixed_nested
    assert g == 1
    assert h == 2
    assert i == 3

    # Multiple nested groups
    multi_nested: tuple[tuple[int, int], tuple[int, int]] = ((1, 2), (3, 4))
    (m1, m2), (m3, m4) = multi_nested
    assert m1 == 1 and m2 == 2 and m3 == 3 and m4 == 4

# Container string representations (exercise print-style formatting)
def test_printing() -> None:
    print_int_list: list[int] = [1, 2, 3]
    assert str(print_int_list) == "[1, 2, 3]"

    print_str_list: list[str] = ["a", "b"]
    assert str(print_str_list) == '["a", "b"]'

    print_empty_list: list[int] = []
    assert str(print_empty_list) == "[]"

    print_nested_list: list[list[int]] = [[1, 2], [3, 4]]
    assert str(print_nested_list) == "[[1, 2], [3, 4]]"

    print_mixed_list = [1, 2, ["string"]]
    assert str(print_mixed_list) == '[1, 2, ["string"]]'

    print_tuple: tuple[int, int, int] = (1, 2, 3)
    assert str(print_tuple) == "(1, 2, 3)"

    print_single_tuple: tuple[int] = (42,)
    assert str(print_single_tuple) == "(42,)"

    print_str_tuple: tuple[str, str] = ("hello", "world")
    assert str(print_str_tuple) == '("hello", "world")'

# Dictionary operations
def test_dicts() -> None:
    d: dict[str, int] = {"a": 1, "b": 2}
    assert len(d) == 2
    assert d["a"] == 1
    assert d["b"] == 2

    # Add new key
    d["c"] = 3
    assert len(d) == 3
    assert d["c"] == 3

    # Update existing key
    d["a"] = 10
    assert d["a"] == 10

    # dict() constructors
    empty: dict[str, int] = dict()
    assert len(empty) == 0

    kw: dict[str, int] = dict(a=1, b=2)
    assert kw["a"] == 1
    assert kw["b"] == 2

    pairs: list[tuple[str, int]] = [("x", 10), ("y", 20)]
    from_pairs: dict[str, int] = dict(pairs)
    assert from_pairs["x"] == 10
    assert from_pairs["y"] == 20

    copied: dict[str, int] = dict(d)
    assert copied["a"] == 10
    assert copied["b"] == 2

# For-loop tuple unpacking
def test_for_tuple_unpacking() -> None:
    # Basic tuple unpacking in for loop
    pairs: list[tuple[int, int]] = [(1, 2), (3, 4), (5, 6)]
    total_a: int = 0
    total_b: int = 0
    for a, b in pairs:
        total_a = total_a + a
        total_b = total_b + b
    assert total_a == 9   # 1+3+5
    assert total_b == 12  # 2+4+6

    # Tuple unpacking with enumerate
    indexed: list[tuple[int, str]] = []
    words: list[str] = ["foo", "bar", "baz"]
    for i, word in enumerate(words):
        indexed.append((i, word))
    assert indexed == [(0, "foo"), (1, "bar"), (2, "baz")]

    # Triple unpacking
    triples: list[tuple[int, int, int]] = [(1, 2, 3), (4, 5, 6)]
    sum_first: int = 0
    sum_second: int = 0
    sum_third: int = 0
    for x, y, z in triples:
        sum_first = sum_first + x
        sum_second = sum_second + y
        sum_third = sum_third + z
    assert sum_first == 5
    assert sum_second == 7
    assert sum_third == 9

    # Mixed type tuple unpacking
    mixed: list[tuple[str, int]] = [("a", 1), ("b", 2), ("c", 3)]
    keys: list[str] = []
    vals: list[int] = []
    for k, v in mixed:
        keys.append(k)
        vals.append(v)
    assert keys == ["a", "b", "c"]
    assert vals == [1, 2, 3]

    # Empty iteration
    empty_pairs: list[tuple[int, int]] = []
    count: int = 0
    for p, q in empty_pairs:
        count = count + 1
    assert count == 0

    # Single element iteration
    single_pair: list[tuple[int, int]] = [(10, 20)]
    for s1, s2 in single_pair:
        assert s1 == 10
        assert s2 == 20

    # Build collection with tuple unpacking
    source: list[tuple[int, int]] = [(1, 10), (2, 20), (3, 30)]
    products: list[int] = []
    for m, n in source:
        products.append(m * n)
    assert products == [10, 40, 90]

    # Tuple unpacking with condition
    filtered_sum: int = 0
    for aa, bb in [(1, 2), (3, 4), (5, 6), (7, 8)]:
        if aa > 2:
            filtered_sum = filtered_sum + bb
    assert filtered_sum == 18  # 4+6+8

# Run all tests
test_lists()
test_strings()
test_tuples()
test_printing()
test_dicts()
test_for_tuple_unpacking()

print("All collection tests passed!")
