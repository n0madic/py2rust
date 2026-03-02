// Collection literal, indexing, slicing, and comprehension expressions.

use super::super::*;

/// Borrowed view of a comprehension clause used during code generation.
struct CompClauseRef<'a> {
    target: &'a str,
    /// Optional tuple target names for destructuring `for (a, b) in ...`.
    tuple_targets: Option<&'a [String]>,
    iter: &'a Expr,
    ifs: &'a [Expr],
}

impl CompClauseRef<'_> {
    /// Return the for-loop target pattern: either the plain name or a tuple pattern.
    fn target_pattern(&self) -> String {
        if let Some(names) = self.tuple_targets {
            format!("({})", names.join(", "))
        } else {
            self.target.to_string()
        }
    }
}

impl<'a> Codegen<'a> {
    /// Lower a list literal expression.
    pub(super) fn gen_list_expr(
        &mut self,
        expr: &Expr,
        items: &[Expr],
    ) -> Result<String, CompileError> {
        self.gen_list_expr_with_storage(expr, items, ListStorage::SharedCell)
    }

    /// Lower a list literal with an explicit storage strategy.
    pub(crate) fn gen_list_expr_with_storage(
        &mut self,
        expr: &Expr,
        items: &[Expr],
        storage: ListStorage,
    ) -> Result<String, CompileError> {
        let expected = match expr.ty.as_ref() {
            Some(Type::List(inner)) => Some(inner.as_ref()),
            _ => None,
        };
        if items.is_empty() {
            if let Some(Type::List(inner)) = expr.ty.as_ref() {
                if !matches!(inner.as_ref(), Type::Unknown) {
                    let base = format!("Vec::<{}>::new()", self.rust_type(inner));
                    return Ok(self.wrap_list_storage_expr(&base, storage));
                }
                // Inside comprehension push() or other inference contexts,
                // omit the type suffix so Rust can infer from usage.
                if self.infer_empty_list_type {
                    let base = "Vec::new()".to_string();
                    return Ok(self.wrap_list_storage_expr(&base, storage));
                }
                self.uses.py_repr = true;
                let base = "Vec::<PyRepr>::new()".to_string();
                return Ok(self.wrap_list_storage_expr(&base, storage));
            }
        }
        // If element types don't unify, coerce to a String-based list for Debug printing.
        if matches!(
            expr.ty.as_ref(),
            Some(Type::List(inner)) if matches!(inner.as_ref(), Type::Unknown)
        ) {
            self.uses.py_repr = true;
            let elems: Result<Vec<String>, CompileError> = items
                .iter()
                .map(|e| self.gen_unknown_list_elem(e))
                .collect();
            let base = format!("vec![{}]", elems?.join(", "));
            return Ok(self.wrap_list_storage_expr(&base, storage));
        }
        let elems: Result<Vec<String>, CompileError> = items
            .iter()
            .map(|e| self.gen_expr_with_expected(e, expected))
            .collect();
        let base = format!("vec![{}]", elems?.join(", "));
        Ok(self.wrap_list_storage_expr(&base, storage))
    }

    /// Coerce heterogeneous list elements into PyRepr for a uniform Vec<PyRepr>.
    fn gen_unknown_list_elem(&mut self, expr: &Expr) -> Result<String, CompileError> {
        if matches!(expr.ty.as_ref(), Some(Type::List(_))) {
            // Nested list reprs already include brackets; avoid Debug-quoting the string.
            let elem_expr = self.list_str_expr(expr)?;
            return Ok(format!("PyRepr({})", elem_expr));
        }
        let elem_expr = self.gen_expr(expr)?;
        Ok(format!("PyRepr(format!(\"{{:?}}\", {}))", elem_expr))
    }

    /// Coerce heterogeneous dict keys/values into PyRepr for a uniform map type.
    fn gen_unknown_dict_part(&mut self, expr: &Expr) -> Result<String, CompileError> {
        if matches!(expr.ty.as_ref(), Some(Type::List(_))) {
            // Nested list reprs already include brackets; avoid Debug-quoting the string.
            let part_expr = self.list_str_expr(expr)?;
            return Ok(format!("PyRepr({})", part_expr));
        }
        let part_expr = self.gen_expr(expr)?;
        Ok(format!("PyRepr(format!(\"{{:?}}\", {}))", part_expr))
    }

