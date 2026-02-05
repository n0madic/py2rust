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

# While loop with break and continue
sum_while: int = 0
count_while: int = 0
while count_while < 5:
    count_while = count_while + 1
    if count_while == 2:
        continue
    if count_while == 4:
        break
    sum_while = sum_while + count_while
assert sum_while == 4, "while with break/continue failed"

# For loop with range(start, stop)
range_sum: int = 0
for k in range(5, 15):
    range_sum = range_sum + k
assert range_sum == 95, "sum of 5 to 14 should be 95"

# Range with positive steps
sum_step1: int = 0
for step_i in range(0, 10, 2):
    sum_step1 = sum_step1 + step_i
assert sum_step1 == 20, "range(0, 10, 2) should give sum 20"

sum_step2: int = 0
for step_j in range(1, 15, 3):
    sum_step2 = sum_step2 + step_j
assert sum_step2 == 35, "range(1, 15, 3) should give sum 35"

# Range with negative steps
sum_step3: int = 0
for step_k in range(10, 0, -1):
    sum_step3 = sum_step3 + step_k
assert sum_step3 == 55, "range(10, 0, -1) should give sum 55"

sum_step4: int = 0
for step_m in range(10, 0, -2):
    sum_step4 = sum_step4 + step_m
assert sum_step4 == 30, "range(10, 0, -2) should give sum 30"

sum_step5: int = 0
for step_n in range(15, 0, -3):
    sum_step5 = sum_step5 + step_n
assert sum_step5 == 45, "range(15, 0, -3) should give sum 45"

# Empty range tests
count_empty: int = 0
for ep in range(5, 5, -1):
    count_empty = count_empty + 1
assert count_empty == 0, "range(5, 5, -1) should be empty"

count_empty2: int = 0
for eq in range(1, 10, -1):
    count_empty2 = count_empty2 + 1
assert count_empty2 == 0, "range(1, 10, -1) should be empty"

count_empty3: int = 0
for er in range(10, 5, 1):
    count_empty3 = count_empty3 + 1
assert count_empty3 == 0, "range(10, 5, 1) should be empty"

# Single iteration
count_single: int = 0
for es in range(5, 6, 1):
    count_single = count_single + 1
assert count_single == 1, "range(5, 6, 1) should iterate once"

# Negative range bounds with negative step
sum_neg: int = 0
for et in range(-1, -6, -1):
    sum_neg = sum_neg + et
assert sum_neg == -15, "range(-1, -6, -1) should give sum -15"

# Positive to negative with negative step
sum_pos_neg: int = 0
for eu in range(3, -3, -1):
    sum_pos_neg = sum_pos_neg + eu
assert sum_pos_neg == 3, "range(3, -3, -1) should give sum 3"

# Large step that exceeds range
count_large: int = 0
for ev in range(0, 5, 10):
    count_large = count_large + 1
assert count_large == 1, "range(0, 5, 10) should iterate once (only 0)"

# Large negative step that exceeds range
count_large_neg: int = 0
for ew in range(5, 0, -10):
    count_large_neg = count_large_neg + 1
assert count_large_neg == 1, "range(5, 0, -10) should iterate once (only 5)"

# For loops with iterables (list, tuple, string, dict)
nums: list[int] = [10, 20, 30, 40, 50]
list_total: int = 0
for num in nums:
    list_total = list_total + num
assert list_total == 150, "sum of list elements should be 150"

empty_list: list[int] = []
empty_count: int = 0
for x in empty_list:
    empty_count = empty_count + 1
assert empty_count == 0, "empty list should have 0 iterations"

single: list[int] = [42]
single_total: int = 0
for s in single:
    single_total = single_total + s
assert single_total == 42, "single element list sum should be 42"

coords: tuple[int, int, int] = (100, 200, 300)
coord_sum: int = 0
for c in coords:
    coord_sum = coord_sum + c
assert coord_sum == 600, "sum of tuple elements should be 600"

single_tuple: tuple[int] = (99,)
single_t: int = 0
for st in single_tuple:
    single_t = single_t + st
assert single_t == 99, "single tuple element sum should be 99"

text: str = "HELLO"
char_count: int = 0
for ch in text:
    char_count = char_count + 1
assert char_count == 5, "string 'HELLO' should have 5 characters"

empty_str: str = ""
empty_char_count: int = 0
for ec in empty_str:
    empty_char_count = empty_char_count + 1
