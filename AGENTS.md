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

### Running the Transpiler
- `cargo run -p py2rust -- <input.py>` - transpile Python to Rust
- `cargo run -p py2rust -- <input.py> --output <output.rs>` - specify output file
- `cargo run -p py2rust -- <input.py> --compile` - transpile and compile with rustc (opt-level=3)
- `cargo run -p py2rust -- <input.py> --run` - transpile, compile, and execute (opt-level=3)
- `cargo run -p py2rust -- <input.py> --emit-hir` - show HIR representation
- `cargo run -p py2rust -- <input.py> --emit-types` - show type information
- `cargo run -p py2rust -- <input.py> --pretty` - format generated Rust with rustfmt

## Repo Layout
- `crates/py2rust/src/lib.rs` core compile pipeline and `main` renaming.
- `crates/py2rust/src/lower.rs` RustPython AST -> HIR lowering.
- `crates/py2rust/src/lower/function.rs` function/class lowering, `__init__` field extraction, `__slots__` handling.
- `crates/py2rust/src/hir.rs` HIR definitions.
- `crates/py2rust/src/hir_visit.rs` macro-generated visitor traits and `accept` dispatch for `ExprKind/StmtKind`.
- `crates/py2rust/src/callspec.rs` shared call-shape validation (arity/keywords) and canonical diagnostics.
- `crates/py2rust/src/call_bind.rs` shared argument binding planner used by typecheck and codegen.
- `crates/py2rust/src/typecheck/` type checking and inference.
- `crates/py2rust/src/typecheck/expr/ops.rs` binary/unary operator type checking, including dunder method resolution on custom types.
- `crates/py2rust/src/codegen/` Rust codegen and helper injection.
- `crates/py2rust/src/codegen/emit/items.rs` class struct/impl/trait emission, operator trait generation.
- `crates/py2rust/src/codegen/util.rs` `collect_assign_counts` for variable mutation tracking.
- `crates/py2rust/src/stdlib/registry.rs` stdlib module registry (os, sys, re, json, math, time, random, subprocess, urllib).
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
- list/dict/tuple/set, indexing, slicing (limited), and list/set/dict comprehensions.
- Tuple/list unpacking supports one starred target (`a, *rest, b = ...`).
- `Union` for enum-like classes.
- `match/case` for literals/singletons, capture and wildcard patterns, OR patterns, guards, and list sequence/star patterns.
- Class-pattern `match` on union variants with `__match_args__` support.
- `__iter__/next` for custom iterators.
- Generator functions via `yield`, including `.send(...)`, `.close()`, and generator expressions.
- `lambda`, `if` expression, `round`, `len`, `range`, `enumerate`, `zip`, `map`, `filter`, `all`, `any`, `iter`, `reversed`, `sorted`, `max`, `min`, `int`, `float`, `str`, `isinstance`, `type`.
- Operator overloading via dunder methods (`__add__`, `__sub__`, `__mul__`, `__truediv__`, `__pow__`, `__neg__`, and reverse variants).
- String methods: `upper`, `lower`.
- Decorators: one simple name decorator on top-level functions only (rewritten).
- Recursive nested functions emitted as standalone `fn` with captured `&mut` parameters.
- Exception handling: `try/except/else/finally`, `raise`, bare `raise` (re-raise), `except Exception` (catch-all), exception propagation through function calls.

## Type System Notes
- `str` maps to `String` (not `&str`).
- `int -> i64`, `float -> f64`, `None -> ()`, `Optional[T] -> Option<T>`.
- `typing.List/Dict/Set/Tuple` annotations are aliases for built-in `list/dict/set/tuple`.
- Dynamic-length `tuple()` construction is currently list-backed at runtime/typecheck time; fixed-arity tuple literals/annotations still use tuple typing.
- `bool` is accepted in numeric contexts as Python-compatible `int` subtype behavior.
- `round(x)` on float inputs currently preserves float result (`py_round(x, 0)`), while integer inputs remain integer.
- `Union` aliases are for enum-like class unions.
- Inline union annotations lower as:
  - `T | None` -> `Optional[T]`
  - wider `A | B` -> gradual typing (`Unknown`) fallback
- Unknowns are allowed locally but resolved where possible.
- Unannotated `self.field = value` in `__init__` creates `FieldDef` with `TypeRef::Unknown`; the type checker resolves the concrete type from call-site context.
- Multi-pass type refresh (`refresh_call_types_in_items_multi_pass`) shares a single `env: HashMap<String, Type>` across passes so that backward-propagated types from call sites reach earlier `Let` declarations.
- Variable-arity tuple fields (e.g., `children=()` receiving `(a, b)` and `(a,)` at different call sites) are unified to `Vec<T>` and tuple literals are converted to `vec![...]` at codegen time.
- Lambdas and callables use `Type::Lambda` and are emitted as `impl Fn(..) -> .. + 'static`.

## Codegen Notes
- Compilation: `--compile`/`--run` use `rustc -C opt-level=3` for optimized binaries. Tests keep `opt-level=0` for fast compilation.
- Read-only scalar globals (assigned exactly once, Int/Float/Bool) use `OnceLock<T>` without Mutex.
  Mutable globals still use `OnceLock<Mutex<T>>`. Detection in `analysis/globals.rs:collect_readonly_globals`.