    /// Lower a tuple literal expression.
    pub(super) fn gen_tuple_expr(
        &mut self,
        expr: &Expr,
        items: &[Expr],
    ) -> Result<String, CompileError> {
        let expected_items = match expr.ty.as_ref() {
            Some(Type::Tuple(tys)) if tys.len() == items.len() => Some(tys),
            _ => None,
        };
        let mut parts = Vec::new();
        for (idx, item) in items.iter().enumerate() {
            let expected = expected_items.and_then(|tys| tys.get(idx));
            parts.push(self.gen_expr_with_expected(item, expected)?);
        }
        let joined = parts.join(", ");
        if items.len() == 1 {
            Ok(format!("({},)", joined))
        } else {
            Ok(format!("({})", joined))
        }
    }

    /// Lower a dict literal expression.
    pub(super) fn gen_dict_expr(
        &mut self,
        expr: &Expr,
        items: &[DictEntry],
    ) -> Result<String, CompileError> {
        self.gen_dict_expr_with_storage(expr, items, DictStorage::SharedCell)
    }

    /// Lower a dict literal expression with an explicit storage strategy.
    pub(crate) fn gen_dict_expr_with_storage(
        &mut self,
        expr: &Expr,
        items: &[DictEntry],
        storage: DictStorage,
    ) -> Result<String, CompileError> {
        self.uses.index_map = true;
        if items.is_empty() {
            // Empty dict literals must still be monomorphized in Rust.
            // Fall back to PyRepr storage when key/value types are unknown.
            let (key_ty, val_ty) = match expr.ty.as_ref() {
                Some(Type::Dict(key, val))
                    if !matches!(key.as_ref(), Type::Unknown)
                        && !matches!(val.as_ref(), Type::Unknown) =>
                {
                    (self.rust_type(key.as_ref()), self.rust_type(val.as_ref()))
                }
                Some(Type::Dict(key, val)) => {
                    self.uses.py_repr = true;
                    let key_ty = if matches!(key.as_ref(), Type::Unknown) {
                        "PyRepr".to_string()
                    } else {
                        self.rust_type(key.as_ref())
                    };
                    let val_ty = if matches!(val.as_ref(), Type::Unknown) {
                        "PyRepr".to_string()
                    } else {
                        self.rust_type(val.as_ref())
                    };
                    (key_ty, val_ty)
                }
                _ => {
                    self.uses.py_repr = true;
                    ("PyRepr".to_string(), "PyRepr".to_string())
                }
            };
            let base = format!("IndexMap::<{}, {}>::new()", key_ty, val_ty);
            return Ok(self.wrap_dict_storage_expr(&base, storage));
        }
        let (expected_key, expected_val) = match expr.ty.as_ref() {
            Some(Type::Dict(k, v)) => (Some(k.as_ref()), Some(v.as_ref())),
            _ => (None, None),
        };
        let unknown_key = matches!(expected_key, Some(Type::Unknown));
        let unknown_val = matches!(expected_val, Some(Type::Unknown));
        if unknown_key || unknown_val {
            self.uses.py_repr = true;
        }
        let dict_tmp = self.new_tmp();
        let mut ops = vec![format!("let mut {} = IndexMap::new()", dict_tmp)];
        for entry in items {
            match entry {
                DictEntry::Item { key, value } => {
                    let key_expr = if unknown_key {
                        self.gen_unknown_dict_part(key)?
                    } else {
                        self.gen_expr_with_expected(key, expected_key)?
                    };
                    let val_expr = if unknown_val {
                        self.gen_unknown_dict_part(value)?
                    } else {
                        self.gen_expr_with_expected(value, expected_val)?
                    };
                    ops.push(format!("{}.insert({}, {})", dict_tmp, key_expr, val_expr));
                }
                DictEntry::Unpack { value } => {
                    let src_expr = self.gen_expr(value)?;
                    let key_map_expr = if unknown_key {
                        "PyRepr(format!(\"{:?}\", k))".to_string()
                    } else {
                        "k.clone()".to_string()
                    };
                    let val_map_expr = if unknown_val {
                        "PyRepr(format!(\"{:?}\", v))".to_string()
                    } else {
                        "v.clone()".to_string()
                    };
                    if matches!(self.dict_storage_for_expr(value), DictStorage::Local) {
                        ops.push(format!(
                            "{}.extend({}.iter().map(|(k, v)| ({}, {})))",
                            dict_tmp, src_expr, key_map_expr, val_map_expr
                        ));
                    } else {
                        let src_tmp = self.new_tmp();
                        let src_guard = self.new_tmp();
                        ops.push(format!(
                            "{{ let {src_tmp} = {src_expr}; let {src_guard} = {src_tmp}.py_dict_guard(); {dict_tmp}.extend({src_guard}.iter().map(|(k, v)| ({key_map_expr}, {val_map_expr}))); }}",
                            src_tmp = src_tmp,
                            src_expr = src_expr,
                            src_guard = src_guard,
                            dict_tmp = dict_tmp,
                            key_map_expr = key_map_expr,
                            val_map_expr = val_map_expr
                        ));
                    }
                }
            }
        }
        let base = format!("{{ {}; {} }}", ops.join("; "), dict_tmp);
        Ok(self.wrap_dict_storage_expr(&base, storage))
    }

