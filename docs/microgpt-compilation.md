# Compiling microgpt.py: Insights and Patterns

## Overview

`microgpt.py` is Karpathy's minimal GPT training script that uses a `Value` class for autograd with operator overloading, the `random` module, and unannotated `__init__` fields. Getting it through the py2rust transpiler required solving several interrelated problems across lowering, type checking, and code generation.

**Result:** The generated `microgpt.rs` compiles with 0 errors. The backward pass uses Clone semantics (not reference semantics), so gradient propagation won't work correctly at runtime — this is a known limitation pending Phase 4 (Rc<RefCell> wrapping).

## Key Challenges and Solutions

### 1. Unannotated `__init__` Fields

**Problem:** microgpt's `Value.__init__` assigns `self.data`, `self._children`, `self._local_grads`, `self.grad` without type annotations. The transpiler required annotations.

**Solution:** In `lower/function.rs`, `extract_init_fields_from_ast` was modified to create `FieldDef` with `TypeRef::Unknown` for unannotated fields instead of emitting an error. The type checker resolves the concrete type later from call-site context via multi-pass refinement.

**Insight:** Deferring type resolution from lowering to typechecking allows the transpiler to handle more natural Python code. The key principle: *lower as Unknown, resolve later*.

### 2. Multi-Pass Type Refinement

**Problem:** Types propagated forward through the program but not backward. When `Value(data, (self, other), (1, 1))` was called, the parameter types were known at the call site but not propagated back to the class field declarations.

**Solution:** Three-layer approach:
1. **Iterative call-site inference** (5-iteration convergence loop) in `check_signatures` resolves function parameter types from all call sites.
2. **Backward type propagation** updates function parameter types back to call-site variables.
3. **Multi-pass refresh with shared env** (`refresh_call_types_in_items_multi_pass`) runs 3 passes sharing a single `HashMap<String, Type>` so backward-propagated types reach earlier `Let` declarations.

**Insight:** Forward-only type propagation is insufficient for real-world Python. The shared-env multi-pass pattern is critical: separate-env passes lose cross-pass information because each pass starts fresh.

### 3. Variable-Arity Tuples → Vec

**Problem:** `Value(data, (self, other), (1, 1))` vs `Value(data, (self,), (expr,))` vs `Value(data)` (default `()`) — the same field receives tuples of different arities.

**Solution:** When the type checker detects a field receiving tuples of different arities across call sites, it unifies the type to `Vec<T>`. Codegen converts tuple literals to `vec![...]`.

**Insight:** Python's duck-typing allows heterogeneous container shapes for the same name. The "collapse to Vec" strategy works for variable-length homogeneous collections. For truly heterogeneous tuples, this would need an enum wrapper (not yet needed).

### 4. Operator Overloading

**Problem:** `Value.__add__`, `__mul__`, `__neg__`, `__radd__`, `__rmul__`, `__pow__` needed to generate Rust trait implementations.

**Solution:**
- **Type checker** (`typecheck/expr/ops.rs`): Before erroring on "Binary arithmetic requires numeric types", check if LHS/RHS is `Type::Custom` with the appropriate dunder method.
- **Codegen** (`codegen/emit/items.rs`): `emit_operator_traits` generates `impl std::ops::Add for ClassName { ... }` for each dunder.
- **Reverse operators**: `__radd__` → `impl Add<ClassName> for f64`.
- **`__pow__`**: No standard trait → plain `.pow()` method.

**Insight:** The dunder → trait mapping is clean, but reverse operators (`__radd__`) are tricky because they flip the type relationship. The transpiler generates `impl Add<ClassName> for f64` which handles `2.0 + value` naturally.

### 5. Recursive Nested Functions

**Problem:** `backward()` contains `build_topo()` which calls itself recursively and captures `visited` and `topo` from the outer scope.

**Solution:**
- Emit as standalone `fn build_topo(v: Value, visited: &mut HashSet<Value>, topo: &mut Vec<Value>)`.
- Save/restore `local_vars` and `local_list_storage` around inner function body emission.
- Track `already_mut_ref_captures: HashSet<String>` to prevent double `&mut` referencing in recursive calls (the function already receives `&mut` params, so recursive calls pass them directly).

**Insight:** Scope management is the hardest part. The codegen maintains parallel state (local_vars, local_list_storage, already_mut_ref_captures) that must be properly sandboxed for inner functions and restored after. Missing any of these causes subtle type mismatches in generated Rust.

