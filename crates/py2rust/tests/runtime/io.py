# Print different types
print(42)
print("hello")
print(True)
print(False)

# Print with multiple arguments is supported in a dedicated test
# Keep this focused on basic single-argument prints

# Print string operations
s: str = "world"
print(s)

# Print arithmetic results
x: int = 10 + 20
print(x)

# Print from function
def get_message() -> str:
    return "message from function"

print(get_message())

# Print in loop - using while to avoid iterator issues
def print_loop() -> None:
    nums: list[int] = [1, 2, 3]
    i: int = 0
    while i < len(nums):
        print(nums[i])
        i = i + 1

print_loop()
