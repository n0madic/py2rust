//! Runtime tests for comprehensions: list, set, and dict comprehensions.

use crate::common::run_py;

#[test]
fn runtime_comprehensions() {
    run_py(
        "comprehensions",
        r#"
# Basic list comprehension
def basic_list_comp() -> list[int]:
    return [x for x in range(5)]

assert basic_list_comp() == [0, 1, 2, 3, 4]

# List comprehension with expression
def squared_list() -> list[int]:
    return [x * x for x in range(5)]

assert squared_list() == [0, 1, 4, 9, 16]

# List comprehension with condition
def even_numbers() -> list[int]:
    return [x for x in range(10) if x % 2 == 0]

assert even_numbers() == [0, 2, 4, 6, 8]

# List comprehension with complex expression and condition
def complex_comp() -> list[int]:
    return [x * 2 + 1 for x in range(10) if x % 2 == 0]

assert complex_comp() == [1, 5, 9, 13, 17]

# List comprehension over a list variable
def comp_over_list() -> list[int]:
    nums: list[int] = [1, 2, 3, 4, 5]
    return [n * 10 for n in nums]

assert comp_over_list() == [10, 20, 30, 40, 50]

# List comprehension with string transformation
def string_comp() -> list[str]:
    words: list[str] = ["hello", "world"]
    return [w + "!" for w in words]

assert string_comp() == ["hello!", "world!"]

# Empty list comprehension (no elements match)
def empty_comp() -> list[int]:
    return [x for x in range(10) if x > 100]

assert empty_comp() == []

# List comprehension producing single element
def single_element_comp() -> list[int]:
    return [x for x in range(10) if x == 5]

assert single_element_comp() == [5]

# Note: Nested list comprehensions (2D grids) are not tested here because
# comparing nested Arc<Mutex<Vec<>>> lists isn't well supported yet.
# TODO: Add nested list comprehension test when comparison is improved.

# List comprehension with boolean values
def bool_comp() -> list[bool]:
    return [x > 2 for x in range(5)]

assert bool_comp() == [False, False, False, True, True]

# List comprehension with float values
def float_comp() -> list[float]:
    return [float(x) / 2.0 for x in range(5)]

assert float_comp() == [0.0, 0.5, 1.0, 1.5, 2.0]

# Set comprehension basic
def basic_set_comp() -> set[int]:
    return {x for x in range(5)}

assert basic_set_comp() == {0, 1, 2, 3, 4}

# Set comprehension with duplicates collapsed
def set_comp_dedup() -> set[int]:
    nums: list[int] = [1, 2, 2, 3, 3, 3, 4]
    return {n for n in nums}

assert set_comp_dedup() == {1, 2, 3, 4}

# Set comprehension with condition
def even_set() -> set[int]:
    return {x for x in range(10) if x % 2 == 0}

assert even_set() == {0, 2, 4, 6, 8}

# Set comprehension with transformation
def squared_set() -> set[int]:
    return {x * x for x in range(5)}

assert squared_set() == {0, 1, 4, 9, 16}

# Note: Dict comprehensions are not yet supported by the transpiler.
# TODO: Add dict comprehension tests when support is implemented.

print("All comprehension tests passed!")
"#,
        Some("All comprehension tests passed!"),
    );
}