### 6. For-Loop Variable Mutability

**Problem:** `for child in v._children:` and `for p in self.parameters():` — variables `child` and `p` are mutated in the loop body (`child.grad += ...`, `p.data -= ...`) but generated without `mut`.

**Solution:**
- `gen_for_target` now accepts `mut_counts` and adds `mut` prefix for variables that appear in the map.
- `collect_assign_counts` was extended to track field assignments (`obj.field = val`) and method calls known to mutate (`backward`, `append`, etc.).

**Insight:** Rust's ownership model requires explicit `mut` declarations that Python doesn't need. The transpiler must trace mutations through field access and method calls, not just direct assignments.

### 7. Empty List Type Inference in Comprehensions

**Problem:** `[[] for _ in range(n)]` generates `vec![Vec::new(); n]` but Rust can't infer the inner Vec's element type from `Vec::new()` alone. Using `Vec::<PyRepr>::new()` everywhere works for standalone contexts but breaks when the comprehension context knows the concrete type.

**Solution:** `infer_empty_list_type` flag is set during comprehension element generation (inside `.push()` context). When set, empty lists emit `Vec::new()` allowing Rust to infer the type from the push context. Outside comprehensions, `Vec::<PyRepr>::new()` is still used.

**Insight:** Context-sensitive codegen (knowing you're inside a `.push()` call) enables better Rust type inference. The flag-based approach is simple but effective — it's essentially "let Rust infer vs. be explicit" based on whether the context provides enough information.

### 8. Iterator Source Type Resolution

**Problem:** `gen_iter_source` received `Some(Unknown)` from HIR for inner function parameters, which prevented the fallback to `local_var_type` lookups.

**Solution:** Filter `Some(Unknown)` before the `.or_else(...)` fallback:
```rust
let value_ty = value.ty.as_ref()
    .filter(|ty| !matches!(ty, Type::Unknown))
    .cloned()
    .or_else(|| { /* local_var_type lookup */ });
```

**Insight:** `Some(Unknown)` is semantically different from `None` — it means "we tried to resolve and failed" vs "we haven't tried yet". But for fallback purposes, both should trigger the same lookup chain. Always filter out Unknown before optional-based fallbacks.

## Architectural Patterns Discovered

### Save/Restore Pattern for Scope Management
Inner function emission requires saving and restoring multiple pieces of codegen state:
- `local_vars: Option<HashMap<String, Type>>`
- `local_list_storage: Option<HashMap<String, ListStorage>>`
- `already_mut_ref_captures: HashSet<String>`

This is a lightweight alternative to a full scope stack. Works well for single-level nesting; deeper nesting would benefit from a proper scope stack.

### Flag-Based Context Propagation
Rather than threading context through all function signatures, use transient flags on the Codegen struct:
- `infer_empty_list_type` — set during comprehension body, cleared after
- Similar to `IterContext` enum but for simpler boolean contexts

**Trade-off:** Simple but fragile. Works when the call graph is shallow (gen_expr → gen_list → check flag). Would break with deeply nested or reentrant generation.

### Shared-Env Multi-Pass
Running multiple type-resolution passes with a shared environment:
```rust
let mut env: HashMap<String, Type> = HashMap::new();
for _ in 0..passes {
    for item in &mut program.items { /* update types using env */ }
}
```
Each pass can read types established by previous passes. This is cheaper than a full fixed-point solver and handles the common case (2-3 passes sufficient for forward + backward propagation).

## Phase 4: Future Work (Reference Semantics)

The `Value` class in microgpt is a DAG node where `_children` stores **references** to other Value objects. With Clone semantics, `backward()` modifies copies instead of originals. Correct behavior requires:
- `Rc<RefCell<ValueInner>>` wrapping for self-referencing classes
- Pointer-based `Hash`/`Eq` (identity comparison, not value comparison)
- Field access through `borrow()`/`borrow_mut()`
- Operator trait impls on the Rc wrapper

This is the most complex pending feature, estimated at significant additional effort.

## Statistics

| Metric | Value |
|--------|-------|
| Starting compilation errors | ~30 |
| Final compilation errors | 0 |
| Test regressions introduced | 0 |
| Files modified (transpiler) | ~15 |
| New stdlib module added | `random` |
| New language features | operator overloading, unannotated fields, recursive nested functions |
