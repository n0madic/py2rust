# AGENTS.md

Project: py2rust - a Rust transpiler for a restricted Python subset.

**Goals:** Native executables, static typing, CPython-compatible behavior, good error diagnostics
**Non-goals:** Full CPython compatibility, dynamic features (`eval/exec`, metaclasses)

## Build/Test/Lint Commands

### Building
- `cargo build` - debug build
- `cargo build --release` - optimized build
- `cargo check` - fast type checking (recommended during development)
- `cargo check --all-targets` - check all targets including tests

### Testing
- `cargo test` - run all tests
- `cargo test -p py2rust` - py2rust crate only
- `cargo test <testname>` - specific test by name
- `cargo test -- --nocapture` - show stdout/stderr
- `cargo test -- --test-threads=1` - serial (for debugging)

### Linting and Formatting
- `cargo clippy` - run linter (recommended before commits)
- `cargo clippy -- -D warnings` - treat warnings as errors
- `cargo fmt` - format all code

### Running the Transpiler
- `cargo run -p py2rust -- <input.py>` - transpile Python to Rust
- `cargo run -p py2rust -- <input.py> --compile` - transpile and compile (opt-level=3)
- `cargo run -p py2rust -- <input.py> --run` - transpile, compile, and execute
- `cargo run -p py2rust -- <input.py> --emit-hir` - show HIR
- `cargo run -p py2rust -- <input.py> --emit-types` - show type info
- `cargo run -p py2rust -- <input.py> --pretty` - format generated Rust

## Repo Layout
- `crates/py2rust/src/lib.rs` — core compile pipeline, `main` renaming
- `crates/py2rust/src/lower.rs` — RustPython AST → HIR lowering
- `crates/py2rust/src/lower/function.rs` — function/class lowering, `__init__` fields, `__slots__`
- `crates/py2rust/src/hir.rs` — HIR definitions
- `crates/py2rust/src/hir_visit.rs` — macro-generated visitor traits
- `crates/py2rust/src/callspec.rs` — call-shape validation, arity/keywords
- `crates/py2rust/src/call_bind.rs` — argument binding planner (typecheck + codegen)
- `crates/py2rust/src/typecheck/` — type checking and inference
- `crates/py2rust/src/typecheck/expr/ops.rs` — binary/unary ops, dunder method resolution
- `crates/py2rust/src/codegen/` — Rust codegen and helper injection
- `crates/py2rust/src/codegen/emit/items.rs` — class struct/impl/trait emission
- `crates/py2rust/src/codegen/util.rs` — `collect_assign_counts` for mutation tracking
- `crates/py2rust/src/stdlib/registry.rs` — stdlib module registry
- `crates/py2rust/src/types.rs` — type system
- `docs/` — project documentation

## Key Behaviors
- No runtime crate. Helpers injected into generated Rust only when used.
- `__name__` backed by `const __NAME__: &str = "__main__"`.
  - Compared to literals: `__NAME__ == "..."` (no allocation).
  - Otherwise: `__NAME__.to_string()`. Assigning rejected by typechecker.
- Global scoping follows CPython rules:
  - Local assignment shadows module names unless `global name` declared.
  - `global name` must precede first use and only for module-scope names.
- User `def main()` renamed to `__py_main` (calls rewritten). Top-level always generates Rust `fn main()`.

## Supported Subset & Type System
See @docs/supported-subset.md for the full list of supported Python features and type mappings.

## Test Structure
Runtime integration tests in `crates/py2rust/tests/`:
- `common/mod.rs` — shared `run_py()` helper
- `runtime.rs` — `runtime_cases!` macro registry
- `runtime/*.py` — fixture programs
- Expectations centralized in `runtime.rs` next to fixture registration

## Development Workflow
**CRITICAL: Always run tests after making changes!**
**RECOMMENDED: Run `cargo clippy` and `cargo fmt` before committing**

1. Run the full test suite: `cargo test`
2. Fix any failures before considering work complete
3. Add test coverage for new functionality
4. Update documentation in `docs/`

## Rust Code Generation Guidelines
- **Type Consistency**: Check existing type signatures before introducing new types that interact with existing functions.
- **Compiler Constraints**: Prefer simpler implementations within current toolchain constraints.
- **Incremental Verification**: Run `cargo check` after edits to catch errors early.
