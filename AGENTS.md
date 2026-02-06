# AGENTS.md

Project: py2rust - a Rust transpiler for a restricted Python subset.

**Goals:** Native executables, static typing, CPython-compatible behavior, good error diagnostics
**Non-goals:** Full CPython compatibility, dynamic features (`eval/exec`, metaclasses)

## Build/Test/Lint Commands

### Building
- `cargo build` - debug build
- `cargo build --release` - optimized build
- `cargo build -p py2rust` - build only py2rust crate
- `cargo check` - fast type checking without codegen (recommended during development)
- `cargo check --all-targets` - check all targets including tests

### Testing
- `cargo test` - run all tests in workspace
- `cargo test -p py2rust` - run tests for py2rust crate only
- `cargo test <testname>` - run specific test containing name (e.g., `cargo test runtime_functions`)
- `cargo test runtime::functions` - run specific test module
- `cargo test -- --nocapture` - show stdout/stderr from tests
- `cargo test -- --test-threads=1` - run tests serially (useful for debugging)

### Linting and Formatting
- `cargo clippy` - run linter (recommended before commits)
- `cargo clippy --all-targets` - lint all targets including tests
- `cargo clippy -- -D warnings` - treat warnings as errors
- `cargo fmt` - format all code with rustfmt
- `cargo fmt --check` - check formatting without modifying files

### Running the Transpiler
- `cargo run -p py2rust -- <input.py>` - transpile Python to Rust
- `cargo run -p py2rust -- <input.py> --output <output.rs>` - specify output file
- `cargo run -p py2rust -- <input.py> --compile` - transpile and compile with rustc
- `cargo run -p py2rust -- <input.py> --run` - transpile, compile, and execute
- `cargo run -p py2rust -- <input.py> --emit-hir` - show HIR representation
- `cargo run -p py2rust -- <input.py> --emit-types` - show type information
- `cargo run -p py2rust -- <input.py> --pretty` - format generated Rust with rustfmt

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
- Global scoping follows CPython rules:
  - Function-local assignment shadows module names unless `global name` is declared.
  - `global name` must be declared before first use in the function.
  - `global name` is valid only when `name` exists at module scope.
- User-defined `def main()` is renamed to `__py_main` (or `__py_mainN`) to avoid collision.
  - All calls to `main()` are rewritten to the new name.
  - Top-level statements always generate Rust `fn main()`.

## Supported Python Subset (high-level)
- Functions, classes (plain data), if/elif/else, while, for, return, break/continue.
- Literals: int, float, bool, None, str.
- list/dict/tuple/set, indexing, slicing (limited), simple comprehension.
- Tuple/list unpacking supports one starred target (`a, *rest, b = ...`).
- `Union` for enum-like classes.
- `match/case` for literals/singletons, capture and wildcard patterns, OR patterns, guards, and list sequence/star patterns.
- Class-pattern `match` on union variants with `__match_args__` support.
- `__iter__/next` for custom iterators.
- `lambda`, `if` expression, `round`, `len`, `range`, `enumerate`, `zip`, `map`, `filter`, `all`, `any`, `reversed`, `max`, `min`, `int`, `float`, `str`, `isinstance`, `type`.
- String methods: `upper`, `lower`.
- Decorators: one simple name decorator on top-level functions only (rewritten).
- Exception handling: `try/except/else/finally`, `raise`, bare `raise` (re-raise), `except Exception` (catch-all), exception propagation through function calls.

## Type System Notes
- `str` maps to `String` (not `&str`).
- `int -> i64`, `float -> f64`, `None -> ()`, `Optional[T] -> Option<T>`.
- `typing.List/Dict/Set/Tuple` annotations are aliases for built-in `list/dict/set/tuple`.
- `bool` is accepted in numeric contexts as Python-compatible `int` subtype behavior.
- `round(x)` on float inputs currently preserves float result (`py_round(x, 0)`), while integer inputs remain integer.
- `Union` aliases are for enum-like class unions.
- Inline union annotations lower as:
  - `T | None` -> `Optional[T]`
  - wider `A | B` -> gradual typing (`Unknown`) fallback
- Unknowns are allowed locally but resolved where possible.
- Lambdas and callables use `Type::Lambda` and are emitted as `impl Fn(..) -> .. + 'static`.

## Codegen Notes
- Numeric literals are emitted with suffixes (`i64`/`f64`) to avoid ambiguity.
- Mixed int/float arithmetic casts ints to f64 when result is float.
- Optional truthiness follows Python semantics (`Some(0)`, `Some("")`, etc. are falsy).
- Control-flow checks (`x is None`, `x is not None`, `if x`) narrow Optional values in branch-local codegen.
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
- Pattern matching:
  - Runtime patterns are lowered to `if/elif` chains (subject evaluated once).
  - Supported runtime patterns: literal/singleton, capture/wildcard, OR, guards, list sequence/star.
  - `__match_args__` defines field order for pattern matching (CPython 3.10+ compatible)
  - If not specified, uses field declaration order
  - All fields must appear in Rust pattern, unbound fields use `_`
  - Class-pattern guards are currently rejected.
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
  - `match.rs` - comprehensive `match/case` patterns
  - `global_scoping.rs` - global declarations, shadowing, and nested global writes
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

## Code Style Guidelines

### General Principles
- Follow Rust 2021 edition conventions
- Keep `#![forbid(unsafe_code)]` across all crates - no unsafe code allowed
- Use `#![allow(clippy::result_large_err)]` for crates with large error types (CompileError includes source text)
- Prefer `expect()` with context over `unwrap()` - always provide error messages
- Use proper error handling with `Result` and `miette::Report` for user-facing errors

### Imports and Module Organization
- Group imports: std library, external crates, internal crates, then local modules
- Use explicit imports rather than glob imports (`use foo::*`)
- Keep module hierarchy flat when possible (see `crates/py2rust/src/` structure)
- Module files: use `mod.rs` for modules with submodules (e.g., `codegen/mod.rs`)

### Type System and Naming
- Enum types: use PascalCase variants (e.g., `Type::Int`, `Type::Float`)
- Functions and variables: use snake_case (e.g., `check_call`, `user_main`)
- Constants: use SCREAMING_SNAKE_CASE (e.g., `MAX_RENAME_ATTEMPTS`, `__NAME__`)
- Type annotations: prefer explicit types in function signatures, allow inference locally
- Use `Box<Type>` for recursive enum variants to prevent infinite size

### Error Handling
- User-facing errors: use `miette::Report` with `CompileError::new(message, span, source, filename)`
- Internal errors: use `Result<T, E>` with meaningful error types (e.g., `thiserror`)
- Always include span information for compile-time errors to show users where the problem is
- Exception handling in codegen: functions with unhandled exceptions return `Result<T, PyError>`

### Code Documentation
**Always add comments to the code you write!**

- Add doc comments (`///`) to all public functions, structs, enums, and traits
- Add inline comments (`//`) to explain complex logic, non-obvious decisions, or important invariants
- Document why, not just what - explain design decisions and tradeoffs
- Add examples in doc comments for non-obvious APIs
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
