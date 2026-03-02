import random

# Test 1: seed + choices — variable must be updated each iteration
# (regression: old bug created a shadowing `let` that left x == 0)
random.seed(1)
x = 0
for _ in range(5):
    x = random.choices([10, 20, 30])[0]
assert x != 0  # x is in [10, 20, 30], never 0

# Test 2: choices with k parameter returns k elements
random.seed(1)
results = random.choices([1, 2, 3], k=3)
assert len(results) == 3

# Test 3: choices with weights — heavy weight on element 3
random.seed(1)
results = random.choices([1, 2, 3], weights=[1.0, 1.0, 8.0], k=10)
count_3 = 0
for r in results:
    if r == 3:
        count_3 += 1
assert count_3 > 5

# Test 4: shuffle mutates the list in place; sum of elements is invariant
random.seed(1)
nums = [1, 2, 3, 4, 5]
random.shuffle(nums)
total = 0
for n in nums:
    total += n
assert total == 15

# Test 5: gauss returns a finite float (NaN != NaN, so val == val means not NaN)
random.seed(1)
val = random.gauss(0.0, 1.0)
assert val == val

print("All random tests passed!")
