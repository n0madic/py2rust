use super::*;

impl<'a> TypeChecker<'a> {
    pub(super) fn check_function(
        &mut self,
        func: &mut Function,
        class_name: Option<&str>,
    ) -> Result<(), CompileError> {
        self.scopes.push(HashMap::new());
        self.global_scopes.push(GlobalScope::default());
        if let Some(class_name) = class_name {
            if let Some(first) = func.params.first() {
                let self_ty = self.resolve_type_ref(&first.ann, first.span)?;
                self.insert_var(&first.name, self_ty, first.span)?;
                if first.name != "self" {
                    return Err(self.error(first.span, "First parameter in methods must be self"));
                }
            } else {
                return Err(self.error(func.span, "Methods must take self parameter"));
            }
            if class_name != func.params[0].ann.to_string() && !class_name.is_empty() {
                // Ignore mismatch for now if annotated differently.
            }
        }
        for param in func
            .params
            .iter()
            .skip(if class_name.is_some() { 1 } else { 0 })
        {
            let ty = self.resolve_type_ref(&param.ann, param.span)?;
            self.insert_var(&param.name, ty, param.span)?;
        }

        for stmt in &mut func.body {
            self.check_stmt(stmt, Some(&func.ret))?;
        }

        if matches!(func.ret, TypeRef::Unknown) {
            let mut inferred: Option<Type> = None;
            fn visit(stmt: &Stmt, inferred: &mut Option<Type>) {
                match &stmt.kind {
                    StmtKind::Return { value } => {
                        let ty = match value {
                            Some(expr) => expr.ty.clone().unwrap_or(Type::Unknown),
                            None => Type::None,
                        };
                        if let Some(existing) = inferred {
                            if existing != &ty {
                                *inferred = Some(Type::Unknown);
                            }
                        } else {
                            *inferred = Some(ty);
                        }
                    }
                    StmtKind::If { body, orelse, .. } => {
                        for stmt in body {
                            visit(stmt, inferred);
                        }
                        for stmt in orelse {
                            visit(stmt, inferred);
                        }
                    }
                    StmtKind::While { body, .. } => {
                        for stmt in body {
                            visit(stmt, inferred);
                        }
                    }
                    StmtKind::For { body, .. } => {
                        for stmt in body {
                            visit(stmt, inferred);
                        }
                    }
                    StmtKind::Match { cases, .. } => {
                        for case in cases {
                            for stmt in &case.body {
                                visit(stmt, inferred);
                            }
                        }
                    }
                    _ => {}
                }
            }
            for stmt in &func.body {
                visit(stmt, &mut inferred);
            }
            if let Some(ty) = inferred {
                if !matches!(ty, Type::Unknown) {
                    func.ret = Self::type_to_ref(&ty);
                }
            } else {
                func.ret = TypeRef::None;
            }
        }

        // Update inferred parameter types in the function signature.
        if let Some(scope) = self.scopes.last() {
            for param in &mut func.params {
                if matches!(param.ann, TypeRef::Unknown) {
                    if let Some(ty) = scope.get(&param.name) {
                        if !matches!(ty, Type::Unknown) {
                            param.ann = Self::type_to_ref(ty);
                        }
                    }
                }
            }
        }
        if func
            .params
            .iter()
            .any(|p| matches!(p.ann, TypeRef::Unknown))
        {
            use std::collections::HashSet;
            let mut string_params = HashSet::new();
            fn is_str_expr(expr: &Expr) -> bool {
                matches!(&expr.kind, ExprKind::Literal(Literal::Str(_)))
                    || matches!(expr.ty.as_ref(), Some(Type::Str))
            }
            fn collect_names(expr: &Expr, out: &mut HashSet<String>) {
                match &expr.kind {
                    ExprKind::Name(name) => {
                        out.insert(name.clone());
                    }
                    ExprKind::Binary { left, right, .. } => {
                        collect_names(left, out);
                        collect_names(right, out);
                    }
                    ExprKind::Call { func, args } => {
                        collect_names(func, out);
                        for arg in args {
                            collect_names(arg, out);
                        }
                    }
                    ExprKind::Attr { value, .. } => collect_names(value, out),
                    ExprKind::Compare { left, right, .. } => {
                        collect_names(left, out);
                        collect_names(right, out);
                    }
                    ExprKind::Unary { expr: inner, .. } => collect_names(inner, out),
                    ExprKind::BoolOp { values, .. } => {
                        for v in values {
                            collect_names(v, out);
                        }
                    }
                    ExprKind::List(items) | ExprKind::Tuple(items) | ExprKind::Set(items) => {
                        for item in items {
                            collect_names(item, out);
                        }
                    }
                    ExprKind::Dict(items) => {
                        for (k, v) in items {
                            collect_names(k, out);
                            collect_names(v, out);
                        }
                    }
                    ExprKind::Index { value, index } => {
                        collect_names(value, out);
                        collect_names(index, out);
                    }
                    ExprKind::Slice { value, start, end } => {
                        collect_names(value, out);
                        if let Some(s) = start {
                            collect_names(s, out);
                        }
                        if let Some(e) = end {
                            collect_names(e, out);
                        }
                    }
                    ExprKind::ListComp { elt, iter, ifs, .. } => {
                        collect_names(elt, out);
                        collect_names(iter, out);
                        for cond in ifs {
                            collect_names(cond, out);
                        }
                    }
                    ExprKind::UnionCtor { inner, .. } => collect_names(inner, out),
                    ExprKind::Lambda { body, .. } => collect_names(body, out),
                    ExprKind::IfExpr { test, body, orelse } => {
                        collect_names(test, out);
                        collect_names(body, out);
                        collect_names(orelse, out);
                    }
                    ExprKind::Block { stmts } => {
                        for stmt in stmts {
                            collect_names_in_stmt(stmt, out);
                        }
                    }
                    ExprKind::Literal(_) => {}
                }
            }
            fn visit_expr(expr: &Expr, out: &mut HashSet<String>) {
                match &expr.kind {
                    ExprKind::Binary {
                        op: BinOp::Add,
                        left,
                        right,
                    } => {
                        if is_str_expr(left) {
                            collect_names(right, out);
                        }
                        if is_str_expr(right) {
                            collect_names(left, out);
                        }
                        visit_expr(left, out);
                        visit_expr(right, out);
                    }
                    ExprKind::Binary { left, right, .. } => {
                        visit_expr(left, out);
                        visit_expr(right, out);
                    }
                    ExprKind::Call { func, args } => {
                        visit_expr(func, out);
                        for arg in args {
                            visit_expr(arg, out);
                        }
                    }
                    ExprKind::Attr { value, .. } => visit_expr(value, out),
                    ExprKind::Compare { left, right, .. } => {
                        visit_expr(left, out);
                        visit_expr(right, out);
                    }
                    ExprKind::Unary { expr: inner, .. } => visit_expr(inner, out),
                    ExprKind::BoolOp { values, .. } => {
                        for v in values {
                            visit_expr(v, out);
                        }
                    }
                    ExprKind::List(items) | ExprKind::Tuple(items) | ExprKind::Set(items) => {
                        for item in items {
                            visit_expr(item, out);
                        }
                    }
                    ExprKind::Dict(items) => {
                        for (k, v) in items {
                            visit_expr(k, out);
                            visit_expr(v, out);
                        }
                    }
                    ExprKind::Index { value, index } => {
                        visit_expr(value, out);
                        visit_expr(index, out);
                    }
                    ExprKind::Slice { value, start, end } => {
                        visit_expr(value, out);
                        if let Some(s) = start {
                            visit_expr(s, out);
                        }
                        if let Some(e) = end {
                            visit_expr(e, out);
                        }
                    }
                    ExprKind::ListComp { elt, iter, ifs, .. } => {
                        visit_expr(elt, out);
                        visit_expr(iter, out);
                        for cond in ifs {
                            visit_expr(cond, out);
                        }
                    }
                    ExprKind::UnionCtor { inner, .. } => visit_expr(inner, out),
                    ExprKind::Lambda { body, .. } => visit_expr(body, out),
                    ExprKind::IfExpr { test, body, orelse } => {
                        visit_expr(test, out);
                        visit_expr(body, out);
                        visit_expr(orelse, out);
                    }
                    ExprKind::Block { stmts } => {
                        for stmt in stmts {
                            visit_stmt(stmt, out);
                        }
                    }
                    ExprKind::Literal(_) | ExprKind::Name(_) => {}
                }
            }
            fn collect_names_in_stmt(stmt: &Stmt, out: &mut HashSet<String>) {
                match &stmt.kind {
                    StmtKind::Let { value, .. } => collect_names(value, out),
                    StmtKind::Assign { value, .. } => collect_names(value, out),
                    StmtKind::Return { value } => {
                        if let Some(expr) = value {
                            collect_names(expr, out);
                        }
                    }
                    StmtKind::If { test, body, orelse } => {
                        collect_names(test, out);
                        for stmt in body {
                            collect_names_in_stmt(stmt, out);
                        }
                        for stmt in orelse {
                            collect_names_in_stmt(stmt, out);
                        }
                    }
                    StmtKind::While { test, body } => {
                        collect_names(test, out);
                        for stmt in body {
                            collect_names_in_stmt(stmt, out);
                        }
                    }
                    StmtKind::For { iter, body, .. } => {
                        collect_names(iter, out);
                        for stmt in body {
                            collect_names_in_stmt(stmt, out);
                        }
                    }
                    StmtKind::Expr(expr) => collect_names(expr, out),
                    StmtKind::Assert { test, msg } => {
                        collect_names(test, out);
                        if let Some(msg) = msg {
                            collect_names(msg, out);
                        }
                    }
                    StmtKind::Match { subject, cases } => {
                        collect_names(subject, out);
                        for case in cases {
                            for stmt in &case.body {
                                collect_names_in_stmt(stmt, out);
                            }
                        }
                    }
                    StmtKind::Global { .. } => {}
                    StmtKind::Break | StmtKind::Continue => {}
                }
            }
            fn visit_stmt(stmt: &Stmt, out: &mut HashSet<String>) {
                match &stmt.kind {
                    StmtKind::Let { value, .. } => visit_expr(value, out),
                    StmtKind::Assign { value, .. } => visit_expr(value, out),
                    StmtKind::Return { value } => {
                        if let Some(expr) = value {
                            visit_expr(expr, out);
                        }
                    }
                    StmtKind::If { test, body, orelse } => {
                        visit_expr(test, out);
                        for stmt in body {
                            visit_stmt(stmt, out);
                        }
                        for stmt in orelse {
                            visit_stmt(stmt, out);
                        }
                    }
                    StmtKind::While { test, body } => {
                        visit_expr(test, out);
                        for stmt in body {
                            visit_stmt(stmt, out);
                        }
                    }
                    StmtKind::For { iter, body, .. } => {
                        visit_expr(iter, out);
                        for stmt in body {
                            visit_stmt(stmt, out);
                        }
                    }
                    StmtKind::Expr(expr) => visit_expr(expr, out),
                    StmtKind::Assert { test, msg } => {
                        visit_expr(test, out);
                        if let Some(msg) = msg {
                            visit_expr(msg, out);
                        }
                    }
                    StmtKind::Match { subject, cases } => {
                        visit_expr(subject, out);
                        for case in cases {
                            for stmt in &case.body {
                                visit_stmt(stmt, out);
                            }
                        }
                    }
                    StmtKind::Global { .. } => {}
                    StmtKind::Break | StmtKind::Continue => {}
                }
            }
            for stmt in &func.body {
                visit_stmt(stmt, &mut string_params);
            }
            for param in &mut func.params {
                if matches!(param.ann, TypeRef::Unknown) && string_params.contains(&param.name) {
                    param.ann = TypeRef::Name("str".to_string());
                }
            }
        }
        let params = self.resolve_params(&func.params)?;
        let ret = self.resolve_type_ref(&func.ret, func.span)?;
        if let Some(class_name) = class_name {
            if let Some(class_info) = self.ctx.classes.get_mut(class_name) {
                class_info.methods.insert(
                    func.name.clone(),
                    FunctionSig {
                        params: params.clone(),
                        ret: ret.clone(),
                        span: func.span,
                    },
                );
            }
        } else {
            self.ctx.functions.insert(
                func.name.clone(),
                FunctionSig {
                    params,
                    ret,
                    span: func.span,
                },
            );
        }

        self.global_scopes.pop();
        self.scopes.pop();
        Ok(())
    }
}
