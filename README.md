# py2rust

A pragmatic transpiler that converts a restricted, statically-typed subset of Python into idiomatic Rust with a minimal runtime.

## Features (MVP)
- Functions with required type annotations
- Function signatures with defaults, keyword arguments, `*args`, keyword-only params, and `**kwargs`
- Basic types: `int`, `float`, `bool`, `str`, `bytes`, `None`
- Collections: `list[T]`, `dict[K, V]`, `tuple[...]`, `set[T]`
- Comparisons: `==`, `!=`, `<`, `<=`, `>`, `>=`, `in`, `not in`, chained comparisons
- Arithmetic and bitwise ops: `+`, `-`, `*`, `/`, `//`, `%`, `**`, `&`, `|`, `^`, `~`, `<<`, `>>` (`int` for bitwise/shifts)
- Augmented assignment: `+=`, `-=`, `*=`, `/=`, `//=`, `%=`, `**=`, `&=`, `|=`, `^=`, `<<=`, `>>=`
- Control flow: `if/elif/else`, `x if cond else y`, `while`, `for`, `return`, `break`, `continue`
- Tuple/list unpacking assignments (including nested and one starred target like `a, *rest, b = ...`)
- For-loop unpacking supports tuple/list targets, including one starred target (e.g. `for a, *rest in items:`)
- Negative indexing and slicing for lists/tuples (including step)
- List methods: `append`, `extend`, `pop`, `insert`, `clear`, `copy`, `reverse`, `sort`, `index`, `count`
- Dict methods: `get`, `pop`, `update`, `clear`, `copy`
- Set methods: `add`, `remove`, `discard`, `clear`, `copy`, `extend`, `pop`
- String operations: indexing/slicing, `upper`, `lower`, `strip`, `lstrip`, `rstrip`, `startswith`, `endswith`, `find`, `replace`, `split`, `join`, `count`, `title`, `capitalize`, `swapcase`, `center`, `ljust`, `rjust`, `zfill`, `isdigit`, `isalpha`, `isalnum`, `isspace`, `isupper`, `islower`
- Classes with fields, methods, and class attributes
- Single inheritance with method overrides and `super().__init__` calls
- Decorators: `@property` (getter/setter), `@staticmethod`, `@classmethod`, and simple top-level function decorators
- Enum-like `Union` aliases via `A | B` type aliases
- `match/case` for literals/singletons, capture and wildcard patterns, `|` patterns, guards, and list sequence/star patterns
- `match/case` on union variants (class patterns) with `__match_args__` support
- Exception handling: `try/except/else/finally`, `raise`, bare re-raise, typed handlers, bare `except:`, and custom exception subclasses rooted in supported built-in exceptions
- Custom iterators via `__iter__` and `next`
- List, set, and dict comprehensions (including multiple generator clauses)
- Generator functions (`yield`) with `next(...)`, `.send(...)`, `.close()`, and generator expressions
- Call-site unpacking via `*args` and `**kwargs` (including mixed call forms)
- Nested functions with closure capture and `nonlocal` writes
- `global` declarations with CPython-compatible shadowing rules (local assignment shadows module names unless explicitly declared `global`)
- Builtins: `abs`, `all`, `any`, `ascii`, `bin`, `bool`, `bytes`, `chr`, `dict`, `divmod`, `enumerate`, `filter`, `float`, `hash`, `hex`, `id`, `int`, `isinstance`, `iter`, `len`, `list`, `map`, `max`, `min`, `oct`, `ord`, `pow`, `range`, `repr`, `reversed`, `round`, `set`, `sorted`, `str`, `sum`, `tuple`, `type`, `zip`
- Stdlib modules (registry-backed):
  - `os`: `remove`, `getcwd`, `chdir`, `mkdir`, `listdir`, `rmdir`, `rename`, `replace`, `makedirs`, `getenv`, and attributes `environ`, `name`, `path`
  - `os.path`: `join`, `exists`, `basename`, `dirname`, `split`, `isdir`, `isfile`, `abspath`
  - `sys`: `exit`, `argv`, `intern`
  - lightweight `re` (via `regex-lite`): `search`, `match`, `sub`, `Match.group(index)`, `Match.span()`
  - lightweight `json`: `dumps`, `loads`, `dump`, `load`
  - `math`: attributes `pi`, `e`, `tau`, `inf`, `nan`; functions `sqrt`, `sin`, `cos`, `tan`, `ceil`, `floor`, `factorial`, `log`, `log2`, `log10`, `exp`, `asin`, `acos`, `atan`, `sinh`, `cosh`, `tanh`, `fabs`, `degrees`, `radians`, `trunc`, `isnan`, `isinf`, `isfinite`, `atan2`, `fmod`, `copysign`, `hypot`, `pow`, `gcd`, `lcm`, `comb`, `perm`
  - `time`: `time`, `time_ns`, `monotonic`, `monotonic_ns`, `perf_counter`, `perf_counter_ns`, `process_time`, `process_time_ns`, `sleep`, `localtime`, `gmtime`, `strftime`, `strptime`
  - lightweight `subprocess`: `run` with `CompletedProcess` fields `args`, `returncode`, `stdout`, `stderr`
