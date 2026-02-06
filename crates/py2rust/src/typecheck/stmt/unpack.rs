use super::*;

impl<'a> TypeChecker<'a> {
    /// Validate and register targets for tuple/list destructuring assignments.
    ///
    /// This mirrors normal assignment checks but walks each leaf in the pattern,
    /// ensuring types line up and creating new bindings when needed.
    pub(super) fn check_unpack_target(
        &mut self,
        target: &mut AssignTarget,
        value_ty: &Type,
        value_expr: Option<&Expr>,
        span: Span,
    ) -> Result<(), CompileError> {
        match target {
            AssignTarget::Name(name) => {
                if name == "__name__" {
                    return Err(self.error(span, "Assignment to __name__ is not supported"));
                }
                if self.in_function() && self.is_declared_nonlocal(name) {
                    let outer_ty = self
                        .lookup_nonlocal_var(name)
                        .ok_or_else(|| self.error(span, "nonlocal binding not found"))?;
                    if matches!(value_ty, Type::Unknown) && !matches!(outer_ty, Type::Unknown) {
                        return Err(self.error(span, "Unable to infer type; add annotation"));
                    }
                    self.ensure_assignable(value_ty, &outer_ty, span)?;
                } else if self.in_function() && self.is_declared_global(name) {
                    let global_ty = self.ctx.globals.get(name).cloned().ok_or_else(|| {
                        self.error(
                            span,
                            format!("global `{name}` is not defined at module scope"),
                        )
                    })?;
                    if matches!(value_ty, Type::Unknown) && !matches!(global_ty, Type::Unknown) {
                        return Err(self.error(span, "Unable to infer type; add annotation"));
                    }
                    self.ensure_assignable(value_ty, &global_ty, span)?;
                } else if let Some(existing) = self.lookup_var(name) {
                    self.ensure_assignable(value_ty, &existing, span)?;
                } else {
                    if matches!(value_ty, Type::Unknown) {
                        return Err(self.error(span, "Unable to infer type; add annotation"));
                    }
                    self.insert_var(name, value_ty.clone(), span)?;
                }
                // Preserve top-level lambda inference when unpacking literal tuples/lists.
                if !self.in_function()
                    && value_expr.is_some_and(|expr| matches!(expr.kind, ExprKind::Lambda { .. }))
                {
                    if let Some(expr) = value_expr {
                        self.lambda_defs.insert(name.clone(), expr.clone());
                    }
                }
            }
            AssignTarget::Attr { value: obj, attr } => {
                let obj_ty = self.check_expr(obj, None)?;
                if let ExprKind::Name(name) = &obj.kind {
                    if let Some(class_info) = self.ctx.classes.get(name) {
                        if let Some(attr_info) = class_info.class_attrs.get(attr) {
                            self.ensure_assignable(value_ty, &attr_info.ty, span)?;
                            return Ok(());
                        }
                    }
                }
                if let Type::Custom(class_name) = obj_ty {
                    let class_info =
                        self.ctx.classes.get(&class_name).ok_or_else(|| {
                            self.error(span, format!("Unknown class: {class_name}"))
                        })?;
                    if let Some(prop) = class_info.properties.get(attr) {
                        if let Some(setter_name) = &prop.setter {
                            if let Some(sig) = class_info.methods.get(setter_name) {
                                if sig.params.len() >= 2 {
                                    let expected = sig.params[1].clone();
                                    self.ensure_assignable(value_ty, &expected, span)?;
                                }
                                return Ok(());
                            }
                        }
                        return Err(
                            self.error(span, format!("Property {class_name}.{attr} has no setter"))
                        );
                    }
                    let field_ty = class_info.fields.get(attr).ok_or_else(|| {
                        self.error(span, format!("Unknown field {class_name}.{attr}"))
                    })?;
                    self.ensure_assignable(value_ty, field_ty, span)?;
                } else {
                    return Err(
                        self.error(span, "Attribute assignment only allowed on class instances")
                    );
                }
            }
            AssignTarget::Index {
                value: container,
                index,
            } => {
                let container_ty = self.check_expr(container, None)?;
                let index_ty = self.check_expr(index, None)?;
                match container_ty {
                    Type::List(inner) => {
                        self.ensure_assignable(&index_ty, &Type::Int, span)?;
                        self.ensure_assignable(value_ty, &inner, span)?;
                    }
                    Type::Dict(key_ty, val_ty) => {
                        self.ensure_assignable(&index_ty, &key_ty, span)?;
                        self.ensure_assignable(value_ty, &val_ty, span)?;
                    }
                    _ => {
                        return Err(self.error(span, "Index assignment requires list or dict"));
                    }
                }
            }
            AssignTarget::Tuple(items) | AssignTarget::List(items) => {
                let starred_indices: Vec<usize> = items
                    .iter()
                    .enumerate()
                    .filter_map(|(idx, item)| {
                        if matches!(item, AssignTarget::Starred(_)) {
                            Some(idx)
                        } else {
                            None
                        }
                    })
                    .collect();
                if starred_indices.len() > 1 {
                    return Err(self.error(span, "Only one starred assignment target is allowed"));
                }
                if let Some(star_idx) = starred_indices.first().copied() {
                    let prefix_len = star_idx;
                    let suffix_len = items.len().saturating_sub(star_idx + 1);
                    match value_ty {
                        Type::Tuple(values) => {
                            if values.len() < (prefix_len + suffix_len) {
                                return Err(self.error(
                                    span,
                                    format!(
                                        "Unpacking expected at least {} values, got {}",
                                        prefix_len + suffix_len,
                                        values.len()
                                    ),
                                ));
                            }
                            for idx in 0..prefix_len {
                                let elem_expr = self.unpack_expr_at(value_expr, idx);
                                self.check_unpack_target(
                                    &mut items[idx],
                                    &values[idx],
                                    elem_expr,
                                    span,
                                )?;
                            }

                            let middle_start = prefix_len;
                            let middle_end = values.len() - suffix_len;
                            let middle_ty =
                                self.merge_many_types(&values[middle_start..middle_end]);
                            let starred_ty = Type::List(Box::new(middle_ty));
                            if let AssignTarget::Starred(inner) = &mut items[star_idx] {
                                self.check_unpack_target(inner, &starred_ty, None, span)?;
                            } else {
                                unreachable!("starred index must point to AssignTarget::Starred");
                            }

                            for offset in 0..suffix_len {
                                let idx = values.len() - suffix_len + offset;
                                let elem_expr = self.unpack_expr_at(value_expr, idx);
                                self.check_unpack_target(
                                    &mut items[star_idx + 1 + offset],
                                    &values[idx],
                                    elem_expr,
                                    span,
                                )?;
                            }
                        }
                        Type::List(inner) => {
                            if let Some(len) = self.unpack_expr_len(value_expr) {
                                if len < (prefix_len + suffix_len) {
                                    return Err(self.error(
                                        span,
                                        format!(
                                            "Unpacking expected at least {} values, got {}",
                                            prefix_len + suffix_len,
                                            len
                                        ),
                                    ));
                                }
                            }
                            for (idx, item_target) in items.iter_mut().take(prefix_len).enumerate()
                            {
                                let elem_expr = self.unpack_expr_at(value_expr, idx);
                                self.check_unpack_target(
                                    item_target,
                                    inner.as_ref(),
                                    elem_expr,
                                    span,
                                )?;
                            }
                            if let AssignTarget::Starred(inner_target) = &mut items[star_idx] {
                                self.check_unpack_target(
                                    inner_target,
                                    &Type::List(inner.clone()),
                                    None,
                                    span,
                                )?;
                            } else {
                                unreachable!("starred index must point to AssignTarget::Starred");
                            }
                            for offset in 0..suffix_len {
                                let elem_expr =
                                    self.unpack_expr_at_from_end(value_expr, suffix_len - offset);
                                self.check_unpack_target(
                                    &mut items[star_idx + 1 + offset],
                                    inner.as_ref(),
                                    elem_expr,
                                    span,
                                )?;
                            }
                        }
                        Type::Unknown => {
                            return Err(self.error(span, "Unable to infer type; add annotation"));
                        }
                        _ => {
                            return Err(self.error(
                                span,
                                "Unpacking assignment requires a tuple or list value",
                            ));
                        }
                    }
                } else {
                    // Unpack element types from the RHS and recurse into each element target.
                    let element_types = self.unpack_element_types(value_ty, items.len(), span)?;
                    let element_exprs = self.unpack_element_exprs(value_expr, items.len());
                    for ((item, elem_ty), elem_expr) in
                        items.iter_mut().zip(element_types).zip(element_exprs)
                    {
                        self.check_unpack_target(item, &elem_ty, elem_expr, span)?;
                    }
                }
            }
            AssignTarget::Starred(inner) => {
                if !matches!(value_ty, Type::List(_)) {
                    return Err(self.error(span, "Starred assignment target expects a list value"));
                }
                self.check_unpack_target(inner, value_ty, value_expr, span)?;
            }
        }
        Ok(())
    }

