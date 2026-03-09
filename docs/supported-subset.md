# Supported Python Subset

## Statements & Control Flow
- Functions, classes (plain data), if/elif/else, while, for, return, break/continue.
- Exception handling: `try/except/else/finally`, `raise`, bare `raise` (re-raise), `except Exception` (catch-all).
- `match/case` for literals/singletons, capture and wildcard patterns, OR patterns, guards, list sequence/star patterns.
- Class-pattern `match` on union variants with `__match_args__` support.

## Expressions & Literals
- Literals: int, float, bool, None, str.
- list/dict/tuple/set, indexing, slicing (limited), list/set/dict comprehensions.
- Tuple/list unpacking with one starred target (`a, *rest, b = ...`).
- `lambda`, ternary `if` expression.
- Generator functions via `yield`, `.send(...)`, `.close()`, generator expressions.

## Builtins
`round`, `len`, `range`, `enumerate`, `zip`, `map`, `filter`, `all`, `any`, `iter`, `reversed`, `sorted`, `max`, `min`, `int`, `float`, `str`, `isinstance`, `type`.

## Classes & OOP
- `Union` for enum-like class unions.
- `__iter__`/`next` for custom iterators.
- Operator overloading: `__add__`, `__sub__`, `__mul__`, `__truediv__`, `__pow__`, `__neg__`, reverse variants.
- String methods: `upper`, `lower`.
- Decorators: one simple name decorator on top-level functions (rewritten).
- Recursive nested functions as standalone `fn` with captured `&mut` parameters.

## Type System
- `str` → `String`, `int` → `i64`, `float` → `f64`, `None` → `()`, `Optional[T]` → `Option<T>`.
- `typing.List/Dict/Set/Tuple` as aliases for builtins.
- `bool` accepted in numeric contexts (Python-compatible `int` subtype).
- `Union` aliases for enum-like class unions.
- `T | None` → `Optional[T>`, wider `A | B` → gradual typing fallback.
- Unannotated `self.field = value` creates `FieldDef` with `TypeRef::Unknown`, resolved from call-site.
- Multi-pass type refresh shares env across passes for backward propagation.
- Variable-arity tuple fields unified to `Vec<T>`.
- Lambdas/callables use `Type::Lambda`, emitted as `impl Fn(..) -> .. + 'static`.