- Stdlib imports: `import os`, `import os as o`, `import os.path`, `import sys`, `import re`, `import json`, `import math`, `import time`, `import subprocess`, `from os import remove`, `from os.path import join`, `from sys import exit`, `from sys import intern`, `from re import search`, `from json import dumps`, `from json import loads`, `from math import pi`, `from math import sqrt`, `from time import sleep`, `from time import localtime`, `from time import gmtime`, `from time import strftime`, `from time import strptime`, `from subprocess import run`
- User imports from local files/packages: `import mymod`, `from mymod import f`, and relative package imports (`from .x import y`, `from .. import z`)

## Usage

Build the workspace:

```bash
cargo build
```

Run the transpiler:

```bash
cargo run -p py2rust -- path/to/input.py --output output.rs
```

Optional flags:
- `--emit-hir` prints lowered HIR
- `--emit-types` prints resolved types
- `--pretty` runs `rustfmt` on the generated Rust file
- `--compile` compiles the generated Rust immediately
- `--run` compiles and runs the generated binary

If `--output` is omitted, the output path defaults to `<input>.rs` in the same directory.

Run tests:

```bash
cargo test -p py2rust
```

Runtime integration coverage is registered centrally in `crates/py2rust/tests/runtime.rs`.
Python fixture inputs remain in `crates/py2rust/tests/runtime/*.py`.

Negative/compile-fail coverage lives in `crates/py2rust/tests/negative_tests.rs`.

## Runtime helpers
The generated Rust injects tiny helper functions only when needed:
- `py_print`
- `py_int` (normalizes borrowed/owned `i64` operands for checked arithmetic)
- `py_len`
- `py_range`
- `py_round`
- `py_os_*`, `py_sys_*`, `py_re_*`, `py_json_*`, `py_math_*`, `py_time_*`, `py_subprocess_*`

## Notes and Limitations
- `self` can be unannotated in methods; other parameters require annotations.
- `Optional[T]` can be written as `Optional[T]` or `T | None` and lowers to `Option<T>`.
- Enum-like class unions are supported via `Union[A, B]` / `A | B` aliases.
- Wider inline unions in annotations (for example `int | str`) currently degrade to gradual typing (`Unknown`) rather than a fully static union type.
- `bool` follows Python numeric compatibility in arithmetic/comparison contexts (subtype of `int`) while remaining `bool` in boolean-only flows.
- `from typing import ...` is treated as a no-op; `Union`, `Optional`, and `Iterator` are built-in in annotations.
- `typing.List/Dict/Set/Tuple` annotations are aliases of `list/dict/set/tuple` annotations.
- Imports are unified through one resolver: `typing`/`os`/`sys`/`re`/`json`/`math`/`time`/`subprocess` stay virtual, while local user modules/packages are loaded from files and merged before type checking/codegen.
- For registry-backed stdlib calls, module import is required (`os.remove(...)`, `sys.exit(...)`, `re.search(...)`, `json.dumps(...)`, `math.sqrt(...)`, `time.time(...)`, `subprocess.run(...)` without import is a compile error).
- `from ... import *` is not supported for stdlib modules.
- Supported stdlib surface is registry-driven and intentionally explicit; unsupported module members produce compile-time errors.
- `re` support is intentionally lightweight and backed by `regex-lite` helpers (`search`, `match`, `sub`, `Match.group`, `Match.span`).
- `json` support is intentionally lightweight and currently targets registry-backed calls (`dumps`, `loads`, `dump`, `load`) used by runtime coverage.
- `math` support currently targets the registry-backed surface covered by runtime integration tests (constants + numeric/trig/hyperbolic/combinatorics helpers listed above).
- `time` support currently targets the registry-backed surface covered by runtime integration tests (`time`, `time_ns`, `monotonic`, `monotonic_ns`, `perf_counter`, `perf_counter_ns`, `process_time`, `process_time_ns`, `sleep`, `localtime`, `gmtime`, `strftime`, `strptime`).
- Lightweight `time.localtime` currently shares the same UTC epoch split behavior as `time.gmtime` (timezone and DST databases are not modeled yet).
- `subprocess` support is intentionally lightweight and currently targets `subprocess.run(args, capture_output=False, check=False)` with `CompletedProcess` field access (`args`, `returncode`, `stdout`, `stderr`).
- When generated code uses `re`, py2rust compile/run flows auto-link `regex-lite`; direct manual `rustc` invocation must provide equivalent external crate flags.
- Keyword arguments are supported for user-defined functions/methods/classes with known signatures.
- Builtins are mostly positional-only; keyword arguments are supported for `print(sep=..., end=...)`, `sorted(key=..., reverse=...)`, and iterable-form `min/max(key=...)`.
- `set.extend(iterable)` is supported as an update-style alias that adds all iterable items to the target set.
- `print(x)` in single-argument form uses a direct fast-path (no intermediate `vec![...].join(...)` and no forced `format!` wrapping).
- Generator expressions are lowered through comprehension+`iter(...)` codegen.
- `generator.send(...)` expects a non-`None` value once the generator has started.
- `round(x)` with a float input currently keeps a float result (`round(3.0)` -> `3.0`), while integer inputs stay integer.
- Call-site `**kwargs` unpacking requires a `dict[str, T]` expression.
- Positional-only parameters (`/`) are not supported.
- Nested `def` lowering currently rejects advanced parameter forms (`*args`, keyword-only, `**kwargs`) inside nested local functions.
- `global x` requires `x` to exist at module scope, and declaration order follows CPython rules (`global` must appear before first use in the function).
- `__init__` is treated as a constructor; it must only assign `self` fields.
- Class decorators and decorator calls are rejected (e.g. `@decorator()` or `@dataclass`).
- Function decorators are limited to simple names on top-level functions.
- Class-pattern `match` (e.g. `case Point(x, y):`) currently requires a union-typed subject.
- Guards on class-pattern union matches are currently rejected.
- `dict` indexing raises `KeyError` (propagated as `PyError`).
- Generated `PyError` variants store messages as `Cow<'static, str>` so static messages avoid heap allocation while dynamic messages remain supported.
- `raise X from Y` / `raise X from None` syntax is accepted, but explicit cause/context metadata is not yet preserved in generated Rust.
- Mixed-type tuple iteration falls back to gradual typing (`Unknown`/`PyRepr`) where static unification is not possible.
- Dynamic-length `tuple()` construction is currently represented with list-backed runtime storage; fixed-arity tuple annotations/values still use `tuple[...]` typing.
- Tuple slicing requires literal integer bounds (including negative literals).
- String indexing/slicing is character-based (Unicode scalar values).
- f-strings support literal-only format specs plus conversions `!s`, `!r`, and `!a`.
- `str.format(...)` supports positional and named placeholders, escaped braces, and common width/alignment/precision/type specs used in runtime tests.

