//! Runtime tests for collections: lists, strings, tuples, dictionaries.

use crate::common::run_py;

#[test]
fn runtime_collections_comprehensive() {
    run_py(
        "collections",
        r#"
# List operations
def test_lists() -> None:
    # Creation and indexing
    nums: list[int] = [1, 2, 3, 4, 5]
    assert len(nums) == 5
    assert nums[0] == 1
    assert nums[4] == 5

    # Append
    nums.append(6)
    assert len(nums) == 6
    assert nums[5] == 6

    # Empty list
    empty: list[int] = []
    assert len(empty) == 0
    empty.append(42)
    assert len(empty) == 1
    assert empty[0] == 42

    # Slicing with step
    slice_nums: list[int] = [0, 1, 2, 3, 4, 5]
    evens: list[int] = slice_nums[::2]
    assert len(evens) == 3
    assert evens[0] == 0
    assert evens[1] == 2
    assert evens[2] == 4

    every_third: list[int] = slice_nums[::3]
    assert len(every_third) == 2
    assert every_third[0] == 0
    assert every_third[1] == 3

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
    t: tuple[int, str] = (42, "answer")
    x: int = t[0]
    s: str = t[1]
    assert x == 42
    assert s == "answer"

    # Nested tuple
    nested: tuple[int, tuple[int, int]] = (1, (2, 3))
    assert nested[0] == 1
    inner: tuple[int, int] = nested[1]
    assert inner[0] == 2
    assert inner[1] == 3

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

# Run all tests
test_lists()
test_strings()
test_tuples()
test_dicts()

print("All collection tests passed!")
"#,
        Some("All collection tests passed!"),
    );
}
