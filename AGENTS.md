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
- `docs/` project documentation (keep docs here).
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
- Exception handling: `try/except/else/finally`, `raise`, bare `raise` (re-raise), `except Exception` (catch-all), exception propagation through function calls.

## Type System Notes
- `str` maps to `String` (not `&str`).
- `int -> i64`, `float -> f64`, `None -> ()`, `Optional[T] -> Option<T>`.
- `Union` is only for enum-like classes; inline unions only for `Optional`.
- Unknowns are allowed locally but resolved where possible.
- Lambdas and callables use `Type::Lambda` and are emitted as `impl Fn(..) -> .. + 'static`.

## Codegen Notes
- Numeric literals are emitted with suffixes (`i64`/`f64`) to avoid ambiguity.
- Mixed int/float arithmetic casts ints to f64 when result is float.
- Tuple concatenation:
  - Literal tuples are inlined directly: `tup + (1, 2)` → `(tup.0.clone(), tup.1.clone(), 1, 2)`
  - Variables use temporary bindings to avoid redundant clones
- For loop tuple unpacking generates direct pattern matching:
  - `for (a, b) in pairs:` → `for (a, b) in pairs.iter().cloned() {`
  - Supports nested unpacking in HIR via `ForTarget` enum
- Iterator generation for `Arc<Mutex<Vec<T>>>`:
  - `IterContext::ImmediateConsumption` holds lock once for entire iteration (for loops, builtins)
  - `IterContext::DeferredCapture` locks per-iteration when iterator is returned/stored (map/filter results)
- List/set iteration for builtins uses `.iter().cloned()` to avoid moves.
- Set ops map to `&set1 | &set2`, `&set1 & &set2`, `&set1 - &set2`, `&set1 ^ &set2`.
- Exception handling uses `Result<T, PyError>` with closures for try blocks.
- Variables declared in try block are exposed to else via `Option<T>` wrapper.
- Functions with unhandled exceptions return `Result<T, PyError>`.
- Supported exception types: `Exception` (catch-all), `ValueError`, `TypeError`, `RuntimeError`, `KeyError`, `IndexError`, `AttributeError`, `ZeroDivisionError`, `NameError`, `AssertionError`, `StopIteration`, `NotImplementedError`, `IOError`, `OverflowError`.
- Bare `raise` in except handler re-raises the current exception.
- `raise X from Y` (exception chaining) is not supported and produces a compile error.

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

## Testing
**Best practices for test development:**

- After making changes to test files, run `cargo test` immediately and fix any variable shadowing or return type issues before proceeding to the next task.
- Verify that generated test code compiles before moving on to the next test case.
- Keep tests simple and avoid patterns that may conflict with compiler limitations.

## Development Workflow
**CRITICAL: Always run tests after making changes!**
**RECOMMENDED: Run `cargo clippy` and `cargo fmt` before committing changes**

After completing any code changes:
1. Run the full test suite: `cargo test`
2. If any tests fail, fix the issues before considering the work complete
3. Never leave broken tests - all tests must pass before finishing
4. If you add new functionality, add corresponding test coverage
5. Update all relevant documentation and keep it in `docs/`

## Code Documentation
**Always add comments to the code you write!**

- Add doc comments (`///` in Rust) to all public functions, structs, enums, and traits
- Add inline comments (`//`) to explain complex logic, non-obvious decisions, or important invariants
- Comment any tricky parts of the code that might not be immediately clear
- When implementing new features, explain the "why" not just the "what"
- Keep comments concise but informative

## Rust Code Generation
**Guidelines for generating correct and maintainable Rust code:**

- **Type Consistency**: Always ensure type consistency with existing code patterns. Check existing type signatures before introducing new types or enums that interact with existing functions. Read the relevant source files first to understand current type usage.
- **Compiler Constraints**: When refactoring or reorganizing code, verify compiler capability limitations before creating complex patterns. Prefer simpler implementations that work within current toolchain constraints.
- **Incremental Verification**: After editing Rust source files, run `cargo check` to catch type mismatches and compilation errors early, before they compound across multiple files.

## Common Pitfalls
- If you modify HIR, update typeck and codegen in sync.
- If you add new builtins, update:
  - typeck `check_call` for type rules
  - codegen `gen_expr` for emission
  - codegen `scan_expr` for helper imports
- If you modify statement handling in codegen, update `collect_assign_counts` in `util.rs` to track variable mutations in new statement types.
- Keep `#![forbid(unsafe_code)]` across crates.
- Avoid .unwrap(), use .expect() with context or proper error handling.
- Always update tests when changing behavior.
- Update documentation in README.md and here as needed.