    /// Lower a set literal expression.
    pub(super) fn gen_set_expr(
        &mut self,
        expr: &Expr,
        items: &[Expr],
    ) -> Result<String, CompileError> {
        self.uses.hash_set = true;
        if items.is_empty() {
            if let Some(Type::Set(inner)) = expr.ty.as_ref() {
                if !matches!(inner.as_ref(), Type::Unknown) {
                    return Ok(format!("HashSet::<{}>::new()", self.rust_type(inner)));
                }
            }
            // CPython-compat divergence:
            // Empty set literals without contextual typing are emitted as
            // `HashSet<PyRepr>` so generated Rust remains concrete.
            self.uses.py_repr = true;
            return Ok("HashSet::<PyRepr>::new()".to_string());
        }
        let expected = match expr.ty.as_ref() {
            Some(Type::Set(inner)) => Some(inner.as_ref()),
            _ => None,
        };
        let mut elems = Vec::new();
        for item in items {
            elems.push(self.gen_expr_with_expected(item, expected)?);
        }
        Ok(format!("HashSet::from([{}])", elems.join(", ")))
    }

    /// Lower indexing expressions, including tuple, dict, and list special cases.
    pub(super) fn gen_index_expr(
        &mut self,
        expr: &Expr,
        value: &Expr,
        index: &Expr,
    ) -> Result<String, CompileError> {
        let base = self.gen_expr(value)?;
        let is_local_name = matches!(&value.kind, ExprKind::Name(name) if !self.is_global(name));
        if let Some(Type::Option(inner)) = value.ty.as_ref() {
            // Optional container indexing follows Python behavior:
            // runtime unwrap is required and may fail if value is None.
            if let Type::Tuple(items) = inner.as_ref() {
                let idx_opt = match &index.kind {
                    ExprKind::Literal(Literal::Int(idx)) => Some(*idx),
                    ExprKind::Unary {
                        op: UnaryOp::Neg,
                        expr: inner,
                    } => {
                        if let ExprKind::Literal(Literal::Int(idx)) = &inner.as_ref().kind {
                            Some(-idx)
                        } else {
                            None
                        }
                    }
                    _ => None,
                };
                if let Some(idx) = idx_opt {
                    let len_i = items.len() as i64;
                    let mut adj = idx;
                    if adj < 0 {
                        adj += len_i;
                    }
                    if adj >= 0 && adj < len_i {
                        let tmp = self.new_tmp();
                        if is_local_name {
                            return Ok(format!(
                                "({base}.as_ref().expect(\"optional value is None\")).{adj}",
                                base = base,
                                adj = adj
                            ));
                        }
                        return Ok(format!(
                            "{{ let {tmp} = {base}; ({tmp}.as_ref().expect(\"optional value is None\")).{adj} }}",
                            tmp = tmp,
                            base = base,
                            adj = adj
                        ));
                    }
                    return Err(self.error(expr.span, "Tuple index out of bounds"));
                }
            }
            if matches!(inner.as_ref(), Type::Dict(_, _)) {
                let idx = self.gen_expr(index)?;
                self.uses.py_dict_get = true;
                self.uses.index_map = true;
                let tmp = self.new_tmp();
                if matches!(self.dict_storage_for_expr(value), DictStorage::Local) {
                    if is_local_name {
                        return Ok(self.wrap_result(format!(
                            "{{ let dict_ref = {base}.as_ref().expect(\"optional value is None\"); py_dict_get(dict_ref, &{idx}) }}",
                            base = base,
                            idx = idx
                        )));
                    }
                    return Ok(self.wrap_result(format!(
                        "{{ let {tmp} = {base}; let dict_ref = {tmp}.as_ref().expect(\"optional value is None\"); py_dict_get(dict_ref, &{idx}) }}",
                        tmp = tmp,
                        base = base,
                        idx = idx
                    )));
                }
                let guard = self.new_tmp();
                if is_local_name {
                    return Ok(self.wrap_result(format!(
                        "{{ let dict_ref = {base}.as_ref().expect(\"optional value is None\"); let {guard} = dict_ref.py_dict_guard(); py_dict_get(&{guard}, &{idx}) }}",
                        base = base,
                        guard = guard,
                        idx = idx
                    )));
                }
                return Ok(self.wrap_result(format!(
                    "{{ let {tmp} = {base}; let dict_ref = {tmp}.as_ref().expect(\"optional value is None\"); let {guard} = dict_ref.py_dict_guard(); py_dict_get(&{guard}, &{idx}) }}",
                    tmp = tmp,
                    base = base,
                    guard = guard,
                    idx = idx
                )));
            }
            if matches!(inner.as_ref(), Type::List(_) | Type::Bytes) {
                let idx_expr = self.gen_expr(index)?;
                self.uses.py_list_get = true;
                let tmp = self.new_tmp();
                if matches!(inner.as_ref(), Type::List(_)) {
                    if matches!(self.list_storage_for_expr(value), ListStorage::Local) {
                        if is_local_name {
                            return Ok(self.wrap_result(format!(
                                "{{ let list_ref = {base}.as_ref().expect(\"optional value is None\"); py_list_get(list_ref, {idx}) }}",
                                base = base,
                                idx = idx_expr
                            )));
                        }
                        return Ok(self.wrap_result(format!(
                            "{{ let {tmp} = {base}; let list_ref = {tmp}.as_ref().expect(\"optional value is None\"); py_list_get(list_ref, {idx}) }}",
                            tmp = tmp,
                            base = base,
                            idx = idx_expr
                        )));
                    }
                    let guard = self.new_tmp();
                    let idx_tmp = self.new_tmp();
                    if is_local_name {
                        return Ok(self.wrap_result(format!(
                            "{{ let {idx_tmp} = {idx}; let list_ref = {base}.as_ref().expect(\"optional value is None\"); let {guard} = list_ref.py_list_guard(); py_list_get(&{guard}, {idx_tmp}) }}",
                            idx_tmp = idx_tmp,
                            idx = idx_expr,
                            base = base,
                            guard = guard,
                        )));
                    }
                    return Ok(self.wrap_result(format!(
                        "{{ let {tmp} = {base}; let {idx_tmp} = {idx}; let list_ref = {tmp}.as_ref().expect(\"optional value is None\"); let {guard} = list_ref.py_list_guard(); py_list_get(&{guard}, {idx_tmp}) }}",
                        tmp = tmp,
                        base = base,
                        idx_tmp = idx_tmp,
                        idx = idx_expr,
                        guard = guard,
                    )));
                }
                if is_local_name {
                    return Ok(self.wrap_result(format!(
                        "{{ let bytes_ref = {base}.as_ref().expect(\"optional value is None\"); py_list_get(bytes_ref, {idx}) }}",
                        base = base,
                        idx = idx_expr
                    )));
                }
                return Ok(self.wrap_result(format!(
                    "{{ let {tmp} = {base}; let bytes_ref = {tmp}.as_ref().expect(\"optional value is None\"); py_list_get(bytes_ref, {idx}) }}",
                    tmp = tmp,
                    base = base,
                    idx = idx_expr
                )));
            }
            if matches!(inner.as_ref(), Type::Str) {
                let idx_expr = self.gen_expr(index)?;
                self.uses.py_str_get = true;
                let tmp = self.new_tmp();
                if is_local_name {
                    return Ok(self.wrap_result(format!(
                        "{{ let str_ref = {base}.as_ref().expect(\"optional value is None\"); py_str_get(str_ref, {idx}) }}",
                        base = base,
                        idx = idx_expr
                    )));
                }
                return Ok(self.wrap_result(format!(
                    "{{ let {tmp} = {base}; let str_ref = {tmp}.as_ref().expect(\"optional value is None\"); py_str_get(str_ref, {idx}) }}",
                    tmp = tmp,
                    base = base,
                    idx = idx_expr
                )));
            }
        }
        if let Some(Type::Tuple(items)) = value.ty.as_ref() {
            let idx_opt = match &index.kind {
                ExprKind::Literal(Literal::Int(idx)) => Some(*idx),
                ExprKind::Unary {
                    op: UnaryOp::Neg,
                    expr: inner,
                } => {
                    if let ExprKind::Literal(Literal::Int(idx)) = &inner.as_ref().kind {
                        Some(-idx)
                    } else {
                        None
                    }
                }
                _ => None,
            };
            if let Some(idx) = idx_opt {
                let len_i = items.len() as i64;
                let mut adj = idx;
                if adj < 0 {
                    adj += len_i;
                }
                if adj >= 0 && adj < len_i {
                    return Ok(format!("({}).{}", base, adj));
                }
                return Err(self.error(expr.span, "Tuple index out of bounds"));
            }
        }
        if let Some(Type::Dict(_, _)) = value.ty.as_ref() {
            let idx = self.gen_expr(index)?;
            self.uses.py_dict_get = true;
            self.uses.index_map = true;
            if matches!(self.dict_storage_for_expr(value), DictStorage::Local) {
                return Ok(self.wrap_result(format!("py_dict_get(&{}, &{})", base, idx)));
            }
            let guard = self.new_tmp();
            let dict_tmp = self.new_tmp();
            if is_local_name {
                return Ok(self.wrap_result(format!(
                    "{{ let {guard} = {base}.py_dict_guard(); py_dict_get(&{guard}, &{idx}) }}",
                    guard = guard,
                    base = base,
                    idx = idx
                )));
            }
            return Ok(self.wrap_result(format!(
                "{{ let {dict_tmp} = {base}; let {guard} = {dict_tmp}.py_dict_guard(); py_dict_get(&{guard}, &{idx}) }}",
                dict_tmp = dict_tmp,
                guard = guard,
                base = base,
                idx = idx
            )));
        }
        // Handle list indexing with negative index support.
        if matches!(value.ty.as_ref(), Some(Type::List(_)) | Some(Type::Bytes)) {
            let idx_expr = self.gen_expr(index)?;
            self.uses.py_list_get = true;
            if matches!(value.ty.as_ref(), Some(Type::List(_))) {
                if matches!(self.list_storage_for_expr(value), ListStorage::Local) {
                    return Ok(
                        self.wrap_result(format!("py_list_get(&{base}, {idx})", idx = idx_expr))
                    );
                }
                // Non-local (Arc<Mutex>) storage: pre-compute the index before acquiring the
                // guard to prevent deadlock when the index expression calls py_len on the same
                // list (e.g. `lst[i % len(lst)]` — py_len would re-lock the same mutex).
                let idx_tmp = self.new_tmp();
                return Ok(self.wrap_result(format!(
                    "{{ let {idx_tmp} = {idx}; py_list_get(&{base}.py_list_guard(), {idx_tmp}) }}",
                    idx_tmp = idx_tmp,
                    idx = idx_expr,
                    base = base
                )));
            }
            return Ok(self.wrap_result(format!("py_list_get(&{}, {})", base, idx_expr)));
        }
        if matches!(value.ty.as_ref(), Some(Type::Str)) {
            let idx_expr = self.gen_expr(index)?;
            self.uses.py_str_get = true;
            return Ok(self.wrap_result(format!(
                "py_str_get(&{base}, {idx})",
                base = base,
                idx = idx_expr
            )));
        }
        let idx = self.gen_expr(index)?;
        Ok(format!("{}[{}]", base, idx))
    }