assert empty_char_count == 0, "empty string should have 0 iterations"

single_char: str = "X"
single_char_count: int = 0
for sc in single_char:
    single_char_count = single_char_count + 1
assert single_char_count == 1, "single char string should have 1 iteration"

ages: dict[str, int] = {"alice": 30, "bob": 25, "carol": 35}
key_count: int = 0
for name in ages:
    key_count = key_count + 1
assert key_count == 3, "dict should have 3 keys"

empty_dict: dict[str, int] = {}
empty_dict_count: int = 0
for ed in empty_dict:
    empty_dict_count = empty_dict_count + 1
assert empty_dict_count == 0, "empty dict should have 0 iterations"

# Nested loops (additional cases)
inner_sum: int = 0
for a in range(3):
    for b in range(4):
        inner_sum = inner_sum + 1
assert inner_sum == 12, "3 * 4 iterations should be 12"

outer_list: list[int] = [1, 2]
inner_list: list[int] = [10, 20, 30]
nested_sum: int = 0
for ol in outer_list:
    for il in inner_list:
        nested_sum = nested_sum + ol + il
assert nested_sum == 129, "nested list sum should be 129"

range_list_sum: int = 0
items: list[int] = [100, 200]
for ri in range(3):
    for item in items:
        range_list_sum = range_list_sum + ri + item
assert range_list_sum == 906, "mixed range+list sum should be 906"

nested_total: int = 0
for na in range(0, 6, 2):
    for nb in range(3, 0, -1):
        nested_total = nested_total + 1
assert nested_total == 9, "nested range loops should give 9 iterations"

triple: int = 0
l1: list[int] = [1, 2]
l2: list[int] = [1, 2]
l3: list[int] = [1, 2]
for x1 in l1:
    for y1 in l2:
        for z1 in l3:
            triple = triple + 1
assert triple == 8, "triple nested should be 8 iterations"

char_pairs: int = 0
str1: str = "AB"
str2: str = "12"
for c1 in str1:
    for c2 in str2:
        char_pairs = char_pairs + 1
assert char_pairs == 4, "string nested should be 4 pairs"

nested_count: int = 0
words: list[str] = ["hi", "bye"]
for word in words:
    for letter in word:
        nested_count = nested_count + 1
assert nested_count == 5, "hi(2) + bye(3) = 5 characters"

result: str = ""
letters: list[str] = ["a", "b", "c"]
for letter in letters:
    result = result + letter
assert result == "abc", "concatenated letters should be 'abc'"

# Ternary operator
num: int = 10
ternary_result: str = "between" if num > 5 and num < 15 else "not between"
assert ternary_result == "between", "ternary operator failed"

ternary_result2: str = "big" if num > 20 else "small"
assert ternary_result2 == "small", "ternary operator 2 failed"

# Nested if-else
nested_result: str = ""
if num > 5:
    if num < 15:
        nested_result = "between 5 and 15"
    else:
        nested_result = "greater than 15"
else:
    nested_result = "less than or equal to 5"
assert nested_result == "between 5 and 15", "nested if-else failed"

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

# Additional break/continue scenarios from control flow script
found: int = 0
for m in range(20):
    if m > 5:
        found = m
        break
assert found == 6, "first number > 5 should be 6"

even_sum: int = 0
for n in range(10):
    if n % 2 == 1:
        continue
    even_sum = even_sum + n
assert even_sum == 20, "sum of even numbers 0,2,4,6,8 should be 20"

first_big: int = 0
values: list[int] = [1, 2, 10, 20, 30]
for v in values:
    if v >= 10:
        first_big = v
        break
assert first_big == 10, "first element >= 10 should be 10"

even_only: int = 0
mixed: list[int] = [1, 2, 3, 4, 5, 6]
for mx in mixed:
    if mx % 2 == 1:
        continue
    even_only = even_only + mx
assert even_only == 12, "sum of even elements (2+4+6) should be 12"

break_count: int = 0
outer: list[int] = [1, 2, 3]
inner: list[int] = [1, 2, 3, 4, 5]
for o in outer:
    for ii in inner:
        if ii > 2:
            break
        break_count = break_count + 1
assert break_count == 6, "break in inner should give 6 iterations"

print("All break/continue tests passed!")
"#,
        Some("All break/continue tests passed!"),
    );
}
