# Basic try/except
def test_basic_catch() -> None:
    caught: bool = False
    try:
        raise ValueError("test error")
    except ValueError:
        caught = True
    assert caught, "Should catch ValueError"

test_basic_catch()

# Try/except/else (variable declared in try used in else)
def test_try_else() -> None:
    else_executed: bool = False
    try:
        val: int = 42
    except ValueError:
        assert False, "Should not catch"
    else:
        else_executed = True
        assert val == 42, "Variable from try should be accessible"
    assert else_executed, "Else block should execute"

test_try_else()

# Try/finally
def test_finally() -> None:
    finally_executed: bool = False
    try:
        dummy: int = 1
    finally:
        finally_executed = True
    assert finally_executed, "Finally should execute"

test_finally()

# Try/except/finally
def test_except_finally() -> None:
    caught: bool = False
    finally_executed: bool = False
    try:
        raise RuntimeError("test error")
    except RuntimeError:
        caught = True
    finally:
        finally_executed = True
    assert caught, "Should catch RuntimeError"
    assert finally_executed, "Finally should execute"

test_except_finally()

# Multiple except handlers
def test_multiple_except() -> None:
    caught_type: int = 0
    condition: int = 1
    try:
        if condition == 0:
            raise ValueError("zero")
        elif condition == 1:
            raise TypeError("one")
        else:
            raise RuntimeError("other")
    except ValueError:
        caught_type = 1
    except TypeError:
        caught_type = 2
    except RuntimeError:
        caught_type = 3
    assert caught_type == 2, "Should catch TypeError"

test_multiple_except()

# Exception propagation through function calls
def throws_error() -> int:
    raise RuntimeError("error from function")

def catches_error() -> int:
    try:
        return throws_error()
    except RuntimeError:
        return -1

def test_catches_error() -> None:
    result: int = catches_error()
    assert result == -1, "Should return -1 after catching"

test_catches_error()

# Conditional exceptions
def conditional_raise(val: int) -> int:
    if val < 0:
        raise ValueError("negative value")
    return val * 2

def test_conditional_raise() -> None:
    # Should succeed
    val1: int = conditional_raise(5)
    assert val1 == 10, "Should return doubled value"

    # Should raise
    caught: bool = False
    try:
        val2: int = conditional_raise(-3)
    except ValueError:
        caught = True
    assert caught, "Should catch ValueError for negative"

test_conditional_raise()

# Nested try/except
def test_nested() -> None:
    inner_caught: bool = False
    outer_finally: bool = False
    after_inner: bool = False
    try:
        try:
            raise ValueError("inner error")
        except ValueError:
            inner_caught = True
        after_inner = True
    except RuntimeError:
        assert False, "Should not catch RuntimeError"
    finally:
        outer_finally = True
    assert inner_caught, "Inner except should catch"
    assert after_inner, "Should continue after inner try"
    assert outer_finally, "Outer finally should execute"

test_nested()

# Bare raise re-throws the current exception
def inner_handler() -> None:
    try:
        raise ValueError("original error")
    except ValueError as err:
        raise

def outer_handler() -> bool:
    try:
        inner_handler()
        return False
    except ValueError:
        return True

def test_bare_raise() -> None:
    result: bool = outer_handler()
    assert result, "Should catch re-raised ValueError"

test_bare_raise()

# Conditional re-raise
def conditional_reraise(do_reraise: bool) -> None:
    try:
        raise RuntimeError("test")
    except RuntimeError as err:
        if do_reraise:
            raise

def test_conditional_reraise() -> None:
    # Should not re-raise
    caught1: bool = False
    try:
        conditional_reraise(False)
    except RuntimeError:
        caught1 = True
    assert not caught1, "Should not catch when not re-raising"

    # Should re-raise
    caught2: bool = False
    try:
        conditional_reraise(True)
    except RuntimeError:
        caught2 = True
    assert caught2, "Should catch when re-raising"

test_conditional_reraise()

# except Exception catches any exception type
def catch_with_exception(exc_type: int) -> bool:
    try:
        if exc_type == 1:
            raise ValueError("value error")
        elif exc_type == 2:
            raise TypeError("type error")
        else:
            raise RuntimeError("runtime error")
    except Exception:
        return True
    return False

def test_exception_catch_all() -> None:
    assert catch_with_exception(1), "Should catch ValueError with Exception"
    assert catch_with_exception(2), "Should catch TypeError with Exception"
    assert catch_with_exception(3), "Should catch RuntimeError with Exception"

test_exception_catch_all()

# Exception can also re-raise from catch-all
def catch_and_reraise() -> None:
    try:
        raise ValueError("will be re-raised")
    except Exception as err:
        raise

def test_catch_reraise() -> None:
    caught: bool = False
    try:
        catch_and_reraise()
    except ValueError:
        caught = True
    assert caught, "Should catch re-raised ValueError"

test_catch_reraise()

# Test new exception types
def test_stop_iteration() -> None:
    caught: bool = False
    try:
        raise StopIteration("iterator exhausted")
    except StopIteration:
        caught = True
    assert caught, "Should catch StopIteration"

def test_not_implemented() -> None:
    caught: bool = False
    try:
        raise NotImplementedError("abstract method")
    except NotImplementedError:
        caught = True
    assert caught, "Should catch NotImplementedError"

def test_io_error() -> None:
    caught: bool = False
    try:
        raise IOError("file not found")
    except IOError:
        caught = True
    assert caught, "Should catch IOError"

