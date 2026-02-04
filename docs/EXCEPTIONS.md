# Exception Handling Implementation

## Overview

This document describes the exception handling implementation for py2rust, which transpiles Python exception handling to idiomatic Rust Result types.

## Implementation Summary

### Completed Features

✅ **Core Infrastructure**
- Added `Try` and `Raise` HIR nodes for exception handling
- Extended type system with `Result<T, E>` and `Exception` types
- Extended `FunctionSig` with `can_throw` tracking

✅ **Control Flow Analysis**
- Implemented `ThrowAnalyzer` that performs control flow analysis
- Automatically determines which functions can throw exceptions
- Propagates throw information through the call graph
- Functions with try/except that catch all don't propagate throws

✅ **Python AST Lowering**
- Converts Python `try/except/else/finally` to HIR
- Converts Python `raise` statements to HIR
- Handles exception handler binding (`except ValueError as e:`)

✅ **Type Checking**
- Validates exception types (built-in and custom)
- Type checks try/except/else/finally blocks
- Validates raise statements
- Prevents re-raise outside except handlers

✅ **Code Generation**
- **PyError Enum**: Automatically emitted with built-in exception types
  - Implements `Display` and `Error` traits
  - Only included when exceptions are used (zero overhead otherwise)

- **Function Signatures**: Throwing functions return `Result<T, PyError>`
  - Non-throwing functions keep normal signatures
  - Automatic based on control flow analysis

- **Try/Except**: Generates match expressions
  - Try body wrapped in closure returning Result
  - Except handlers map to match arms
  - Catch-all pattern for unhandled exceptions

- **Finally**: Uses Drop trait for guaranteed cleanup
  - Creates local `Finally` struct with Drop impl
  - Cleanup code runs even if exception occurs

- **Error Propagation**: Automatic `?` operator insertion
  - Calls to throwing functions use `?`
  - Return statements wrapped in `Ok()` when needed

- **Top-Level Handling**: Main function wraps throwing code
  - Catches and prints uncaught exceptions
  - Exits with error code

## Supported Python Features

### Basic Exception Handling
```python
def example():
    try:
        raise ValueError("error message")
    except ValueError as e:
        print("Caught:", e)
```

### Multiple Exception Handlers
```python
try:
    risky_operation()
except ValueError as e:
    handle_value_error(e)
except TypeError as e:
    handle_type_error(e)
```

### Try/Except/Finally
```python
try:
    operation()
except Exception as e:
    handle_error(e)
finally:
    cleanup()  # Always runs
```

### Exception Propagation
```python
def inner() -> int:
    raise RuntimeError("error")

def outer() -> int:
    return inner()  # Automatically gets Result return type

def caller():
    try:
        x = outer()
    except RuntimeError as e:
        print("Caught:", e)
```

### Built-in Exceptions
- `ValueError`
- `TypeError`
- `RuntimeError`
- `KeyError`
- `IndexError`
- `AttributeError`
- `ZeroDivisionError`
- `NameError`
- `AssertionError`

## Known Limitations

### Variable Scoping
Variables defined in try blocks may not be accessible in else blocks due to Rust closure scoping:

```python
# This pattern has scoping issues in generated Rust:
try:
    y = compute()
except:
    y = default
else:
    print(y)  # y is out of scope in generated code
```

**Workaround**: Define variables before the try block or use them within the try/except handlers.

### Exception Type Analysis
The throw analyzer determines if functions throw based on:
- Explicit `raise` statements
- Calls to functions that throw
- Whether exceptions are caught

Some edge cases:
- Try/except blocks that don't catch all exception types will propagate
- Partial exception handling is conservative (assumes propagation)

### Not Yet Implemented
- Custom exception classes (user-defined)
- Exception chaining (`raise X from Y`)
- Bare `except:` clauses (catch-all without type)
- Re-raise within nested except handlers

## Generated Code Examples

### Simple Raise
```python
def example():
    raise ValueError("error")
```

Generates:
```rust
pub fn example() -> Result<(), PyError> {
    return Err(PyError::ValueError("error".to_string()));
}
```

### Try/Except
```python
def example():
    try:
        risky()
    except ValueError as e:
        print("Error:", e)
```

Generates:
```rust
pub fn example() -> Result<(), PyError> {
    let _try_result = (|| -> Result<(), PyError> {
        (risky()?);
        Ok(())
    })();
    match _try_result {
        Ok(_) => {}
        Err(PyError::ValueError(e)) => {
            py_print(format!("Error: {}", e));
        }
        Err(e) => return Err(e),
    }
    Ok(())
}
```

### Try/Finally
```python
def example():
    try:
        operation()
    finally:
        cleanup()
```

Generates:
```rust
pub fn example() -> () {
    {
        struct Finally<F: FnOnce()>(Option<F>);
        impl<F: FnOnce()> Drop for Finally<F> {
            fn drop(&mut self) {
                if let Some(f) = self.0.take() { f(); }
            }
        }
        let _finally = Finally(Some(|| {
            cleanup();
        }));
        let _try_result = (|| -> Result<(), PyError> {
            operation();
            Ok(())
        })();
        _try_result.unwrap();
    }
}
```

## Performance Characteristics

- **Zero Overhead**: Functions without exceptions have no runtime cost
- **No Runtime Crate**: All exception handling code is inlined
- **Compile-Time Analysis**: Throw determination happens at compile time
- **Idiomatic Rust**: Uses standard `Result<T, E>` pattern with `?` operator

## Future Enhancements

Potential improvements:
1. Support for custom exception classes
2. Better variable scoping in try/except/else
3. Exception chaining support
4. More precise throw analysis for partial exception handling
5. Optimization of match patterns for common cases