    /// Lower list slicing with a requested output storage strategy.
    pub(crate) fn gen_list_slice_expr_with_storage(
        &mut self,
        value: &Expr,
        start: Option<&Expr>,
        end: Option<&Expr>,
        step: Option<&Expr>,
        storage: ListStorage,
    ) -> Result<String, CompileError> {
        let base = self.gen_expr(value)?;
        let start_arg = match start {
            Some(s) => format!("Some({})", self.gen_expr(s)?),
            None => "None".to_string(),
        };
        let end_arg = match end {
            Some(e) => format!("Some({})", self.gen_expr(e)?),
            None => "None".to_string(),
        };
        self.uses.py_list_slice_step = true;
        let call = if let Some(step_expr) = step {
            let step_arg = self.gen_expr(step_expr)?;
            let list_ref = if matches!(self.list_storage_for_expr(value), ListStorage::Local) {
                format!("&{}", base)
            } else {
                format!("&{}.py_list_guard()", base)
            };
            self.wrap_result(format!(
                "py_list_slice_step({}, {}, {}, {})",
                list_ref, start_arg, end_arg, step_arg
            ))
        } else {
            let list_ref = if matches!(self.list_storage_for_expr(value), ListStorage::Local) {
                format!("&{}", base)
            } else {
                format!("&{}.py_list_guard()", base)
            };
            self.wrap_result(format!(
                "py_list_slice_step({}, {}, {}, 1i64)",
                list_ref, start_arg, end_arg
            ))
        };
        Ok(self.wrap_list_storage_expr(&call, storage))
    }

