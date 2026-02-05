# py2rust

A pragmatic transpiler that converts a restricted, statically-typed subset of Python into idiomatic Rust with a minimal runtime.

## Project Layout
- `crates/py2rust` — compiler/transpiler (Rust)

## Features (MVP)
- Functions with required type annotations
- Basic types: `int`, `float`, `bool`, `str`, `bytes`, `None`
- Collections: `list[T]`, `dict[K, V]`, `tuple[...]`, `set[T]`
- Comparisons: `==`, `!=`, `<`, `<=`, `>`, `>=`, `in`, `not in`, chained comparisons
- Arithmetic and bitwise ops: `+`, `-`, `*`, `/`, `//`, `%`, `**`, `&`, `|`, `^`, `~`, `<<`, `>>` (`int` for bitwise/shifts)
- Augmented assignment: `+=`, `-=`, `*=`, `/=`, `//=`, `%=`, `**=`, `&=`, `|=`, `^=`, `<<=`, `>>=`
- Control flow: `if/elif/else`, `x if cond else y`, `while`, `for`, `return`, `break`, `continue`
- Tuple/list unpacking assignments (including nested)
- Negative indexing and slicing for lists/tuples (including step)
- List methods: `append`, `extend`, `pop`, `insert`, `clear`, `copy`, `reverse`, `sort`, `index`, `count`
- Dict methods: `get`, `pop`, `update`, `clear`, `copy`
- Set methods: `add`, `remove`, `discard`, `clear`, `copy`
- String methods: `upper`
- Classes with fields, methods, and class attributes
- Single inheritance with method overrides and `super().__init__` calls
- Decorators: `@property` (getter/setter), `@staticmethod`, `@classmethod`, and simple top-level function decorators
- Enum-like `Union` aliases via `A | B` type aliases
- `match/case` on union variants with `__match_args__` support
- Custom iterators via `__iter__` and `next`
- Simple list and set comprehensions
- Builtins: `abs`, `all`, `any`, `bin`, `bool`, `bytes`, `chr`, `dict`, `divmod`, `enumerate`, `filter`, `float`, `hash`, `hex`, `id`, `int`, `isinstance`, `len`, `list`, `map`, `max`, `min`, `oct`, `ord`, `pow`, `range`, `repr`, `reversed`, `round`, `set`, `str`, `sum`, `tuple`, `type`, `zip`

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

Runtime integration coverage lives in `crates/py2rust/tests/runtime/`:
- `collections.rs` covers lists, tuples, dicts, sets, bytes.
- `builtins.rs` covers builtin functions.
and others.

## Runtime helpers
The generated Rust injects tiny helper functions only when needed:
- `py_print`
- `py_len`
- `py_range`
- `py_round` etc.

## Notes and Limitations
- `self` can be unannotated in methods; other parameters require annotations.
- `Union[A, B]` and `A | B` are allowed only for enum-like class unions.
- `from typing import ...` is treated as a no-op; `Union`, `Optional`, and `Iterator` are built-in in annotations.
- Keyword arguments are supported only for `dict()`; other keyword args are rejected.
- `__init__` is treated as a constructor; it must only assign `self` fields.
- Class decorators and decorator calls are rejected (e.g. `@decorator()` or `@dataclass`).
- Function decorators are limited to simple names on top-level functions.
- `dict` indexing raises `KeyError` (propagated as `PyError`).
- Tuple slicing requires literal integer bounds (including negative literals).
- String slicing is character-based (Unicode scalar values).
- f-strings support literal-only format specs (limited subset).

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