- Numeric literals are emitted with suffixes (`i64`/`f64`) to avoid ambiguity.
- Mixed int/float arithmetic casts ints to f64 when result is float.
- Optional truthiness follows Python semantics (`Some(0)`, `Some("")`, etc. are falsy).
- Control-flow checks (`x is None`, `x is not None`, `if x`) narrow Optional values in branch-local codegen.
- Tuple concatenation:
  - Literal tuples are inlined directly: `tup + (1, 2)` → `(tup.0.clone(), tup.1.clone(), 1, 2)`
  - Variables use temporary bindings to avoid redundant clones
- For loop unpacking:
  - Simple tuple targets generate direct pattern loops:
    - `for (a, b) in pairs:` → `for (a, b) in pairs.iter().cloned() {`
  - Complex tuple/list targets (including starred) are lowered via per-iteration assignment:
    - `for a, *rest in items:` → loop over temp item + unpack assignment inside loop body
- Iterator generation for `Arc<Mutex<Vec<T>>>`:
  - `IterContext::ImmediateConsumption` holds lock once for entire iteration (for loops, builtins)
  - `IterContext::DeferredCapture` locks per-iteration when iterator is returned/stored (map/filter results)
  - `zip(a, b)` in for loops is intercepted by `gen_iter_source` to use `ImmediateConsumption` for both sides
- Generator functions are emitted as dedicated iterator wrapper structs with replay-based state,
  supporting `next(...)`, `.send(...)`, and `.close()` on the same object.
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
- Mutex re-entrance limitation: `Rc<RefCell>` and `Arc<Mutex>` storage for lists/dicts cannot
  handle recursive data structures or re-entrant access within the same guard scope. Self-comparisons
  (e.g., `x == x`) use `ptr_eq` to short-circuit and avoid deadlocks or borrow panics.
- Variables declared in try blocks are captured for use in both `else` and `finally` blocks via
  `Option<T>` snapshot variables.
- Operator overloading on custom types:
  - Dunder methods (`__add__`, `__sub__`, `__mul__`, `__truediv__`, `__neg__`) generate `std::ops` trait implementations.
  - `__pow__` has no standard trait — emitted as a `.pow()` method, called via `left.pow(right)`.
  - Reverse operators (`__radd__`, `__rsub__`, `__rmul__`, `__rtruediv__`) generate `impl Add<ClassName> for f64` (and `i64`).
  - Operands of Custom types are `.clone()`'d since `Add` etc. take `self` by value.
- Recursive nested functions:
  - Detected at lowering/codegen time when a nested `def` calls itself.
  - Emitted as standalone `fn` items with captured variables as explicit `&mut` parameters.
  - `already_mut_ref_captures: HashSet<String>` prevents double `&mut` referencing in recursive calls within the inner function body.
  - Codegen saves/restores `local_vars` and `local_list_storage` scopes around inner function body emission.
- For-loop target mutability:
  - `gen_for_target` checks `mut_counts` and adds `mut` prefix for variables mutated in the loop body.
  - `collect_assign_counts` tracks field assignments (`obj.field = ...`) and known mutating method calls (including `backward`).
- Empty list type inference in comprehensions:
  - `infer_empty_list_type` flag is set during comprehension element generation (inside `.push()` context).
  - When set, empty lists emit `Vec::new()` instead of `Vec::<PyRepr>::new()`, allowing Rust type inference from the push context.
  - Outside comprehensions, `Vec::<PyRepr>::new()` is still used to avoid ambiguity.
- `__slots__` declarations in class bodies are silently ignored (CPython memory optimization hint only).

## Test Structure
Runtime integration tests are in `crates/py2rust/tests/`:
- `common/mod.rs` - shared `run_py()` helper for compile+execute tests.
- `runtime.rs` - centralized runtime-case registry macro (`runtime_cases!`) that executes all runtime fixtures.
- `runtime/*.py` - runtime fixture programs used by the registry.
- Runtime expectations are centralized in `runtime.rs` next to fixture registration.

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
- If you add new stdlib modules, update `stdlib/registry.rs`: module id, method specs, type rules, codegen handlers, and helper implementations in `codegen/emit/helpers.rs`.
- If you modify statement handling in codegen, update `collect_assign_counts` in `util.rs` to track variable mutations in new statement types. Mutating method names are hardcoded there — add new ones as needed (e.g., `"backward"` for custom class methods).
- When modifying inner/nested function codegen in `stmt.rs`, always save/restore `local_vars` and `local_list_storage` scopes to avoid outer scope pollution.
- When adding captured variables to inner functions, check `already_mut_ref_captures` to prevent double `&mut` referencing in recursive calls.
- `gen_iter_source` may receive `Some(Unknown)` from HIR — filter it before falling back to `local_var_type` lookups.
- Keep `#![forbid(unsafe_code)]` across crates.
- Avoid .unwrap(), use .expect() with context or proper error handling.
- Always update tests when changing behavior.
- Update documentation in README.md and here as needed.
