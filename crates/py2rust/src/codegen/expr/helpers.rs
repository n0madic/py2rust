// Shared helpers for expression generation and typing behavior.

use super::super::util::collect_assign_counts;
use super::super::*;
use std::mem;

impl<'a> Codegen<'a> {
    /// Render a list of expression arguments as a comma-separated string.
    pub(super) fn gen_args(&mut self, args: &[Expr]) -> Result<String, CompileError> {
        let parts: Result<Vec<String>, CompileError> =
            args.iter().map(|a| self.gen_expr(a)).collect();
        Ok(parts?.join(", "))
    }

    /// Generate an expression while applying an expected type when needed.
    pub(crate) fn gen_expr_with_expected(
        &mut self,
        expr: &Expr,
        expected: Option<&Type>,
    ) -> Result<String, CompileError> {
        if let Some(Type::Lambda { params, .. }) = expected {
            if let ExprKind::Lambda {
                params: names,
                body,
            } = &expr.kind
            {
                return self.gen_lambda_with_param_types(names, body, Some(params.as_slice()));
            }
        }
        if let Some(Type::Option(_)) = expected {
            if matches!(expr.ty.as_ref(), Some(Type::Option(_))) {
                return self.gen_expr(expr);
            }
            if matches!(expr.kind, ExprKind::Literal(Literal::None)) {
                return Ok("None".to_string());
            }
            let inner = self.gen_expr(expr)?;
            return Ok(format!("Some({})", inner));
        }
        self.gen_expr(expr)
    }

    /// Generate arguments for a call using an explicit parameter type list.
    pub(super) fn gen_call_args_for_sig(
        &mut self,
        param_types: &[Type],
        args: &[Expr],
    ) -> Result<String, CompileError> {
        let mut parts = Vec::new();
        for (idx, arg) in args.iter().enumerate() {
            // Cache the parameter type lookup to avoid duplicate .get() calls.
            let param_ty = param_types.get(idx);
            let rendered = if let Some(ty) = param_ty {
                self.gen_expr_with_expected(arg, Some(ty))?
            } else {
                self.gen_expr(arg)?
            };
            if let Some(param_ty) = param_ty {
                // Shared containers and owned strings must be cloned to avoid moves.
                if matches!(
                    param_ty,
                    Type::List(_) | Type::Dict(_, _) | Type::Str | Type::Bytes
                ) {
                    parts.push(format!("{}.clone()", rendered));
                    continue;
                }
                if self.needs_borrow(arg.ty.as_ref(), param_ty) {
                    parts.push(format!("&{}", rendered));
                } else {
                    parts.push(rendered);
                }
            } else {
                parts.push(rendered);
            }
        }
        Ok(parts.join(", "))
    }

    /// Check if we need to add & when passing an argument.
    fn needs_borrow(&self, arg_ty: Option<&Type>, param_ty: &Type) -> bool {
        match param_ty {
            // Parameter expects a slice, argument is a list.
            Type::Slice(_) => {
                matches!(arg_ty, Some(Type::List(_)))
            }
            // Parameter expects &str, argument is String.
            Type::Ref(inner) if matches!(inner.as_ref(), Type::Str) => {
                matches!(arg_ty, Some(Type::Str))
            }
            // Parameter expects &dict, argument is dict.
            Type::Ref(inner) if matches!(inner.as_ref(), Type::Dict(_, _)) => {
                matches!(arg_ty, Some(Type::Dict(_, _)))
            }
            // Parameter expects &HashSet, argument is HashSet.
            Type::Ref(inner) if matches!(inner.as_ref(), Type::Set(_)) => {
                matches!(arg_ty, Some(Type::Set(_)))
            }
            // Parameter expects &Custom, argument is Custom.
            Type::Ref(inner) if matches!(inner.as_ref(), Type::Custom(_)) => {
                matches!(arg_ty, Some(Type::Custom(_)))
            }
            // Parameter expects &Union, argument is Union.
            Type::Ref(inner) if matches!(inner.as_ref(), Type::Union(_)) => {
                matches!(arg_ty, Some(Type::Union(_)))
            }
            _ => false,
        }
    }