    /// Merge a sequence of types into one element type for starred unpacking.
    fn merge_many_types(&self, items: &[Type]) -> Type {
        if items.is_empty() {
            return Type::Unknown;
        }
        let mut acc = items[0].clone();
        for ty in &items[1..] {
            acc = Self::merge_types(acc, ty.clone());
        }
        acc
    }

    /// Read unpack source length when it's a literal tuple/list expression.
    fn unpack_expr_len(&self, value_expr: Option<&Expr>) -> Option<usize> {
        let expr = value_expr?;
        match &expr.kind {
            ExprKind::Tuple(items) | ExprKind::List(items) => Some(items.len()),
            _ => None,
        }
    }

    /// Extract unpack source element by absolute index for literal tuple/list RHS.
    fn unpack_expr_at<'b>(&self, value_expr: Option<&'b Expr>, idx: usize) -> Option<&'b Expr> {
        let expr = value_expr?;
        match &expr.kind {
            ExprKind::Tuple(items) | ExprKind::List(items) => items.get(idx),
            _ => None,
        }
    }

    /// Extract unpack source element by 1-based index from end for literal tuple/list RHS.
    fn unpack_expr_at_from_end<'b>(
        &self,
        value_expr: Option<&'b Expr>,
        from_end: usize,
    ) -> Option<&'b Expr> {
        let expr = value_expr?;
        match &expr.kind {
            ExprKind::Tuple(items) | ExprKind::List(items) => {
                if from_end == 0 || from_end > items.len() {
                    None
                } else {
                    items.get(items.len() - from_end)
                }
            }
            _ => None,
        }
    }

    /// Compute the element types for tuple/list unpacking.
    fn unpack_element_types(
        &self,
        value_ty: &Type,
        count: usize,
        span: Span,
    ) -> Result<Vec<Type>, CompileError> {
        match value_ty {
            Type::Tuple(items) => {
                if items.len() != count {
                    return Err(self.error(
                        span,
                        format!("Unpacking expected {count} values, got {}", items.len()),
                    ));
                }
                Ok(items.clone())
            }
            Type::List(inner) => Ok(vec![inner.as_ref().clone(); count]),
            Type::Unknown => Err(self.error(span, "Unable to infer type; add annotation")),
            _ => Err(self.error(span, "Unpacking assignment requires a tuple or list value")),
        }
    }

    /// Extract element expressions when unpacking from a literal tuple/list.
    fn unpack_element_exprs<'b>(
        &self,
        value_expr: Option<&'b Expr>,
        count: usize,
    ) -> Vec<Option<&'b Expr>> {
        if let Some(expr) = value_expr {
            match &expr.kind {
                ExprKind::Tuple(items) | ExprKind::List(items) if items.len() == count => {
                    return items.iter().map(Some).collect();
                }
                _ => {}
            }
        }
        vec![None; count]
    }
}
