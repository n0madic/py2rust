// Collection literal, indexing, slicing, and comprehension expressions.

use super::super::*;

impl<'a> Codegen<'a> {
    /// Lower a list literal expression.
    pub(super) fn gen_list_expr(
        &mut self,
        expr: &Expr,
        items: &[Expr],
    ) -> Result<String, CompileError> {
        let expected = match expr.ty.as_ref() {
            Some(Type::List(inner)) => Some(inner.as_ref()),
            _ => None,
        };
        if items.is_empty() {
            if let Some(Type::List(inner)) = expr.ty.as_ref() {
                if !matches!(inner.as_ref(), Type::Unknown) {
                    return Ok(format!("Vec::<{}>::new()", self.rust_type(inner)));
                }
            }
        }
        let elems: Result<Vec<String>, CompileError> = items
            .iter()
            .map(|e| self.gen_expr_with_expected(e, expected))
            .collect();
        Ok(format!("vec![{}]", elems?.join(", ")))
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
        items: &[(Expr, Expr)],
    ) -> Result<String, CompileError> {
        self.uses.hash_map = true;
        if items.is_empty() {
            return Ok("HashMap::new()".to_string());
        }
        let (expected_key, expected_val) = match expr.ty.as_ref() {
            Some(Type::Dict(k, v)) => (Some(k.as_ref()), Some(v.as_ref())),
            _ => (None, None),
        };
        let mut pairs = Vec::new();
        for (k, v) in items {
            let key_expr = self.gen_expr_with_expected(k, expected_key)?;
            let val_expr = self.gen_expr_with_expected(v, expected_val)?;
            pairs.push(format!("({}, {})", key_expr, val_expr));
        }
        Ok(format!("HashMap::from([{}])", pairs.join(", ")))
    }

    /// Lower a set literal expression.
    pub(super) fn gen_set_expr(
        &mut self,
        expr: &Expr,
        items: &[Expr],
    ) -> Result<String, CompileError> {
        self.uses.hash_set = true;
        if items.is_empty() {
            return Ok("HashSet::new()".to_string());
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
        _expr: &Expr,
        value: &Expr,
        index: &Expr,
    ) -> Result<String, CompileError> {
        let base = self.gen_expr(value)?;
        if let Some(Type::Tuple(_)) = value.ty.as_ref() {
            if let ExprKind::Literal(Literal::Int(idx)) = &index.kind {
                return Ok(format!("({}).{}", base, idx));
            }
        }
        if let Some(Type::Dict(_, _)) = value.ty.as_ref() {
            let idx = self.gen_expr(index)?;
            self.uses.py_dict_get = true;
            self.uses.hash_map = true;
            return Ok(self.wrap_result(format!("py_dict_get(&{}, &{})", base, idx)));
        }
        // Handle list indexing with negative index support.
        if matches!(value.ty.as_ref(), Some(Type::List(_))) {
            let idx_expr = self.gen_expr(index)?;
            self.uses.py_list_get = true;
            return Ok(self.wrap_result(format!("py_list_get(&{}, {})", base, idx_expr)));
        }
        let idx = self.gen_expr(index)?;
        Ok(format!("{}[{}]", base, idx))
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
            if let Some(step) = step {
                self.uses.py_list_slice_step = true;
                let step_arg = self.gen_expr(step)?;
                return Ok(self.wrap_result(format!(
                    "py_list_slice_step(&{}, {}, {}, {})",
                    base, start_arg, end_arg, step_arg
                )));
            }
            let range = self.slice_range(start, end)?;
            return Ok(format!("{}[{}].to_vec()", base, range));
        }
        Err(self.error(expr.span, "Slicing requires list or str"))
    }

    /// Lower list comprehension expressions.
    pub(super) fn gen_list_comp_expr(
        &mut self,
        elt: &Expr,
        target: &str,
        iter: &Expr,
        ifs: &[Expr],
    ) -> Result<String, CompileError> {
        let tmp = self.new_tmp();
        let mut out = String::new();
        out.push('{');
        out.push_str(&format!(" let mut {} = Vec::new();", tmp));
        out.push_str(&format!(
            " for {} in {}.into_iter() {{",
            target,
            self.gen_expr(iter)?
        ));
        if ifs.is_empty() {
            out.push_str(&format!(" {}.push({});", tmp, self.gen_expr(elt)?));
        } else {
            let conds: Result<Vec<String>, CompileError> =
                ifs.iter().map(|c| self.gen_expr(c)).collect();
            out.push_str(&format!(
                " if {} {{ {}.push({}); }}",
                conds?.join(" && "),
                tmp,
                self.gen_expr(elt)?
            ));
        }
        out.push_str(" }");
        out.push_str(&format!(" {} }}", tmp));
        Ok(out)
    }
}