    /// Generate a numeric operand, casting ints to floats when required.
    pub(super) fn gen_numeric_operand(
        &mut self,
        expr: &Expr,
        target_float: bool,
    ) -> Result<String, CompileError> {
        let rendered = self.gen_expr(expr)?;
        if target_float && matches!(expr.ty.as_ref(), Some(Type::Int)) {
            return Ok(format!("({} as f64)", rendered));
        }
        Ok(rendered)
    }

    /// Build an iterator source expression that matches Python iteration semantics.
    pub(crate) fn gen_iter_source(&mut self, expr: &Expr) -> Result<IterSource, CompileError> {
        // Optimize literal lists - generate local Vec without Arc<Mutex<>> overhead.
        if let ExprKind::List(items) = &expr.kind {
            if let Some(Type::List(inner)) = expr.ty.as_ref() {
                let list_expr = self.gen_list_expr_with_storage(expr, items, ListStorage::Local)?;
                let iter_expr = if self.is_copy_type(inner) {
                    format!("{}.iter().copied()", list_expr)
                } else {
                    format!("{}.iter().cloned()", list_expr)
                };
                return Ok(IterSource {
                    setup: Vec::new(),
                    expr: iter_expr,
                });
            }
        }

        let rendered = self.gen_expr(expr)?;
        let use_owned = match &expr.kind {
            ExprKind::Name(name) => self.is_global(name),
            _ => true,
        };
        match expr.ty.as_ref() {
            // Slice references: just .iter() - items are already references.
            Some(Type::Slice(_)) => Ok(IterSource {
                setup: Vec::new(),
                expr: format!("{}.iter().copied()", rendered),
            }),
            // Lists are shared (Arc<Mutex<...>>), so keep the lock guard in scope.
            Some(Type::List(inner)) => {
                if matches!(self.list_storage_for_expr(expr), ListStorage::Local) {
                    let expr = if self.is_copy_type(inner) {
                        format!("{}.iter().copied()", rendered)
                    } else {
                        format!("{}.iter().cloned()", rendered)
                    };
                    return Ok(IterSource {
                        setup: Vec::new(),
                        expr,
                    });
                }
                let tmp = self.new_tmp();
                let guard = self.new_tmp();
                let iter_expr = if self.is_copy_type(inner) {
                    format!("{}.iter().copied()", guard)
                } else {
                    format!("{}.iter().cloned()", guard)
                };
                Ok(IterSource {
                    // Clone the Arc to avoid moving out of the source expression.
                    setup: vec![
                        format!("let {} = {}.clone()", tmp, rendered),
                        format!(
                            "let {} = {}.lock().expect(\"list mutex poisoned\")",
                            guard, tmp
                        ),
                    ],
                    expr: iter_expr,
                })
            }
            Some(Type::Dict(key_ty, _)) => {
                if matches!(self.dict_storage_for_expr(expr), DictStorage::Local) {
                    let iter_expr = if self.is_copy_type(key_ty) {
                        format!("{}.keys().copied()", rendered)
                    } else {
                        format!("{}.keys().cloned()", rendered)
                    };
                    return Ok(IterSource {
                        setup: Vec::new(),
                        expr: iter_expr,
                    });
                }
                let tmp = self.new_tmp();
                let guard = self.new_tmp();
                let iter_expr = if self.is_copy_type(key_ty) {
                    format!("{}.keys().copied()", guard)
                } else {
                    format!("{}.keys().cloned()", guard)
                };
                Ok(IterSource {
                    // Clone the Arc to avoid moving out of the source expression.
                    setup: vec![
                        format!("let {} = {}.clone()", tmp, rendered),
                        format!(
                            "let {} = {}.lock().expect(\"dict mutex poisoned\")",
                            guard, tmp
                        ),
                    ],
                    expr: iter_expr,
                })
            }
            // Owned sets need .iter().cloned() (or .copied() for Copy types).
            Some(Type::Set(inner)) => {
                let expr = if use_owned {
                    format!("{}.into_iter()", rendered)
                } else if self.is_copy_type(inner) {
                    format!("{}.iter().copied()", rendered)
                } else {
                    format!("{}.iter().cloned()", rendered)
                };
                Ok(IterSource {
                    setup: Vec::new(),
                    expr,
                })
            }
            Some(Type::Bytes) => {
                let expr = if use_owned {
                    format!("{}.into_iter()", rendered)
                } else {
                    format!("{}.iter().copied()", rendered)
                };
                Ok(IterSource {
                    setup: Vec::new(),
                    expr,
                })
            }
            Some(Type::Str) => {
                let expr = if use_owned {
                    format!(
                        "{}.chars().map(|c| c.to_string()).collect::<Vec<_>>().into_iter()",
                        rendered
                    )
                } else {
                    format!("{}.chars().map(|c| c.to_string())", rendered)
                };
                Ok(IterSource {
                    setup: Vec::new(),
                    expr,
                })
            }
            Some(Type::Tuple(items)) => {
                if items.is_empty() {
                    return Ok(IterSource {
                        setup: Vec::new(),
                        expr: "std::iter::empty::<()>()".to_string(),
                    });
                }
                let tmp = self.new_tmp();
                let mut elems = Vec::new();
                for (idx, ty) in items.iter().enumerate() {
                    if self.is_copy_type(ty) {
                        elems.push(format!("{}.{}", tmp, idx));
                    } else {
                        elems.push(format!("{}.{}.clone()", tmp, idx));
                    }
                }
                if use_owned {
                    Ok(IterSource {
                        setup: Vec::new(),
                        expr: format!(
                            "{{ let {} = {}; vec![{}].into_iter() }}",
                            tmp,
                            rendered,
                            elems.join(", ")
                        ),
                    })
                } else {
                    Ok(IterSource {
                        setup: Vec::new(),
                        expr: format!(
                            "{{ let {} = &{}; vec![{}].into_iter() }}",
                            tmp,
                            rendered,
                            elems.join(", ")
                        ),
                    })
                }
            }
            // Iterators are already iterable; avoid redundant .into_iter() on ranges.
            Some(Type::Iterator(_)) => Ok(IterSource {
                setup: Vec::new(),
                expr: rendered,
            }),
            // References to collections.
            Some(Type::Ref(inner)) => match inner.as_ref() {
                Type::Set(elem) => {
                    let expr = if self.is_copy_type(elem) {
                        format!("{}.iter().copied()", rendered)
                    } else {
                        format!("{}.iter().cloned()", rendered)
                    };
                    Ok(IterSource {
                        setup: Vec::new(),
                        expr,
                    })
                }
                _ => Ok(IterSource {
                    setup: Vec::new(),
                    expr: format!("{}.iter()", rendered),
                }),
            },
            _ => Ok(IterSource {
                setup: Vec::new(),
                expr: format!("{}.into_iter()", rendered),
            }),
        }
    }

