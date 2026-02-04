# AGENTS.md

Project: py2rust - a Rust transpiler for a restricted Python subset.

## Quick Start
- Build: `cargo build --release`
- Run transpiler: `./target/release/py2rust <input.py> --output <output.rs>`
- Compile generated Rust: `./target/release/py2rust <input.py> --compile`
- Compile and run: `./target/release/py2rust <input.py> --run`
- HIR/Types debug: `--emit-hir`, `--emit-types`, `--pretty`

## Repo Layout
- `crates/py2rust/src/lib.rs` core compile pipeline and `main` renaming.
- `crates/py2rust/src/lower.rs` RustPython AST -> HIR lowering.
- `crates/py2rust/src/hir.rs` HIR definitions.
- `crates/py2rust/src/typeck.rs` type checking and inference.
- `crates/py2rust/src/codegen.rs` Rust codegen and helper injection.
- `crates/py2rust/src/types.rs` type system.
- `README.md` user docs.

## Key Behaviors
- No runtime crate. Helpers are injected into generated Rust only when used.
- `__name__` is backed by `const __NAME__: &str = "__main__"`.
- `__name__` usage:
  - If only compared to string literals, emit `__NAME__ == "..."` without allocation.
  - Otherwise emit `__NAME__.to_string()` on each access.
  - Assigning to `__name__` is rejected by type checker.
- User-defined `def main()` is renamed to `__py_main` (or `__py_mainN`) to avoid collision.
  - All calls to `main()` are rewritten to the new name.
  - Top-level statements always generate Rust `fn main()`.

## Supported Python Subset (high-level)
- Functions, classes (plain data), if/elif/else, while, for, return, break/continue.
- Literals: int, float, bool, None, str.
- list/dict/tuple/set, indexing, slicing (limited), simple comprehension.
- `Union` for enum-like classes, `match/case` limited to union variants.
- `__iter__/next` for custom iterators.
- `lambda`, `if` expression, `round`, `len`, `range`, `enumerate`, `zip`, `map`, `filter`, `all`, `any`, `reversed`, `max`, `min`, `int`, `float`, `str`, `isinstance`, `type`.
- Decorators: one simple name decorator on top-level functions only (rewritten).
- Exception handling: `try/except/else/finally`, `raise`, exception propagation through function calls.

## Type System Notes
- `str` maps to `String` (not `&str`).
- `int -> i64`, `float -> f64`, `None -> ()`, `Optional[T] -> Option<T>`.
- `Union` is only for enum-like classes; inline unions only for `Optional`.
- Unknowns are allowed locally but resolved where possible.
- Lambdas and callables use `Type::Lambda` and are emitted as `impl Fn(..) -> .. + 'static`.

## Codegen Notes
- Numeric literals are emitted with suffixes (`i64`/`f64`) to avoid ambiguity.
- Mixed int/float arithmetic casts ints to f64 when result is float.
- Tuple concatenation clones elements to avoid move errors.
- List/set iteration for builtins uses `.iter().cloned()` to avoid moves.
- Set ops map to `&set1 | &set2`, `&set1 & &set2`, `&set1 - &set2`, `&set1 ^ &set2`.
- Exception handling uses `Result<T, PyError>` with closures for try blocks.
- Variables declared in try block are exposed to else via `Option<T>` wrapper.
- Functions with unhandled exceptions return `Result<T, PyError>`.

## Test Structure
Runtime integration tests are in `crates/py2rust/tests/`:
- `common/mod.rs` - shared `run_py()` helper for compile+execute tests.
- `runtime/*.rs` - categorized comprehensive runtime tests:
  - `functions.rs` - functions and recursion
  - `classes.rs` - classes and objects
  - `control_flow.rs` - loops and conditionals
  - `collections.rs` - lists, strings, tuples, dicts
  - `operators.rs` - arithmetic, comparison, boolean
  - `io.rs` - print output
  - `assert.rs` - assertions
  - `exceptions.rs` - try/except/finally/raise
- Each category has one comprehensive test to minimize compilation overhead.

## Common Pitfalls
- If you modify HIR, update typeck and codegen in sync.
- If you add new builtins, update:
  - typeck `check_call` for type rules
  - codegen `gen_expr` for emission
  - codegen `scan_expr` for helper imports
- Keep `#![forbid(unsafe_code)]` across crates.
- Always update tests when changing behavior.
- Update documentation in README.md and here as needed.
