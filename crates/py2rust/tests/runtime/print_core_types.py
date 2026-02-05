# Print single integer
print(42)

# Print single float
print(3.14)

# Print single bool
print(True)
print(False)

# Print None
print(None)

# Print string
print("Hello, World!")

# Print multiple values of same type
print(1, 2, 3)

# Print multiple values of different types
print(42, 3.14, True, "hello")

# Print expression results
x_print: int = 10
y_print: int = 20
print(x_print + y_print)

# Print in a loop
for i in range(3):
    print(i)

# Nested print (result of expression with function call)
def add_for_print(a: int, b: int) -> int:
    return a + b

print(add_for_print(5, 7))