    /// Build an iterator expression that can be returned or stored safely.
    ///
    /// The `context` parameter determines the locking strategy for `Arc<Mutex<Vec<T>>>`:
    /// - `ImmediateConsumption`: Holds the mutex lock for the entire iteration.
    ///   Use for: for loops, enumerate, zip, all/any, sum, and other builtins that
    ///   consume the iterator immediately in the same scope.
    ///   Generated pattern: `list.lock().unwrap().iter().cloned()`
    /// - `DeferredCapture`: Acquires/releases the lock on each iteration.
    ///   Use for: map/filter results that may be returned or stored, or when the
    ///   iterator escapes the current scope.
    ///   Generated pattern: `std::iter::from_fn(move || { list.lock().unwrap().get(i)... })`
    ///
    /// For `ListStorage::Local` (non-escaping Vec<T>), the context is ignored and
    /// a simple index-based iterator is generated without mutex overhead.
    pub(crate) fn gen_iter_source_owned(
        &mut self,
        expr: &Expr,
        context: IterContext,
    ) -> Result<String, CompileError> {
        let rendered = self.gen_expr(expr)?;
        match expr.ty.as_ref() {
            // Lists need a guard that lives inside the returned iterator.
            Some(Type::List(inner)) => {
                if matches!(self.list_storage_for_expr(expr), ListStorage::Local) {
                    let idx = self.new_tmp();
                    let list_ref = self.new_tmp();
                    let item_expr = if self.is_copy_type(inner) {
                        format!("{list}[{idx}]", list = list_ref, idx = idx)
                    } else {
                        format!("{list}[{idx}].clone()", list = list_ref, idx = idx)
                    };
                    // Index-based iteration avoids cloning the whole Vec for local lists.
                    return Ok(format!(
                        "{{ let {list_ref} = &{list}; let mut {idx}: usize = 0; std::iter::from_fn(move || {{ if {idx} < {list_ref}.len() {{ let item = {item}; {idx} += 1; Some(item) }} else {{ None }} }}) }}",
                        idx = idx,
                        list = rendered,
                        list_ref = list_ref,
                        item = item_expr
                    ));
                }

                // Optimize for immediate consumption: single lock for entire iteration
                if context == IterContext::ImmediateConsumption {
                    let iter_method = if self.is_copy_type(inner) {
                        ".iter().copied()"
                    } else {
                        ".iter().cloned()"
                    };
                    return Ok(format!(
                        "{}.lock().expect(\"list mutex poisoned\"){}",
                        rendered, iter_method
                    ));
                }

                // Deferred capture: lock per-iteration to enable storing/returning
                let tmp = self.new_tmp();
                let idx = self.new_tmp();
                let guard = self.new_tmp();
                let item_expr = if self.is_copy_type(inner) {
                    format!("{}[{}]", guard, idx)
                } else {
                    format!("{}[{}].clone()", guard, idx)
                };
                // Lock the list per-iteration to avoid holding a guard across expression boundaries.
                Ok(format!(
                    "{{ let {tmp} = {expr}.clone(); let mut {idx}: usize = 0; std::iter::from_fn(move || {{ let {guard} = {tmp}.lock().expect(\"list mutex poisoned\"); if {idx} < {guard}.len() {{ let item = {item}; {idx} += 1; Some(item) }} else {{ None }} }}) }}",
                    tmp = tmp,
                    expr = rendered,
                    guard = guard,
                    idx = idx,
                    item = item_expr
                ))
            }
            Some(Type::Dict(key_ty, _)) => {
                let iter_method = if self.is_copy_type(key_ty) {
                    "keys().copied()"
                } else {
                    "keys().cloned()"
                };
                if matches!(self.dict_storage_for_expr(expr), DictStorage::Local) {
                    // Local dicts can borrow directly for immediate consumption.
                    if context == IterContext::ImmediateConsumption {
                        return Ok(format!("{}.{}", rendered, iter_method));
                    }
                    // Snapshot keys to avoid borrowing across escaped iterators.
                    let keys = self.new_tmp();
                    return Ok(format!(
                        "{{ let {keys} = {expr}.{iter}.collect::<Vec<_>>(); {keys}.into_iter() }}",
                        keys = keys,
                        expr = rendered,
                        iter = iter_method
                    ));
                }
                // For immediate consumption, keep the lock for the iterator lifetime.
                if context == IterContext::ImmediateConsumption {
                    return Ok(format!(
                        "{}.lock().expect(\"dict mutex poisoned\").{}",
                        rendered, iter_method
                    ));
                }
                // Snapshot keys to avoid holding the lock across escaped iterators.
                let tmp = self.new_tmp();
                let guard = self.new_tmp();
                let keys = self.new_tmp();
                Ok(format!(
                    "{{ let {tmp} = {expr}.clone(); let {guard} = {tmp}.lock().expect(\"dict mutex poisoned\"); let {keys} = {guard}.{iter}.collect::<Vec<_>>(); {keys}.into_iter() }}",
                    tmp = tmp,
                    expr = rendered,
                    guard = guard,
                    keys = keys,
                    iter = iter_method
                ))
            }
            _ => Ok(self.gen_iter_source(expr)?.expr),
        }
    }

