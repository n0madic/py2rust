// Dict attribute call lowering.

use super::super::super::*;

impl<'a> Codegen<'a> {
    /// Lower dict method calls.
    pub(super) fn gen_dict_attr_call(
        &mut self,
        value: &Expr,
        attr: &str,
        args: &[Expr],
        _keywords: &[KeywordArg],
    ) -> Result<String, CompileError> {
        if attr == "get" {
            if let Some(Type::Dict(_, _)) = value.ty.as_ref() {
                if args.is_empty() || args.len() > 2 {
                    return Err(self.error(value.span, "dict.get() expects one or two arguments"));
                }
                self.uses.hash_map = true;
                let key_expr = self.gen_expr(&args[0])?;
                let default_expr = if args.len() == 2 {
                    Some(self.gen_expr(&args[1])?)
                } else {
                    None
                };
                if matches!(self.dict_storage_for_expr(value), DictStorage::Local) {
                    let target_expr = self.gen_expr(value)?;
                    if let Some(default_expr) = default_expr {
                        return Ok(format!(
                            "{target}.get(&{key}).cloned().unwrap_or({default})",
                            target = target_expr,
                            key = key_expr,
                            default = default_expr
                        ));
                    }
                    self.uses.py_dict_get = true;
                    return Ok(
                        self.wrap_result(format!("py_dict_get(&{}, &{})", target_expr, key_expr))
                    );
                }
                if let ExprKind::Name(name) = &value.kind {
                    if self.is_global(name) {
                        // Global dicts store an Arc<Mutex<...>> inside a global lock.
                        let outer = self.new_tmp();
                        let guard = self.new_tmp();
                        if let Some(default_expr) = default_expr {
                            return Ok(format!(
                                "{{ let {outer} = {lock}; let {guard} = {outer}.lock().expect(\"dict mutex poisoned\"); {guard}.get(&{key}).cloned().unwrap_or({default}) }}",
                                outer = outer,
                                guard = guard,
                                lock = self.global_lock_expr(name),
                                key = key_expr,
                                default = default_expr
                            ));
                        }
                        self.uses.py_dict_get = true;
                        return Ok(self.wrap_result(format!(
                            "{{ let {outer} = {lock}; let {guard} = {outer}.lock().expect(\"dict mutex poisoned\"); py_dict_get(&{guard}, &{key}) }}",
                            outer = outer,
                            guard = guard,
                            lock = self.global_lock_expr(name),
                            key = key_expr
                        )));
                    }
                }
                let target_expr = self.gen_expr(value)?;
                if let Some(default_expr) = default_expr {
                    let guard = self.new_tmp();
                    if !matches!(value.kind, ExprKind::Name(_)) {
                        let tmp = self.new_tmp();
                        return Ok(format!(
                            "{{ let {tmp} = {target}; let {guard} = {tmp}.lock().expect(\"dict mutex poisoned\"); {guard}.get(&{key}).cloned().unwrap_or({default}) }}",
                            tmp = tmp,
                            target = target_expr,
                            guard = guard,
                            key = key_expr,
                            default = default_expr
                        ));
                    }
                    return Ok(format!(
                        "{{ let {guard} = {target}.lock().expect(\"dict mutex poisoned\"); {guard}.get(&{key}).cloned().unwrap_or({default}) }}",
                        guard = guard,
                        target = target_expr,
                        key = key_expr,
                        default = default_expr
                    ));
                }
                self.uses.py_dict_get = true;
                let guard = self.new_tmp();
                if !matches!(value.kind, ExprKind::Name(_)) {
                    let tmp = self.new_tmp();
                    return Ok(self.wrap_result(format!(
                        "{{ let {tmp} = {target}; let {guard} = {tmp}.lock().expect(\"dict mutex poisoned\"); py_dict_get(&{guard}, &{key}) }}",
                        tmp = tmp,
                        target = target_expr,
                        guard = guard,
                        key = key_expr
                    )));
                }
                return Ok(self.wrap_result(format!(
                    "{{ let {guard} = {target}.lock().expect(\"dict mutex poisoned\"); py_dict_get(&{guard}, &{key}) }}",
                    guard = guard,
                    target = target_expr,
                    key = key_expr
                )));
            }
        }
        if attr == "pop" {
            if let Some(Type::Dict(_, _)) = value.ty.as_ref() {
                if args.is_empty() || args.len() > 2 {
                    return Err(self.error(value.span, "dict.pop() expects one or two arguments"));
                }
                self.uses.hash_map = true;
                let key_expr = self.gen_expr(&args[0])?;
                let default_expr = if args.len() == 2 {
                    Some(self.gen_expr(&args[1])?)
                } else {
                    None
                };
                if matches!(self.dict_storage_for_expr(value), DictStorage::Local) {
                    let target_expr = self.gen_expr(value)?;
                    if let Some(default_expr) = default_expr {
                        return Ok(format!(
                            "{target}.remove(&{key}).unwrap_or({default})",
                            target = target_expr,
                            key = key_expr,
                            default = default_expr
                        ));
                    }
                    let pop_expr = format!(
                        "{target}.remove(&{key}).ok_or_else(|| PyError::KeyError(\"KeyError\".into()))",
                        target = target_expr,
                        key = key_expr
                    );
                    return Ok(self.wrap_result(pop_expr));
                }
                if let Some(default_expr) = default_expr {
                    return self.with_locked_attr_target(
                        value,
                        "dict mutex poisoned",
                        true,
                        |_tc, guard| {
                            format!(
                                "{guard}.remove(&{key}).unwrap_or({default})",
                                guard = guard,
                                key = key_expr,
                                default = default_expr
                            )
                        },
                    );
                }
                return self.with_locked_attr_target(
                    value,
                    "dict mutex poisoned",
                    true,
                    |tc, guard| {
                        tc.wrap_result(format!(
                            "{guard}.remove(&{key}).ok_or_else(|| PyError::KeyError(\"KeyError\".into()))",
                            guard = guard,
                            key = key_expr
                        ))
                    },
                );
            }
        }
        if attr == "clear" {
            if let Some(Type::Dict(_, _)) = value.ty.as_ref() {
                if !args.is_empty() {
                    return Err(self.error(value.span, "dict.clear() expects no arguments"));
                }
                if matches!(self.dict_storage_for_expr(value), DictStorage::Local) {
                    let target_expr = self.gen_expr(value)?;
                    return Ok(format!("{}.clear()", target_expr));
                }
                return self.with_locked_attr_target(
                    value,
                    "dict mutex poisoned",
                    true,
                    |_tc, guard| format!("{guard}.clear();", guard = guard),
                );
            }
        }
        if attr == "copy" {
            if let Some(Type::Dict(_, _)) = value.ty.as_ref() {
                if !args.is_empty() {
                    return Err(self.error(value.span, "dict.copy() expects no arguments"));
                }
                if matches!(self.dict_storage_for_expr(value), DictStorage::Local) {
                    let target_expr = self.gen_expr(value)?;
                    // HashMap::clone creates a new dict object.
                    return Ok(format!("{}.clone()", target_expr));
                }
                return self.with_locked_attr_target(
                    value,
                    "dict mutex poisoned",
                    false,
                    |_tc, guard| format!("Arc::new(Mutex::new({guard}.clone()))", guard = guard),
                );
            }
        }
        if attr == "update" {
            if let Some(Type::Dict(_, _)) = value.ty.as_ref() {
                if args.len() != 1 {
                    return Err(self.error(value.span, "dict.update() expects one argument"));
                }
                self.uses.hash_map = true;
                let arg_expr = self.gen_expr(&args[0])?;
                // Snapshot key/value pairs to avoid holding two dict borrows/locks at once.
                let pairs_tmp = self.new_tmp();
                let pairs_expr = if matches!(
                    self.dict_storage_for_expr(&args[0]),
                    DictStorage::Local
                ) {
                    format!(
                        "{arg}.iter().map(|(k, v)| (k.clone(), v.clone())).collect::<Vec<_>>()",
                        arg = arg_expr
                    )
                } else {
                    let arg_tmp = self.new_tmp();
                    let arg_guard = self.new_tmp();
                    let arg_init = if matches!(args[0].kind, ExprKind::Name(_)) {
                        format!("{}.clone()", arg_expr)
                    } else {
                        arg_expr
                    };
                    format!(
                        "{{ let {arg_tmp} = {arg_init}; let {arg_guard} = {arg_tmp}.lock().expect(\"dict mutex poisoned\"); {arg_guard}.iter().map(|(k, v)| (k.clone(), v.clone())).collect::<Vec<_>>() }}",
                        arg_tmp = arg_tmp,
                        arg_init = arg_init,
                        arg_guard = arg_guard
                    )
                };
                if matches!(self.dict_storage_for_expr(value), DictStorage::Local) {
                    let target_expr = self.gen_expr(value)?;
                    return Ok(format!(
                        "{{ let {pairs} = {pairs_expr}; {target}.extend({pairs}); }}",
                        pairs = pairs_tmp,
                        pairs_expr = pairs_expr,
                        target = target_expr
                    ));
                }
                return self.with_locked_attr_target(
                    value,
                    "dict mutex poisoned",
                    true,
                    |_tc, guard| {
                        format!(
                            "let {pairs} = {pairs_expr}; {guard}.extend({pairs});",
                            pairs = pairs_tmp,
                            pairs_expr = pairs_expr,
                            guard = guard
                        )
                    },
                );
            }
        }
        Err(self.error(
            value.span,
            format!("Internal error: unsupported dict method `{attr}`"),
        ))
    }
}