def test_overflow_error() -> None:
    caught: bool = False
    try:
        raise OverflowError("number too large")
    except OverflowError:
        caught = True
    assert caught, "Should catch OverflowError"

test_stop_iteration()
test_not_implemented()
test_io_error()
test_overflow_error()

# Builtin error behavior
def test_builtin_errors() -> None:
    caught: bool = False
    try:
        chr(-1)
    except ValueError:
        caught = True
    assert caught, "chr() should raise ValueError for invalid codepoint"

    caught = False
    try:
        ord("")
    except TypeError:
        caught = True
    assert caught, "ord() should raise TypeError for empty string"

    caught = False
    try:
        next(range(0))
    except StopIteration:
        caught = True
    assert caught, "next() should raise StopIteration on empty iterator"

    caught = False
    try:
        d: dict[str, int] = {"a": 1}
        d["missing"]
    except KeyError:
        caught = True
    assert caught, "Dict missing key should raise KeyError"

    caught = False
    try:
        nums: list[int] = [1, 2]
        nums[5]
    except IndexError:
        caught = True
    assert caught, "List out-of-range index should raise IndexError"

    caught = False
    try:
        max([])
    except ValueError:
        caught = True
    assert caught, "max([]) should raise ValueError"

    caught = False
    try:
        min([])
    except ValueError:
        caught = True
    assert caught, "min([]) should raise ValueError"

    caught = False
    try:
        range(0, 5, 0)
    except ValueError:
        caught = True
    assert caught, "range() step 0 should raise ValueError"

    caught = False
    try:
        step: int = 0
        nums2: list[int] = [1, 2, 3]
        nums2[::step]
    except ValueError:
        caught = True
    assert caught, "slice step 0 should raise ValueError"

    caught = False
    try:
        int("not-a-number")
    except ValueError:
        caught = True
    assert caught, "int() invalid string should raise ValueError"

    caught = False
    try:
        float("not-a-number")
    except ValueError:
        caught = True
    assert caught, "float() invalid string should raise ValueError"

test_builtin_errors()

# Top-level exception handling (exception at script root)
top_level_caught1: bool = False
try:
    raise KeyError("top-level key error")
except KeyError:
    top_level_caught1 = True
assert top_level_caught1, "Should catch top-level KeyError"

top_level_caught2: bool = False
try:
    raise IndexError("top-level index error")
except Exception:
    top_level_caught2 = True
assert top_level_caught2, "Should catch top-level with Exception"

# Top-level with finally
top_level_finally: bool = False
try:
    top_dummy: int = 1
finally:
    top_level_finally = True
assert top_level_finally, "Top-level finally should execute"

# Exception type narrowing - ValueError should not be caught by TypeError handler
def test_exception_narrowing() -> None:
    caught_value: bool = False
    caught_type: bool = False

    # ValueError should only be caught by ValueError handler
    try:
        raise ValueError("value error")
    except TypeError:
        caught_type = True
    except ValueError:
        caught_value = True

    assert caught_value, "ValueError should be caught by ValueError handler"
    assert not caught_type, "ValueError should NOT be caught by TypeError handler"

    # Reset and test that TypeError is caught correctly
    caught_value = False
    caught_type = False
    try:
        raise TypeError("type error")
    except ValueError:
        caught_value = True
    except TypeError:
        caught_type = True

    assert caught_type, "TypeError should be caught by TypeError handler"
    assert not caught_value, "TypeError should NOT be caught by ValueError handler"

test_exception_narrowing()

# Try/else with multiple variables declared in try block
def test_try_else_multiple_vars() -> None:
    else_executed: bool = False
    try:
        x: int = 10
        y: int = 20
        z: int = 30
    except ValueError:
        assert False, "Should not catch"
    else:
        # All variables from try block should be accessible in else
        else_executed = True
        assert x == 10, "x should be 10"
        assert y == 20, "y should be 20"
        assert z == 30, "z should be 30"
        total: int = x + y + z
        assert total == 60, "total should be 60"

    assert else_executed, "Else block should execute"

test_try_else_multiple_vars()

# Try/else with computation in try block
def test_try_else_computation() -> None:
    result: int = 0
    try:
        a: int = 5
        b: int = a * 2
        c: int = b + 3
    except RuntimeError:
        result = -1
    else:
        result = c

    assert result == 13, "Result should be 13 (5*2+3)"

test_try_else_computation()

# Try with partial value-return must not panic on fallthrough path
def test_try_partial_value_return() -> None:
    def partial_return(flag: bool) -> int:
        try:
            if flag:
                return 11
            marker: int = 5
        except Exception:
            return -1
        return 6

    assert partial_return(True) == 11
    assert partial_return(False) == 6

test_try_partial_value_return()

# Exception propagation with specific types
def raises_value_error() -> int:
    raise ValueError("from function")

def raises_type_error() -> int:
    raise TypeError("from function")

def test_propagation_narrowing() -> None:
    # Only ValueError should be caught
    caught: bool = False
    try:
        raises_value_error()
    except ValueError:
        caught = True
    except TypeError:
        assert False, "Should not catch TypeError"
    assert caught, "Should catch propagated ValueError"

    # Only TypeError should be caught
    caught = False
    try:
        raises_type_error()
    except ValueError:
        assert False, "Should not catch ValueError"
    except TypeError:
        caught = True
    assert caught, "Should catch propagated TypeError"

test_propagation_narrowing()

print("All exception tests passed!")