    /// Check if a type implements Copy (primitives).
    pub(crate) fn is_copy_type(&self, ty: &Type) -> bool {
        matches!(ty, Type::Int | Type::Float | Type::Bool)
    }

    /// Provide a best-effort element type hint for iterable types.
    pub(crate) fn iter_item_type_hint(&self, ty: &Type) -> Option<Type> {
        match ty {
            Type::List(inner) | Type::Set(inner) => Some(*inner.clone()),
            Type::Dict(key, _) => Some(*key.clone()),
            Type::Tuple(items) => {
                if items.is_empty() {
                    None
                } else if items.iter().all(|t| t == &items[0]) {
                    Some(items[0].clone())
                } else {
                    None
                }
            }
            Type::Str => Some(Type::Str),
            Type::Bytes => Some(Type::Int),
            Type::Iterator(inner) => Some(*inner.clone()),
            Type::Ref(inner) | Type::MutRef(inner) | Type::Slice(inner) => {
                self.iter_item_type_hint(inner)
            }
            _ => None,
        }
    }

    /// Compute the truthiness test for a given type.
    pub(super) fn truthy_expr_for_type(&mut self, expr_str: &str, ty: &Type) -> String {
        let expr = match ty {
            Type::Bool => expr_str.to_string(),
            Type::Int => format!("{} != 0", expr_str),
            Type::Float => format!("{} != 0.0", expr_str),
            Type::Str => format!("!{}.is_empty()", expr_str),
            Type::Bytes | Type::Set(_) => format!("!{}.is_empty()", expr_str),
            Type::Dict(_, _) => {
                self.uses.len = true;
                format!("py_len(&{}) != 0", expr_str)
            }
            Type::List(_) => {
                // Use py_len so both Vec and Arc<Mutex<Vec<T>>> are supported.
                self.uses.len = true;
                format!("py_len(&{}) != 0", expr_str)
            }
            Type::Tuple(items) => {
                if items.is_empty() {
                    "false".to_string()
                } else {
                    "true".to_string()
                }
            }
            Type::None => "false".to_string(),
            Type::Option(inner) => {
                let inner_expr = self.truthy_expr_for_type("v", inner);
                format!(
                    "match {} {{ Some(v) => {}, None => false }}",
                    expr_str, inner_expr
                )
            }
            Type::Ref(inner) | Type::MutRef(inner) | Type::Slice(inner) => {
                self.truthy_expr_for_type(expr_str, inner)
            }
            _ => "true".to_string(),
        };
        format!("({})", expr)
    }

