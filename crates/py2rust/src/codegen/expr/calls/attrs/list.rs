// List attribute call lowering.

use super::super::super::*;
use super::AttrValueTarget;

impl<'a> Codegen<'a> {
    /// Lower list method calls.
    pub(super) fn gen_list_attr_call(
        &mut self,
        value: &Expr,
        attr: &str,
        args: &[Expr],
        _keywords: &[KeywordArg],
    ) -> Result<String, CompileError> {
        if attr == "append" {
            if let Some(Type::List(_)) = value.ty.as_ref() {
                let target = if let ExprKind::Name(name) = &value.kind {
                    if self.is_global(name) {
                        format!(
                            "{}.lock().expect(\"list mutex poisoned\")",
                            self.global_lock_expr(name)
                        )
                    } else if self.is_local_list_name(name) {
                        return Ok(format!("{}.push({})", name, self.gen_args(args)?));
                    } else {
                        format!(
                            "{}.lock().expect(\"list mutex poisoned\")",
                            self.gen_expr(value)?
                        )
                    }
                } else {
                    format!(
                        "{}.lock().expect(\"list mutex poisoned\")",
                        self.gen_expr(value)?
                    )
                };
                return Ok(format!("{}.push({})", target, self.gen_args(args)?));
            }
        }
        if attr == "extend" {
            if let Some(Type::List(_)) = value.ty.as_ref() {
                let mut target = None;
                if let ExprKind::Name(name) = &value.kind {
                    if self.is_global(name) {
                        target = Some(format!(
                            "{}.lock().expect(\"list mutex poisoned\")",
                            self.global_lock_expr(name)
                        ));
                    } else if self.is_local_list_name(name) {
                        target = Some(name.clone());
                    }
                }
                let target = match target {
                    Some(expr) => expr,
                    None => format!(
                        "{}.lock().expect(\"list mutex poisoned\")",
                        self.gen_expr(value)?
                    ),
                };
                if args.is_empty() {
                    return Ok(format!("{{ {}.extend(std::iter::empty()); }}", target));
                }
                let arg = &args[0];
                // Avoid moving the source list/tuple by iterating and cloning elements.
                if matches!(arg.ty.as_ref(), Some(Type::Tuple(_))) {
                    let tuple_tmp = self.new_tmp();
                    let arg_expr = self.gen_expr(arg)?;
                    let mut elems = Vec::new();
                    if let Some(Type::Tuple(items)) = arg.ty.as_ref() {
                        for idx in 0..items.len() {
                            elems.push(format!("{}.{}.clone()", tuple_tmp, idx));
                        }
                    }
                    return Ok(format!(
                        "{{ let {} = {}; {}.extend(vec![{}]); }}",
                        tuple_tmp,
                        arg_expr,
                        target,
                        elems.join(", ")
                    ));
                }
                let arg_expr = self.gen_expr(arg)?;
                if matches!(arg.ty.as_ref(), Some(Type::List(_))) {
                    if matches!(self.list_storage_for_expr(arg), ListStorage::Local) {
                        return Ok(format!("{}.extend({}.iter().cloned())", target, arg_expr));
                    }
                    return Ok(format!(
                        "{}.extend({}.lock().expect(\"list mutex poisoned\").iter().cloned())",
                        target, arg_expr
                    ));
                }
                return Ok(format!("{}.extend({}.into_iter())", target, arg_expr));
            }
        }
        if attr == "pop" {
            if let Some(Type::List(_)) = value.ty.as_ref() {
                if args.len() > 1 {
                    return Err(self.error(value.span, "list.pop() expects zero or one argument"));
                }
                let idx_arg = args.first();
                if let ExprKind::Name(name) = &value.kind {
                    if !self.is_global(name) && self.is_local_list_name(name) {
                        if let Some(arg) = idx_arg {
                            let idx_raw = self.gen_expr(arg)?;
                            self.uses.py_index = true;
                            let len_tmp = self.new_tmp();
                            let idx_tmp = self.new_tmp();
                            return Ok(format!(
                                "{{ let {len_tmp} = {target}.len(); let {idx_tmp} = {idx_expr}; {target}.remove({idx_tmp}) }}",
                                len_tmp = len_tmp,
                                idx_tmp = idx_tmp,
                                idx_expr = self.wrap_result(format!(
                                    "py_index({}, {})",
                                    idx_raw, len_tmp
                                )),
                                target = name
                            ));
                        }
                        let pop_expr = format!(
                            "{}.pop().ok_or_else(|| PyError::IndexError(\"IndexError\".into()))",
                            name
                        );
                        return Ok(self.wrap_result(pop_expr));
                    }
                }
                if let Some(arg) = idx_arg {
                    let idx_raw = self.gen_expr(arg)?;
                    self.uses.py_index = true;
                    let len_tmp = self.new_tmp();
                    let idx_tmp = self.new_tmp();
                    let idx_expr = self.wrap_result(format!("py_index({}, {})", idx_raw, len_tmp));
                    return self.with_locked_attr_target(
                        value,
                        "list mutex poisoned",
                        true,
                        |_tc, guard| {
                            format!(
                                "let {len_tmp} = {guard}.len(); let {idx_tmp} = {idx_expr}; {guard}.remove({idx_tmp})",
                                len_tmp = len_tmp,
                                guard = guard,
                                idx_tmp = idx_tmp,
                                idx_expr = idx_expr
                            )
                        },
                    );
                }
                return self.with_locked_attr_target(
                    value,
                    "list mutex poisoned",
                    true,
                    |tc, guard| {
                        tc.wrap_result(format!(
                            "{guard}.pop().ok_or_else(|| PyError::IndexError(\"IndexError\".into()))",
                            guard = guard
                        ))
                    },
                );
            }
        }
        if attr == "insert" {
            if let Some(Type::List(inner)) = value.ty.as_ref() {
                if args.len() != 2 {
                    return Err(self.error(value.span, "list.insert() expects two arguments"));
                }
                let idx_raw = self.gen_expr(&args[0])?;
                let val_expr = self.gen_expr_with_expected(&args[1], Some(inner.as_ref()))?;
                self.uses.py_insert_index = true;
                if let ExprKind::Name(name) = &value.kind {
                    if !self.is_global(name) && self.is_local_list_name(name) {
                        let len_tmp = self.new_tmp();
                        let idx_tmp = self.new_tmp();
                        return Ok(format!(
                            "{{ let {len_tmp} = {target}.len(); let {idx_tmp} = py_insert_index({idx_raw}, {len_tmp}); {target}.insert({idx_tmp}, {val}); }}",
                            len_tmp = len_tmp,
                            idx_tmp = idx_tmp,
                            idx_raw = idx_raw,
                            target = name,
                            val = val_expr
                        ));
                    }
                }
                if let ExprKind::Name(name) = &value.kind {
                    if self.is_global(name) {
                        let outer = self.new_tmp();
                        let guard = self.new_tmp();
                        let len_tmp = self.new_tmp();
                        let idx_tmp = self.new_tmp();
                        return Ok(format!(
                            "{{ let {outer} = {lock}; let mut {guard} = {outer}.lock().expect(\"list mutex poisoned\"); let {len_tmp} = {guard}.len(); let {idx_tmp} = py_insert_index({idx_raw}, {len_tmp}); {guard}.insert({idx_tmp}, {val}); }}",
                            outer = outer,
                            guard = guard,
                            lock = self.global_lock_expr(name),
                            len_tmp = len_tmp,
                            idx_tmp = idx_tmp,
                            idx_raw = idx_raw,
                            val = val_expr
                        ));
                    }
                }
                let target_expr = self.gen_expr(value)?;
                if !matches!(value.kind, ExprKind::Name(_)) {
                    let tmp = self.new_tmp();
                    let guard = self.new_tmp();
                    let len_tmp = self.new_tmp();
                    let idx_tmp = self.new_tmp();
                    return Ok(format!(
                        "{{ let {tmp} = {target}; let mut {guard} = {tmp}.lock().expect(\"list mutex poisoned\"); let {len_tmp} = {guard}.len(); let {idx_tmp} = py_insert_index({idx_raw}, {len_tmp}); {guard}.insert({idx_tmp}, {val}); }}",
                        tmp = tmp,
                        guard = guard,
                        target = target_expr,
                        len_tmp = len_tmp,
                        idx_tmp = idx_tmp,
                        idx_raw = idx_raw,
                        val = val_expr
                    ));
                }
                let len_tmp = self.new_tmp();
                let idx_tmp = self.new_tmp();
                return Ok(format!(
                    "{{ let mut guard = {target}.lock().expect(\"list mutex poisoned\"); let {len_tmp} = guard.len(); let {idx_tmp} = py_insert_index({idx_raw}, {len_tmp}); guard.insert({idx_tmp}, {val}); }}",
                    len_tmp = len_tmp,
                    idx_tmp = idx_tmp,
                    idx_raw = idx_raw,
                    target = target_expr,
                    val = val_expr
                ));
            }
        }
        if attr == "clear" {
            if let Some(Type::List(_)) = value.ty.as_ref() {
                if !args.is_empty() {
                    return Err(self.error(value.span, "list.clear() expects no arguments"));
                }
                if let ExprKind::Name(name) = &value.kind {
                    if !self.is_global(name) && self.is_local_list_name(name) {
                        return Ok(format!("{{ {}.clear(); }}", name));
                    }
                }
                return self.with_locked_attr_target(
                    value,
                    "list mutex poisoned",
                    true,
                    |_tc, guard| format!("{guard}.clear();", guard = guard),
                );
            }
        }
        if attr == "copy" {
            if let Some(Type::List(_)) = value.ty.as_ref() {
                if !args.is_empty() {
                    return Err(self.error(value.span, "list.copy() expects no arguments"));
                }
                if let ExprKind::Name(name) = &value.kind {
                    if self.is_global(name) {
                        return Ok(format!(
                            "Arc::new(Mutex::new({}.lock().expect(\"list mutex poisoned\").clone()))",
                            self.global_lock_expr(name)
                        ));
                    }
                    if self.is_local_list_name(name) {
                        return Ok(format!("Arc::new(Mutex::new({}.clone()))", name));
                    }
                }
                return match self.resolve_attr_value_target(value)? {
                    AttrValueTarget::GlobalName(name) => Ok(format!(
                        "Arc::new(Mutex::new({}.lock().expect(\"list mutex poisoned\").clone()))",
                        self.global_lock_expr(&name)
                    )),
                    AttrValueTarget::Name(name) => Ok(format!(
                        "Arc::new(Mutex::new({}.lock().expect(\"list mutex poisoned\").clone()))",
                        name
                    )),
                    AttrValueTarget::Expr(target) => Ok(format!(
                        "Arc::new(Mutex::new({}.lock().expect(\"list mutex poisoned\").clone()))",
                        target
                    )),
                };
            }
        }
        if attr == "reverse" {
            if let Some(Type::List(_)) = value.ty.as_ref() {
                if !args.is_empty() {
                    return Err(self.error(value.span, "list.reverse() expects no arguments"));
                }
                if let ExprKind::Name(name) = &value.kind {
                    if !self.is_global(name) && self.is_local_list_name(name) {
                        return Ok(format!("{{ {}.reverse(); }}", name));
                    }
                }
                return self.with_locked_attr_target(
                    value,
                    "list mutex poisoned",
                    true,
                    |_tc, guard| format!("{guard}.reverse();", guard = guard),
                );
            }
        }
        if attr == "index" {
            if let Some(Type::List(_)) = value.ty.as_ref() {
                if args.len() != 1 {
                    return Err(self.error(value.span, "list.index() expects one argument"));
                }
                self.uses.py_list_index = true;
                let needle_expr = self.gen_expr(&args[0])?;
                if let ExprKind::Name(name) = &value.kind {
                    if !self.is_global(name) && self.is_local_list_name(name) {
                        let call = format!(
                            "py_list_index(&{target}, &{needle})",
                            target = name,
                            needle = needle_expr
                        );
                        return Ok(self.wrap_result(call));
                    }
                }
                return self.with_locked_attr_target(
                    value,
                    "list mutex poisoned",
                    false,
                    |tc, guard| {
                        tc.wrap_result(format!(
                            "py_list_index(&{guard}, &{needle})",
                            guard = guard,
                            needle = needle_expr
                        ))
                    },
                );
            }
        }
        if attr == "sort" {
            if let Some(Type::List(inner)) = value.ty.as_ref() {
                if !args.is_empty() {
                    return Err(self.error(value.span, "list.sort() expects no arguments"));
                }
                let sort_call = if matches!(inner.as_ref(), Type::Float) {
                    // `total_cmp` avoids panics for NaN while still providing deterministic order.
                    "sort_by(|a, b| a.total_cmp(b))"
                } else {
                    "sort()"
                };
                if let ExprKind::Name(name) = &value.kind {
                    if !self.is_global(name) && self.is_local_list_name(name) {
                        return Ok(format!("{{ {}.{}; }}", name, sort_call));
                    }
                }
                return self.with_locked_attr_target(
                    value,
                    "list mutex poisoned",
                    true,
                    |_tc, guard| {
                        format!("{guard}.{sort_call};", guard = guard, sort_call = sort_call)
                    },
                );
            }
        }
        if attr == "count" {
            if let Some(Type::List(_)) = value.ty.as_ref() {
                if args.len() != 1 {
                    return Err(self.error(value.span, "list.count() expects one argument"));
                }
                self.uses.py_list_count = true;
                let needle_expr = self.gen_expr(&args[0])?;
                if let ExprKind::Name(name) = &value.kind {
                    if !self.is_global(name) && self.is_local_list_name(name) {
                        return Ok(format!("py_list_count(&{}, &{})", name, needle_expr));
                    }
                }
                return self.with_locked_attr_target(
                    value,
                    "list mutex poisoned",
                    false,
                    |_tc, guard| {
                        format!(
                            "py_list_count(&{guard}, &{needle})",
                            guard = guard,
                            needle = needle_expr
                        )
                    },
                );
            }
        }
        Err(self.error(
            value.span,
            format!("Internal error: unsupported list method `{attr}`"),
        ))
    }
}
