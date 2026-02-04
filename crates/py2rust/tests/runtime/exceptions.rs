//! Runtime tests for exception handling: try/except/finally/raise.

use crate::common::run_py;

#[test]
fn runtime_exceptions_comprehensive() {
    run_py(
        "exceptions",
        r#"
# Basic try/except
def test_basic_catch() -> None:
    try:
        raise ValueError("test error")
    except ValueError as e:
        print("Caught ValueError")

test_basic_catch()

# Try/except/else (variable declared in try used in else)
def test_try_else() -> None:
    try:
        val: int = 42
    except ValueError as e:
        print("Error")
    else:
        print("No exception, val:", val)

test_try_else()

# Try/finally
def test_finally() -> None:
    try:
        print("In try block")
    finally:
        print("In finally block")

test_finally()

# Try/except/finally
def test_except_finally() -> None:
    try:
        raise RuntimeError("test error")
    except RuntimeError as e:
        print("Caught RuntimeError")
    finally:
        print("Cleanup in finally")

test_except_finally()

# Multiple except handlers
def test_multiple_except() -> None:
    condition: int = 1
    try:
        if condition == 0:
            raise ValueError("zero")
        elif condition == 1:
            raise TypeError("one")
        else:
            raise RuntimeError("other")
    except ValueError as e:
        print("Caught ValueError")
    except TypeError as e:
        print("Caught TypeError")
    except RuntimeError as e:
        print("Caught RuntimeError")

test_multiple_except()

# Exception propagation through function calls
def throws_error() -> int:
    raise RuntimeError("error from function")

def catches_error() -> int:
    try:
        return throws_error()
    except RuntimeError as e:
        print("Caught propagated error")
        return -1

def test_catches_error() -> None:
    caught_result: int = catches_error()
    assert caught_result == -1

test_catches_error()

# Conditional exceptions
def conditional_raise(val: int) -> int:
    if val < 0:
        raise ValueError("negative value")
    return val * 2

def test_conditional() -> None:
    # Should succeed
    val1: int = conditional_raise(5)
    print("Positive value result:", val1)

    # Should raise
    try:
        val2: int = conditional_raise(-3)
        print("Should not reach here:", val2)
    except ValueError as e:
        print("Caught negative value error")

test_conditional()

# Nested try/except
def test_nested() -> None:
    try:
        print("Outer try")
        try:
            print("Inner try")
            raise ValueError("inner error")
        except ValueError as e:
            print("Caught in inner except")
        print("After inner try")
    except RuntimeError as e:
        print("Caught in outer except")
    finally:
        print("Outer finally")

test_nested()

print("All exception tests passed!")
"#,
        Some("Caught ValueError\nNo exception, val: 42\nIn try block\nIn finally block\nCaught RuntimeError\nCleanup in finally\nCaught TypeError\nCaught propagated error\nPositive value result: 10\nCaught negative value error\nOuter try\nInner try\nCaught in inner except\nAfter inner try\nOuter finally\nAll exception tests passed!"),
    );
}