    /// Decide whether we are in a context that expects Result propagation.
    fn in_throwing_context(&self) -> bool {
        if self.lambda_depth > 0 {
            return false;
        }
        if self.try_block_return_type.is_some() {
            return true;
        }
        if let Some(Type::Result(_, _)) = self.current_function_ret.as_ref() {
            return true;
        }
        self.current_function.is_none() && self.top_level_can_throw
    }

    /// Wrap a Result-returning expression with ? or a panic depending on context.
    pub(crate) fn wrap_result(&self, expr: String) -> String {
        if self.in_throwing_context() {
            format!("({}?)", expr)
        } else {
            // Match CPython-style crashes by panicking with the PyError display message.
            format!(
                "{}.unwrap_or_else(|e| panic!(\"Unhandled exception: {{}}\", e))",
                expr
            )
        }
    }

    /// Wrap parse helper results using the current throwing context.
    pub(super) fn wrap_parse_result(&self, expr: String) -> String {
        self.wrap_result(expr)
    }

    /// Map a type to its Python name string when available.
    pub(super) fn python_type_name(&self, ty: &Type) -> Option<String> {
        let name = match ty {
            Type::Int => "int",
            Type::Float => "float",
            Type::Bool => "bool",
            Type::Str => "str",
            Type::Bytes => "bytes",
            Type::None => "NoneType",
            Type::List(_) => "list",
            Type::Tuple(_) => "tuple",
            Type::Dict(_, _) => "dict",
            Type::Set(_) => "set",
            Type::Custom(name) | Type::Union(name) => return Some(name.clone()),
            _ => return None,
        };
        Some(name.to_string())
    }

    /// Map a type to its Python class string (e.g., "<class 'int'>").
    pub(super) fn python_type_class(&self, ty: &Type) -> Option<String> {
        self.python_type_name(ty)
            .map(|name| format!("<class '{}'>", name))
    }

    /// Check if a type is already a reference type.
    pub(super) fn is_reference_type(&self, ty: Option<&Type>) -> bool {
        matches!(
            ty,
            Some(Type::Ref(_)) | Some(Type::MutRef(_)) | Some(Type::Slice(_))
        )
    }