## Example

Here's a comprehensive example showcasing the compiler's capabilities:

**Python input:**

```python
class Task:
    """Represents a task with priority and status"""
    total_tasks: int = 0

    def __init__(self, title: str, priority: int) -> None:
        """Initialize a new task"""
        self.title: str = title
        self.priority: int = priority
        self.completed: bool = False
        Task.total_tasks = Task.total_tasks + 1

    @property
    def status(self) -> str:
        """Get task status"""
        return "Done" if self.completed else "Pending"

    def complete(self) -> None:
        """Mark task as completed"""
        self.completed = True

    @staticmethod
    def validate_priority(p: int) -> bool:
        """Check if priority value is valid"""
        return p >= 1 and p <= 5

# Union types for polymorphism
class Success:
    __match_args__ = ('value',)
    value: int
    def __init__(self, v: int) -> None:
        self.value = v

class Failure:
    __match_args__ = ('error',)
    error: str
    def __init__(self, e: str) -> None:
        self.error = e

TaskResult = Success | Failure

def process_result(r: TaskResult) -> str:
    """Process result using pattern matching"""
    match r:
        case Success(v):
            return f"Success: {v}"
        case Failure(e):
            return f"Error: {e}"

# Main program
priorities: list[int] = [p for p in range(1, 6) if p % 2 == 1]
print(f"Odd priorities: {priorities}")

task: Task = Task("Write docs", 3)
print(f"Task '{task.title}' has priority {task.priority}")

if Task.validate_priority(3):
    print(f"Total tasks created: {Task.total_tasks}")

results: list[TaskResult] = [Success(42), Failure("timeout")]
for r in results:
    print(process_result(r))

numbers: list[int] = [1, 5, 3, 9, 2]
print(f"Max: {max(numbers)}, Min: {min(numbers)}")
print(f"Reversed: {list(reversed(numbers))}")
```

**Generated Rust output** (simplified):

```rust
#[derive(Debug, Clone)]
pub struct Task {
    pub title: String,
    pub priority: i64,
    pub completed: bool,
}

impl Task {
    pub fn new(title: String, priority: i64) -> Result<Task, PyError> {
        if (priority < 1i64) || (priority > 5i64) {
            return Err(PyError::TaskError(TaskError::new("Priority must be between 1 and 5".to_string())));
        }
        // ... initialization
        Ok(Task { title, priority, completed: false })
    }

    pub fn status(&self) -> String {
        if self.completed { "Done".to_string() } else { "Pending".to_string() }
    }

    pub fn complete(&mut self) {
        self.completed = true;
    }

    pub fn validate_priority(p: i64) -> bool {
        (p >= 1i64) && (p <= 5i64)
    }
}

#[derive(Debug, Clone)]
pub enum TaskResult {
    Success(Success),
    Failure(Failure),
}

pub fn process_result(r: &TaskResult) -> String {
    match r {
        TaskResult::Success(ref _x) => format!("Success: {}", _x.value.to_string()),
        TaskResult::Failure(ref _x) => format!("Error: {}", _x.error),
    }
}

// ... more generated code
```

**Compile and run:**

```bash
# Transpile only
./target/release/py2rust input.py --output output.rs --pretty

# Transpile and compile
./target/release/py2rust input.py --compile

# Transpile, compile, and run
./target/release/py2rust input.py --run
```
