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

# While loop building a list from empty literal
results = []
counter = 0
while counter < 3:
    # Append strings to force list element type inference.
    results.append(str(counter))
    counter = counter + 1
assert results == ["0", "1", "2"], "while loop should build string list"

# For loop building a list from empty literal
range_results = []
for i in range(3):
    # Append strings to force list element type inference.
    range_results.append(str(i))
assert range_results == ["0", "1", "2"], "for loop should build string list"

# Test nested loops
assert multiply_table(1) == 1
assert multiply_table(2) == 9
assert multiply_table(3) == 36

print("All control flow tests passed!")
"#,
        Some("All control flow tests passed!"),
    );
}

#[test]
fn runtime_break_continue() {
    run_py(
        "break_continue",
        r#"
# Test break in while loop (explicit break)
def while_break_sum() -> int:
    total: int = 0
    i: int = 0
    while i < 100:
        if i >= 5:
            break
        total = total + i
        i = i + 1
    return total

assert while_break_sum() == 10  # 0+1+2+3+4

# Test continue in while loop
def while_continue_sum() -> int:
    total: int = 0
    i: int = 0
    while i < 10:
        i = i + 1
        if i % 2 == 0:
            continue
        total = total + i
    return total

assert while_continue_sum() == 25  # 1+3+5+7+9

# Test break in for loop
def for_break_find(target: int) -> int:
    idx: int = -1
    for i in range(10):
        if i == target:
            idx = i
            break
    return idx

assert for_break_find(5) == 5
assert for_break_find(20) == -1

# Test continue in for loop
def for_continue_sum() -> int:
    total: int = 0
    for i in range(10):
        if i % 2 == 0:
            continue
        total = total + i
    return total

assert for_continue_sum() == 25  # 1+3+5+7+9

# Test break in nested loops (only inner loop)
def nested_break() -> int:
    total: int = 0
    for i in range(3):
        for j in range(10):
            if j >= 2:
                break
            total = total + 1
    return total

assert nested_break() == 6  # 3 outer iterations * 2 inner iterations

# Test continue in nested loops
def nested_continue() -> int:
    total: int = 0
    for i in range(3):
        for j in range(3):
            if j == 1:
                continue
            total = total + 1
    return total

assert nested_continue() == 6  # 3 * 2 (skipping j=1 each time)

# Test break with accumulation
def break_accumulate() -> list[int]:
    result: list[int] = []
    for i in range(10):
        if i >= 5:
            break
        result.append(i)
    return result

assert break_accumulate() == [0, 1, 2, 3, 4]

# Test continue with accumulation
def continue_accumulate() -> list[int]:
    result: list[int] = []
    for i in range(10):
        if i % 2 == 0:
            continue
        result.append(i)
    return result

assert continue_accumulate() == [1, 3, 5, 7, 9]

# Test break immediately
def break_immediate() -> int:
    count: int = 0
    for i in range(10):
        break
        count = count + 1
    return count

assert break_immediate() == 0

# Test continue on every iteration (empty body after continue)
def continue_all() -> int:
    count: int = 0
    for i in range(5):
        count = count + 1
        continue
    return count

assert continue_all() == 5

print("All break/continue tests passed!")
"#,
        Some("All break/continue tests passed!"),
    );
}
