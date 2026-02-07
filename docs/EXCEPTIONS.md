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
- Supports `except Exception` as a catch-all
- Supports bare `except:` as a catch-all
- Parses `raise X from Y` / `raise X from None` syntax

✅ **Type Checking**
- Validates exception types (built-in and user-defined exception subclasses)
- Type checks try/except/else/finally blocks
- Validates raise statements
- Prevents re-raise outside except handlers

✅ **Code Generation**
- **PyError Enum**: Automatically emitted with built-in exception types
  - Message payload type is `Cow<'static, str>` (static strings avoid heap allocation)
  - Implements `Display` and `Error` traits
  - Only included when exceptions are used (zero overhead otherwise)

- **Function Signatures**: Throwing functions return `Result<T, PyError>`
  - Non-throwing functions keep normal signatures
  - Automatic based on control flow analysis

- **Try/Except**: Generates match expressions
  - Try body wrapped in closure returning Result
  - Except handlers map to match arms
  - Duplicate typed handlers are dropped after the first effective match arm
  - Catch-all pattern for unhandled exceptions

- **Finally**: Uses Drop trait for guaranteed cleanup
  - Creates local `Finally` struct with Drop impl
  - Cleanup code runs even if exception occurs

- **Error Propagation**: Automatic `?` operator insertion
  - Calls to throwing functions use `?`
  - Return statements wrapped in `Ok()` when needed
  - Dead `Ok(())` closure epilogues are omitted when the try body is provably terminal

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

### Re-raise
```python
def example():
    try:
        risky()
    except Exception:
        raise
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
- `StopIteration`
- `NotImplementedError`
- `IOError`
- `OverflowError`
- `GeneratorExit`
- `MemoryError`
- `Exception` (catch-all)

### Custom Exceptions
- User-defined exception classes are supported when they inherit (single inheritance chain) from a supported built-in exception root.
- Custom exceptions map to their built-in root variant in generated `PyError`.
- Exception inheritance chains are resolved for `raise`, `except`, and catch compatibility.

## Known Limitations

### Variable Scoping
Variables declared in a `try` block but used in `else` are wrapped with `Option<T>` so they can cross the Rust closure boundary. This makes the variable available in the `else` block, but it will be `None` unless the `try` block assigns it on all paths.

**Workaround**: Initialize the variable before the `try` block if you need a non-optional value in `else`.

### Exception Type Analysis
The throw analyzer determines if functions throw based on:
- Explicit `raise` statements
- Calls to functions that throw
- Whether exceptions are caught

Some edge cases:
- Try/except blocks that don't catch all exception types will propagate
- Partial exception handling is conservative (assumes propagation)

### Current Limitations
- `raise X from Y` and `raise X from None` are accepted, but explicit cause/context metadata is currently not preserved in generated Rust.
- Custom exception hierarchies must be single-inheritance chains rooted in a supported built-in exception.

## Generated Code Examples

### Simple Raise
```python
def example():
    raise ValueError("error")
```

Generates:
```rust
pub fn example() -> Result<(), PyError> {
    return Err(PyError::ValueError(("error".to_string()).into()));
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
            py_print(&e);
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
1. Preserve explicit cause/context metadata for `raise ... from ...`
2. Better variable scoping in try/except/else
3. More precise throw analysis for partial exception handling
4. Optimization of match patterns for common cases
