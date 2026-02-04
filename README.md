# py2rust

A pragmatic transpiler that converts a restricted, statically-typed subset of Python into idiomatic Rust with a minimal runtime.

## Project Layout
- `crates/py2rust` — compiler/transpiler (Rust)

## Features (MVP)
- Functions with required type annotations
- Basic types: `int`, `float`, `bool`, `str`, `bytes`, `None`
- Collections: `list[T]`, `dict[K, V]`, `tuple[...]`, `set[T]`
- Comparisons: `==`, `!=`, `<`, `<=`, `>`, `>=`, `in`, `not in`
- Control flow: `if/elif/else`, `while`, `for`, `return`, `break`, `continue`
- Tuple/list unpacking assignments (including nested)
- Negative indexing and slicing for lists/tuples (including step)
- List methods: `append`, `extend`, `pop`, `insert`, `clear`, `copy`, `reverse`, `sort`, `index`, `count`
- Dict methods: `get`, `pop`, `update`, `clear`, `copy`
- Set methods: `add`, `remove`, `discard`, `clear`, `copy`
- Classes with fields, methods, and class attributes
- Single inheritance with method overrides and `super().__init__` calls
- Decorators: `@property` (getter/setter), `@staticmethod`, `@classmethod`, and simple top-level function decorators
- Enum-like `Union` aliases via `A | B` type aliases
- `match/case` on union variants
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

```python
class Circle:
    def __init__(self, r: float) -> None:
        self.r: float = r

class Rect:
    def __init__(self, w: float, h: float) -> None:
        self.w: float = w
        self.h: float = h

Shape = Circle | Rect

def area(s: Shape) -> float:
    match s:
        case Circle(r):
            return 3.14 * r * r
        case Rect(w, h):
            return w * h
```

Transpile and compile:

```bash
cargo run -p py2rust -- input.py -o output.rs
rustc output.rs
```
