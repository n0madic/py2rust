# py2rust

A pragmatic transpiler that converts a restricted, statically-typed subset of Python into idiomatic Rust with a minimal runtime.

## Project Layout
- `crates/py2rust` — compiler/transpiler (Rust)

## Features (MVP)
- Functions with required type annotations
- Basic types: `int`, `float`, `bool`, `str`, `None`
- Collections: `list[T]`, `dict[K, V]`, `tuple[...]`
- Control flow: `if/elif/else`, `while`, `for`, `return`, `break`, `continue`
- Simple classes (plain data) with methods
- Enum-like `Union` aliases via `A | B` type aliases
- `match/case` on union variants
- Custom iterators via `__iter__` and `next`
- Simple list comprehensions

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

## Runtime helpers
The generated Rust injects tiny helper functions only when needed:
- `py_print`
- `py_len`
- `py_range`
- `py_round` etc.

## Notes and Limitations
- `self` can be unannotated in methods; other parameters require annotations.
- `Union[A, B]` and `A | B` are allowed only for enum-like class unions.
- `__init__` is treated as a constructor; it must only assign `self` fields.
- `dict` indexing uses `HashMap` indexing and will panic on missing keys.
- String slicing uses byte offsets (Rust rules apply).
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