    /// Lower slicing expressions for lists and strings.
    pub(super) fn gen_slice_expr(
        &mut self,
        expr: &Expr,
        value: &Expr,
        start: Option<&Expr>,
        end: Option<&Expr>,
        step: Option<&Expr>,
    ) -> Result<String, CompileError> {
        let base = self.gen_expr(value)?;
        if let Some(Type::Tuple(items)) = value.ty.as_ref() {
            // Tuple slicing is only supported for literal bounds.
            let lit_int = |expr: &Expr| -> Option<i64> {
                match &expr.kind {
                    ExprKind::Literal(Literal::Int(idx)) => Some(*idx),
                    ExprKind::Unary {
                        op: UnaryOp::Neg,
                        expr,
                    } => {
                        if let ExprKind::Literal(Literal::Int(idx)) = &expr.as_ref().kind {
                            Some(-idx)
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            };
            let start_lit = start.and_then(lit_int);
            let end_lit = end.and_then(lit_int);
            let step_lit = match step {
                Some(st) => lit_int(st).ok_or_else(|| {
                    self.error(expr.span, "Tuple slicing requires literal bounds")
                })?,
                None => 1,
            };
            if step_lit == 0 {
                return Err(self.error(expr.span, "Slice step cannot be zero"));
            }
            if (start.is_some() && start_lit.is_none()) || (end.is_some() && end_lit.is_none()) {
                return Err(self.error(expr.span, "Tuple slicing requires literal bounds"));
            }

            let len = items.len() as i64;
            let mut indices = Vec::new();
            if step_lit > 0 {
                let mut i = match start_lit {
                    Some(s) => {
                        let s = if s < 0 { len + s } else { s };
                        s.max(0).min(len)
                    }
                    None => 0,
                };
                let end_i = match end_lit {
                    Some(e) => {
                        let e = if e < 0 { len + e } else { e };
                        e.max(0).min(len)
                    }
                    None => len,
                };
                while i < end_i {
                    if i >= 0 && i < len {
                        indices.push(i as usize);
                    }
                    i += step_lit;
                }
            } else {
                let mut i = match start_lit {
                    Some(s) => {
                        let s = if s < 0 { len + s } else { s };
                        if s < 0 {
                            -1
                        } else if s >= len {
                            len - 1
                        } else {
                            s
                        }
                    }
                    None => len - 1,
                };
                let end_i = match end_lit {
                    Some(e) => {
                        let e = if e < 0 { len + e } else { e };
                        if e < 0 {
                            -1
                        } else if e >= len {
                            len - 1
                        } else {
                            e
                        }
                    }
                    None => -1,
                };
                while i > end_i {
                    if i >= 0 && i < len {
                        indices.push(i as usize);
                    }
                    i += step_lit;
                }
            }

            let needs_tmp = match &value.kind {
                ExprKind::Name(name) => self.is_global(name),
                _ => true,
            };
            // Global tuple reads must be cached to avoid multiple mutex locks in one statement.
            if needs_tmp {
                let tmp = self.new_tmp();
                let tuple = self.build_tuple_from_indices(&tmp, &indices);
                return Ok(format!("{{ let {} = {}; {} }}", tmp, base, tuple));
            }
            return Ok(self.build_tuple_from_indices(&base, &indices));
        }
        let start_arg = match start {
            Some(s) => format!("Some({})", self.gen_expr(s)?),
            None => "None".to_string(),
        };
        let end_arg = match end {
            Some(e) => format!("Some({})", self.gen_expr(e)?),
            None => "None".to_string(),
        };
        if matches!(value.ty.as_ref(), Some(Type::Str)) {
            // Use character-based slicing for Python string semantics.
            if let Some(step) = step {
                self.uses.py_str_slice_step = true;
                let step_arg = self.gen_expr(step)?;
                return Ok(self.wrap_result(format!(
                    "py_str_slice_step(&{}, {}, {}, {})",
                    base, start_arg, end_arg, step_arg
                )));
            }
            self.uses.py_str_slice = true;
            return Ok(format!(
                "py_str_slice(&{}, {}, {})",
                base, start_arg, end_arg
            ));
        }
        if matches!(value.ty.as_ref(), Some(Type::List(_))) {
            return self.gen_list_slice_expr_with_storage(
                value,
                start,
                end,
                step,
                ListStorage::SharedCell,
            );
        }
        if matches!(value.ty.as_ref(), Some(Type::Bytes)) {
            // Use the step helper with step=1 to handle negative bounds consistently.
            self.uses.py_list_slice_step = true;
            let call = if let Some(step) = step {
                let step_arg = self.gen_expr(step)?;
                self.wrap_result(format!(
                    "py_list_slice_step(&{}, {}, {}, {})",
                    base, start_arg, end_arg, step_arg
                ))
            } else {
                self.wrap_result(format!(
                    "py_list_slice_step(&{}, {}, {}, 1i64)",
                    base, start_arg, end_arg
                ))
            };
            return Ok(call);
        }
        // Gradual typing fallback: treat Unknown as a Vec and generate list slice.
        if matches!(value.ty.as_ref(), Some(Type::Unknown) | None) {
            let start_arg = match start {
                Some(s) => self.gen_expr(s)?,
                None => "0i64".to_string(),
            };
            let end_arg = match end {
                Some(e) => self.gen_expr(e)?,
                None => format!("{}.len() as i64", base),
            };
            if let Some(step) = step {
                let step_arg = self.gen_expr(step)?;
                return Ok(self.wrap_result(format!(
                    "py_list_slice_step(&{}, {}, {}, {})",
                    base, start_arg, end_arg, step_arg
                )));
            }
            self.uses.py_list_slice_step = true;
            return Ok(self.wrap_result(format!(
                "py_list_slice_step(&{}, Some({}), Some({}), 1i64)",
                base, start_arg, end_arg
            )));
        }
        Err(self.error(expr.span, "Slicing requires list or str"))
    }

    /// Build a tuple literal from selected indices of a tuple expression.
    fn build_tuple_from_indices(&self, base: &str, indices: &[usize]) -> String {
        if indices.is_empty() {
            return "()".to_string();
        }
        if indices.len() == 1 {
            return format!("({}.{},)", base, indices[0]);
        }
        let parts: Vec<String> = indices.iter().map(|i| format!("{}.{}", base, i)).collect();
        format!("({})", parts.join(", "))
    }

    /// Lower list comprehension expressions.
    pub(super) fn gen_list_comp_expr(
        &mut self,
        elt: &Expr,
        target: &str,
        iter: &Expr,
        ifs: &[Expr],
        generators: &[CompClause],
    ) -> Result<String, CompileError> {
        self.gen_list_comp_expr_with_storage(
            elt,
            target,
            iter,
            ifs,
            generators,
            ListStorage::SharedCell,
        )
    }

    /// Lower list comprehension expressions with explicit storage.
    pub(crate) fn gen_list_comp_expr_with_storage(
        &mut self,
        elt: &Expr,
        target: &str,
        iter: &Expr,
        ifs: &[Expr],
        generators: &[CompClause],
        storage: ListStorage,
    ) -> Result<String, CompileError> {
        let tmp = self.new_tmp();
        let clauses = Self::comp_clause_refs(target, iter, ifs, generators);
        let mut out = String::new();
        out.push('{');
        out.push_str(&format!(" let mut {} = Vec::new();", tmp));
        self.emit_list_comp_loops(&mut out, &clauses, 0, elt, &tmp)?;
        out.push_str(&format!(
            " {} }}",
            self.wrap_list_storage_expr(&tmp, storage)
        ));
        Ok(out)
    }

    /// Lower set comprehension expressions.
    pub(super) fn gen_set_comp_expr(
        &mut self,
        elt: &Expr,
        target: &str,
        iter: &Expr,
        ifs: &[Expr],
        generators: &[CompClause],
    ) -> Result<String, CompileError> {
        self.uses.hash_set = true;
        let tmp = self.new_tmp();
        let clauses = Self::comp_clause_refs(target, iter, ifs, generators);
        let mut out = String::new();
        out.push('{');
        out.push_str(&format!(" let mut {} = HashSet::new();", tmp));
        self.emit_set_comp_loops(&mut out, &clauses, 0, elt, &tmp)?;
        out.push_str(&format!(" {} }}", tmp));
        Ok(out)
    }

    /// Return normalized comprehension clauses, falling back to first-clause fields.
    fn comp_clause_refs<'b>(
        target: &'b str,
        iter: &'b Expr,
        ifs: &'b [Expr],
        generators: &'b [CompClause],
    ) -> Vec<CompClauseRef<'b>> {
        if generators.is_empty() {
            return vec![CompClauseRef {
                target,
                tuple_targets: None,
                iter,
                ifs,
            }];
        }
        generators
            .iter()
            .map(|clause| CompClauseRef {
                target: &clause.target,
                tuple_targets: clause.tuple_targets.as_deref(),
                iter: &clause.iter,
                ifs: &clause.ifs,
            })
            .collect()
    }

    /// Emit nested `for` loops for list comprehension clauses.
    fn emit_list_comp_loops(
        &mut self,
        out: &mut String,
        clauses: &[CompClauseRef<'_>],
        idx: usize,
        elt: &Expr,
        tmp: &str,
    ) -> Result<(), CompileError> {
        let clause = clauses
            .get(idx)
            .ok_or_else(|| self.error(elt.span, "Comprehension has no generators"))?;
        let iter_src = self.gen_iter_source(clause.iter)?;
        // Keep lock guards alive for the loop body.
        for line in &iter_src.setup {
            out.push_str(&format!(" {};", line));
        }
        let target_pat = clause.target_pattern();
        out.push_str(&format!(" for {} in {} {{", target_pat, iter_src.expr));

        // Treat each generator target as a comprehension-local binding.
        let saved_locals = self.local_vars.clone();
        let mut scoped_locals = saved_locals.clone().unwrap_or_default();
        let item_ty = clause
            .iter
            .ty
            .as_ref()
            .and_then(|ty| self.iter_item_type_hint(ty))
            .unwrap_or(Type::Unknown);
        // Register individual tuple target names if present, otherwise the combined name.
        if let Some(names) = clause.tuple_targets {
            if let Type::Tuple(items) = &item_ty {
                for (i, name) in names.iter().enumerate() {
                    let ty = items.get(i).cloned().unwrap_or(Type::Unknown);
                    scoped_locals.insert(name.clone(), ty);
                }
            } else {
                for name in names {
                    scoped_locals.insert(name.clone(), Type::Unknown);
                }
            }
        } else {
            scoped_locals.insert(clause.target.to_string(), item_ty);
        }
        self.local_vars = Some(scoped_locals);

        let body_result = (|| -> Result<(), CompileError> {
            let cond_expr = if clause.ifs.is_empty() {
                None
            } else {
                let conds: Result<Vec<String>, CompileError> = clause
                    .ifs
                    .iter()
                    .map(|c| self.gen_condition_expr(c))
                    .collect();
                Some(conds?.join(" && "))
            };

            if let Some(cond) = &cond_expr {
                out.push_str(&format!(" if {} {{", cond));
            }

            if idx + 1 < clauses.len() {
                self.emit_list_comp_loops(out, clauses, idx + 1, elt, tmp)?;
            } else {
                // Inside push(), Rust can infer empty list element types from the
                // outer collection's type, so omit explicit PyRepr annotations.
                let saved = self.infer_empty_list_type;
                self.infer_empty_list_type = true;
                let elt_expr = self.gen_expr(elt)?;
                self.infer_empty_list_type = saved;
                out.push_str(&format!(" {}.push({});", tmp, elt_expr));
            }

            if cond_expr.is_some() {
                out.push_str(" }");
            }
            Ok(())
        })();

        self.local_vars = saved_locals;
        body_result?;
        out.push_str(" }");
        Ok(())
    }

    /// Emit nested `for` loops for set comprehension clauses.
    fn emit_set_comp_loops(
        &mut self,
        out: &mut String,
        clauses: &[CompClauseRef<'_>],
        idx: usize,
        elt: &Expr,
        tmp: &str,
    ) -> Result<(), CompileError> {
        let clause = clauses
            .get(idx)
            .ok_or_else(|| self.error(elt.span, "Comprehension has no generators"))?;
        let iter_src = self.gen_iter_source(clause.iter)?;
        // Keep lock guards alive for the loop body.
        for line in &iter_src.setup {
            out.push_str(&format!(" {};", line));
        }
        let target_pat = clause.target_pattern();
        out.push_str(&format!(" for {} in {} {{", target_pat, iter_src.expr));

        // Treat each generator target as a comprehension-local binding.
        let saved_locals = self.local_vars.clone();
        let mut scoped_locals = saved_locals.clone().unwrap_or_default();
        let item_ty = clause
            .iter
            .ty
            .as_ref()
            .and_then(|ty| self.iter_item_type_hint(ty))
            .unwrap_or(Type::Unknown);
        // Register individual tuple target names if present, otherwise the combined name.
        if let Some(names) = clause.tuple_targets {
            if let Type::Tuple(items) = &item_ty {
                for (i, name) in names.iter().enumerate() {
                    let ty = items.get(i).cloned().unwrap_or(Type::Unknown);
                    scoped_locals.insert(name.clone(), ty);
                }
            } else {
                for name in names {
                    scoped_locals.insert(name.clone(), Type::Unknown);
                }
            }
        } else {
            scoped_locals.insert(clause.target.to_string(), item_ty);
        }
        self.local_vars = Some(scoped_locals);

        let body_result = (|| -> Result<(), CompileError> {
            let cond_expr = if clause.ifs.is_empty() {
                None
            } else {
                let conds: Result<Vec<String>, CompileError> = clause
                    .ifs
                    .iter()
                    .map(|c| self.gen_condition_expr(c))
                    .collect();
                Some(conds?.join(" && "))
            };

            if let Some(cond) = &cond_expr {
                out.push_str(&format!(" if {} {{", cond));
            }

            if idx + 1 < clauses.len() {
                self.emit_set_comp_loops(out, clauses, idx + 1, elt, tmp)?;
            } else {
                let elt_expr = self.gen_expr(elt)?;
                out.push_str(&format!(" {}.insert({});", tmp, elt_expr));
            }

            if cond_expr.is_some() {
                out.push_str(" }");
            }
            Ok(())
        })();

        self.local_vars = saved_locals;
        body_result?;
        out.push_str(" }");
        Ok(())
    }
}