    /// Check if a name refers to a borrowed parameter.
    pub(super) fn is_borrowed_param(&self, name: &str) -> bool {
        self.borrowed_params.contains(name)
    }

    /// Emit a block expression into a temporary output buffer.
    pub(super) fn gen_block_expr(&mut self, stmts: &[Stmt]) -> Result<String, CompileError> {
        let mut_counts = collect_assign_counts(stmts);
        let saved_out = mem::take(&mut self.out);
        let saved_indent = self.indent;
        let saved_tmp = self.tmp_counter;
        self.out = String::new();
        self.indent = 0;
        self.push_line("{");
        self.indent += 1;
        for stmt in stmts {
            self.emit_stmt(stmt, &mut_counts)?;
        }
        self.indent -= 1;
        self.push_line("}");
        let block = self.out.trim_end().to_string();
        self.out = saved_out;
        self.indent = saved_indent;
        self.tmp_counter = saved_tmp;
        Ok(block)
    }

    /// Decide whether to use Debug formatting when printing an expression.
    pub(super) fn print_needs_debug(&self, expr: &Expr) -> bool {
        let ty = match expr.ty.as_ref() {
            Some(Type::Unknown) | None => {
                if let ExprKind::Name(name) = &expr.kind {
                    self.local_var_type(name)
                } else {
                    None
                }
            }
            Some(other) => Some(other),
        };
        match ty {
            Some(Type::Int | Type::Float | Type::Bool | Type::Str | Type::None) => false,
            Some(_) => true,
            None => true,
        }
    }

    /// Build an expression suitable for Debug formatting of a value.
    pub(super) fn debug_arg_expr(&mut self, expr: &Expr) -> Result<String, CompileError> {
        let rendered = self.gen_expr(expr)?;
        if matches!(expr.ty.as_ref(), Some(Type::List(_))) {
            if matches!(self.list_storage_for_expr(expr), ListStorage::Local) {
                return Ok(format!("&{}", rendered));
            }
            return Ok(format!(
                "{}.lock().expect(\"list mutex poisoned\")",
                rendered
            ));
        }
        Ok(rendered)
    }

    /// Build a list repr expression that matches the list storage strategy.
    pub(super) fn list_str_expr(&mut self, expr: &Expr) -> Result<String, CompileError> {
        self.uses.py_list_str = true;
        if let ExprKind::List(items) = &expr.kind {
            let tmp = self.new_tmp();
            let list_expr = self.gen_list_expr_with_storage(expr, items, ListStorage::Local)?;
            // Print temporary list literals without Arc<Mutex<...>> overhead.
            return Ok(format!(
                "{{ let {tmp} = {list}; py_list_str_vec(&{tmp}) }}",
                tmp = tmp,
                list = list_expr
            ));
        }
        if let ExprKind::ListComp {
            elt,
            target,
            iter,
            ifs,
        } = &expr.kind
        {
            let tmp = self.new_tmp();
            let list_expr =
                self.gen_list_comp_expr_with_storage(elt, target, iter, ifs, ListStorage::Local)?;
            // Keep list comprehension results local when formatting.
            return Ok(format!(
                "{{ let {tmp} = {list}; py_list_str_vec(&{tmp}) }}",
                tmp = tmp,
                list = list_expr
            ));
        }
        // Optimize list(iterable) calls for immediate consumption - no Arc<Mutex> needed.
        if let ExprKind::Call { func, args } = &expr.kind {
            if let ExprKind::Name(name) = &func.kind {
                if name == "list" && args.len() == 1 {
                    let tmp = self.new_tmp();
                    let iter_src = self.gen_iter_source(&args[0])?;
                    let list_expr = format!("({}).collect::<Vec<_>>()", iter_src.expr);
                    let body = format!(
                        "{{ let {tmp} = {list}; py_list_str_vec(&{tmp}) }}",
                        tmp = tmp,
                        list = list_expr
                    );
                    return Ok(iter_src.wrap(body));
                }
            }
        }
        let rendered = self.gen_expr(expr)?;
        if matches!(self.list_storage_for_expr(expr), ListStorage::Local) {
            return Ok(format!("py_list_str_vec(&{})", rendered));
        }
        Ok(format!("py_list_str(&{})", rendered))
    }
}
