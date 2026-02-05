use super::*;

/// This module scans the HIR to determine which helper functions and imports are needed.
///
/// Why scan before generating?
/// - We only want to emit helpers that are actually used (keeps output clean)
/// - We need to know imports (HashMap, HashSet) before emitting the header
/// - Some optimizations require whole-program analysis (e.g., __name__ comparison)
///
/// The scan pass is read-only - it doesn't modify the HIR, just sets flags in `Uses`.
impl<'a> Codegen<'a> {
    /// Scan the entire program to determine which helpers are needed.
    ///
    /// This traverses all functions, classes, statements, and expressions to find:
    /// - Builtin calls (print, len, range, etc.)
    /// - Collection types (dict, set - need HashMap/HashSet imports)
    /// - Special operations (slicing, indexing - may need helpers)
    pub(crate) fn collect_uses(&mut self, program: &Program) -> Result<(), CompileError> {
        for item in &program.items {
            match item {
                Item::Function(func) => self.scan_function(func)?,
                Item::Class(class) => {
                    for method in &class.methods {
                        self.scan_function(method)?;
                    }
                }
                Item::Stmt(stmt) => self.scan_stmt(stmt.as_ref())?,
                Item::Union(_) => {}
            }
        }
        for ty in self.ctx.globals.values() {
            self.scan_type_uses(ty);
        }
        Ok(())
    }

    fn scan_type_uses(&mut self, ty: &Type) {
        match ty {
            Type::Dict(k, v) => {
                self.uses.hash_map = true;
                self.scan_type_uses(k);
                self.scan_type_uses(v);
            }
            Type::Set(inner) => {
                self.uses.hash_set = true;
                self.scan_type_uses(inner);
            }
            Type::List(inner) | Type::Option(inner) | Type::Iterator(inner) => {
                self.scan_type_uses(inner);
            }
            Type::Tuple(items) => {
                for item in items {
                    self.scan_type_uses(item);
                }
            }
            Type::Lambda { params, ret } => {
                for p in params {
                    self.scan_type_uses(p);
                }
                self.scan_type_uses(ret);
            }
            Type::Ref(inner) | Type::MutRef(inner) | Type::Slice(inner) => {
                self.scan_type_uses(inner);
            }
            Type::Result(ok, err) => {
                self.scan_type_uses(ok);
                self.scan_type_uses(err);
            }
            Type::Custom(_)
            | Type::Union(_)
            | Type::Exception(_)
            | Type::Int
            | Type::Float
            | Type::Bool
            | Type::Str
            | Type::Bytes
            | Type::None
            | Type::Unknown => {}
        }
    }

    /// Analyze if __name__ is only used in comparisons with string literals.
    ///
    /// Common Python idiom: `if __name__ == "__main__":`
    ///
    /// Optimization opportunity:
    /// - If __name__ is only compared to literals, we can emit const comparisons:
    ///   `__NAME__ == "__main__"` (no allocation)
    /// - Otherwise, we must emit `__NAME__.to_string()` on each access (allocates)
    ///
    /// This function returns true if we can use the optimized version.
    /// It traverses the entire program looking for __name__ uses that aren't
    /// simple string literal comparisons.
    pub(crate) fn analyze_name_compare_only(&self, program: &Program) -> bool {
        let mut ok = true;
        fn visit_stmt(stmt: &Stmt, ok: &mut bool) {
            if !*ok {
                return;
            }
            match &stmt.kind {
                StmtKind::Let { value, .. } => visit_expr(value, ok),
                StmtKind::Assign { value, .. } => visit_expr(value, ok),
                StmtKind::Return { value } => {
                    if let Some(expr) = value {
                        visit_expr(expr, ok);
                    }
                }
                StmtKind::If { test, body, orelse } => {
                    visit_expr(test, ok);
                    for stmt in body {
                        visit_stmt(stmt, ok);
                    }
                    for stmt in orelse {
                        visit_stmt(stmt, ok);
                    }
                }
                StmtKind::While { test, body } => {
                    visit_expr(test, ok);
                    for stmt in body {
                        visit_stmt(stmt, ok);
                    }
                }
                StmtKind::For { iter, body, .. } => {
                    visit_expr(iter, ok);
                    for stmt in body {
                        visit_stmt(stmt, ok);
                    }
                }
                StmtKind::Expr(expr) => visit_expr(expr, ok),
                StmtKind::Assert { test, msg } => {
                    visit_expr(test, ok);
                    if let Some(msg) = msg {
                        visit_expr(msg, ok);
                    }
                }
                StmtKind::Match { subject, cases } => {
                    visit_expr(subject, ok);
                    for case in cases {
                        for stmt in &case.body {
                            visit_stmt(stmt, ok);
                        }
                    }
                }
                StmtKind::Try {
                    body,
                    handlers,
                    orelse,
                    finalbody,
                } => {
                    for stmt in body {
                        visit_stmt(stmt, ok);
                    }
                    for handler in handlers {
                        for stmt in &handler.body {
                            visit_stmt(stmt, ok);
                        }
                    }
                    for stmt in orelse {
                        visit_stmt(stmt, ok);
                    }
                    for stmt in finalbody {
                        visit_stmt(stmt, ok);
                    }
                }
                StmtKind::Raise { exc, cause } => {
                    if let Some(expr) = exc {
                        visit_expr(expr, ok);
                    }
                    if let Some(expr) = cause {
                        visit_expr(expr, ok);
                    }
                }
                StmtKind::Global { .. } | StmtKind::Nonlocal { .. } => {}
                StmtKind::Break | StmtKind::Continue => {}
            }
        }

        fn is_name_name(expr: &Expr) -> bool {
            matches!(&expr.kind, ExprKind::Name(name) if name == "__name__")
        }

        fn is_str_lit(expr: &Expr) -> bool {
            matches!(&expr.kind, ExprKind::Literal(Literal::Str(_)))
        }

        fn visit_expr(expr: &Expr, ok: &mut bool) {
            if !*ok {
                return;
            }
            match &expr.kind {
                ExprKind::Name(name) => {
                    if name == "__name__" {
                        *ok = false;
                    }
                }
                ExprKind::Compare { op, left, right } => {
                    let left_is_name = is_name_name(left);
                    let right_is_name = is_name_name(right);
                    if left_is_name || right_is_name {
                        if !matches!(op, CmpOp::Eq | CmpOp::NotEq) {
                            *ok = false;
                            return;
                        }
                        if left_is_name && !is_str_lit(right) {
                            *ok = false;
                            return;
                        }
                        if right_is_name && !is_str_lit(left) {
                            *ok = false;
                            return;
                        }
                        return;
                    }
                    visit_expr(left, ok);
                    visit_expr(right, ok);
                }
                ExprKind::CompareChain {
                    left, comparators, ..
                } => {
                    if is_name_name(left) || comparators.iter().any(is_name_name) {
                        *ok = false;
                        return;
                    }
                    visit_expr(left, ok);
                    for cmp in comparators {
                        visit_expr(cmp, ok);
                    }
                }
                ExprKind::Call {
                    func,
                    args,
                    keywords,
                } => {
                    visit_expr(func, ok);
                    for arg in args {
                        visit_expr(arg, ok);
                    }
                    for kw in keywords {
                        visit_expr(&kw.value, ok);
                    }
                }
                ExprKind::Attr { value, .. } => visit_expr(value, ok),
                ExprKind::Binary { left, right, .. } => {
                    visit_expr(left, ok);
                    visit_expr(right, ok);
                }
                ExprKind::Unary { expr: inner, .. } => visit_expr(inner, ok),
                ExprKind::BoolOp { values, .. } => {
                    for v in values {
                        visit_expr(v, ok);
                    }
                }
                ExprKind::List(items) | ExprKind::Tuple(items) | ExprKind::Set(items) => {
                    for item in items {
                        visit_expr(item, ok);
                    }
                }
                ExprKind::Dict(items) => {
                    for (k, v) in items {
                        visit_expr(k, ok);
                        visit_expr(v, ok);
                    }
                }
                ExprKind::Index { value, index } => {
                    visit_expr(value, ok);
                    visit_expr(index, ok);
                }
                ExprKind::Slice {
                    value,
                    start,
                    end,
                    step,
                } => {
                    visit_expr(value, ok);
                    if let Some(s) = start {
                        visit_expr(s, ok);
                    }
                    if let Some(e) = end {
                        visit_expr(e, ok);
                    }
                    if let Some(st) = step.as_deref() {
                        visit_expr(st, ok);
                    }
                }
                ExprKind::ListComp { elt, iter, ifs, .. }
                | ExprKind::SetComp { elt, iter, ifs, .. } => {
                    visit_expr(elt, ok);
                    visit_expr(iter, ok);
                    for cond in ifs {
                        visit_expr(cond, ok);
                    }
                }
                ExprKind::UnionCtor { inner, .. } => visit_expr(inner, ok),
                ExprKind::Lambda { body, .. } => visit_expr(body, ok),
                ExprKind::IfExpr { test, body, orelse } => {
                    visit_expr(test, ok);
                    visit_expr(body, ok);
                    visit_expr(orelse, ok);
                }
                ExprKind::Block { stmts } => {
                    for stmt in stmts {
                        visit_stmt(stmt, ok);
                    }
                }
                ExprKind::Literal(_) => {}
            }
        }

        for item in &program.items {
            match item {
                Item::Function(func) => {
                    for stmt in &func.body {
                        visit_stmt(stmt, &mut ok);
                    }
                }
                Item::Class(class) => {
                    for method in &class.methods {
                        for stmt in &method.body {
                            visit_stmt(stmt, &mut ok);
                        }
                    }
                }
                Item::Stmt(stmt) => visit_stmt(stmt.as_ref(), &mut ok),
                Item::Union(_) => {}
            }
        }

        ok
    }

    fn scan_function(&mut self, func: &Function) -> Result<(), CompileError> {
        for stmt in &func.body {
            self.scan_stmt(stmt)?;
        }
        Ok(())
    }

    fn scan_stmt(&mut self, stmt: &Stmt) -> Result<(), CompileError> {
        match &stmt.kind {
            StmtKind::Let { value, .. } => self.scan_expr(value)?,
            StmtKind::Assign { value, .. } => self.scan_expr(value)?,
            StmtKind::Return { value } => {
                if let Some(expr) = value {
                    self.scan_expr(expr)?;
                }
            }
            StmtKind::If { test, body, orelse } => {
                self.scan_expr(test)?;
                for stmt in body {
                    self.scan_stmt(stmt)?;
                }
                for stmt in orelse {
                    self.scan_stmt(stmt)?;
                }
            }
            StmtKind::While { test, body } => {
                self.scan_expr(test)?;
                for stmt in body {
                    self.scan_stmt(stmt)?;
                }
            }
            StmtKind::For { iter, body, .. } => {
                self.scan_expr(iter)?;
                for stmt in body {
                    self.scan_stmt(stmt)?;
                }
            }
            StmtKind::Expr(expr) => self.scan_expr(expr)?,
            StmtKind::Assert { test, msg } => {
                self.scan_expr(test)?;
                if let Some(msg) = msg {
                    self.scan_expr(msg)?;
                }
            }
            StmtKind::Match { subject, cases } => {
                self.scan_expr(subject)?;
                for case in cases {
                    for stmt in &case.body {
                        self.scan_stmt(stmt)?;
                    }
                }
            }
            StmtKind::Try {
                body,
                handlers,
                orelse,
                finalbody,
            } => {
                for stmt in body {
                    self.scan_stmt(stmt)?;
                }
                for handler in handlers {
                    for stmt in &handler.body {
                        self.scan_stmt(stmt)?;
                    }
                }
                for stmt in orelse {
                    self.scan_stmt(stmt)?;
                }
                for stmt in finalbody {
                    self.scan_stmt(stmt)?;
                }
            }
            StmtKind::Raise { exc, cause } => {
                if let Some(expr) = exc {
                    self.scan_expr(expr)?;
                }
                if let Some(expr) = cause {
                    self.scan_expr(expr)?;
                }
            }
            StmtKind::Global { .. } | StmtKind::Nonlocal { .. } => {}
            StmtKind::Break | StmtKind::Continue => {}
        }
        Ok(())
    }

    fn scan_expr(&mut self, expr: &Expr) -> Result<(), CompileError> {
        match &expr.kind {
            ExprKind::Call {
                func,
                args,
                keywords,
            } => {
                if let ExprKind::Name(name) = &func.kind {
                    if name == "print" {
                        self.uses.print = true;
                    }
                    if name == "len" {
                        self.uses.len = true;
                    }
                    if name == "range" {
                        if args.len() == 1 {
                            self.uses.range = true;
                        } else {
                            self.uses.range2 = true;
                        }
                    }
                    if name == "round" {
                        self.uses.round = true;
                    }
                    if name == "type" {
                        self.uses.type_name = true;
                    }
                }
                for arg in args {
                    self.scan_expr(arg)?;
                }
                for kw in keywords {
                    self.scan_expr(&kw.value)?;
                }
            }
            ExprKind::Dict(_) => {
                self.uses.hash_map = true;
            }
            ExprKind::Set(items) => {
                self.uses.hash_set = true;
                for item in items {
                    self.scan_expr(item)?;
                }
            }
            ExprKind::Attr { value, attr } => {
                if attr == "__name__" {
                    if let ExprKind::Call { func, .. } = &value.kind {
                        if let ExprKind::Name(name) = &func.kind {
                            if name == "type" {
                                self.uses.type_name = true;
                            }
                        }
                    }
                }
                self.scan_expr(value)?;
            }
            ExprKind::List(items) | ExprKind::Tuple(items) => {
                for item in items {
                    self.scan_expr(item)?;
                }
            }
            ExprKind::Binary { left, right, .. } => {
                self.scan_expr(left)?;
                self.scan_expr(right)?;
            }
            ExprKind::Unary { expr: inner, .. } => self.scan_expr(inner)?,
            ExprKind::Compare { left, right, .. } => {
                self.scan_expr(left)?;
                self.scan_expr(right)?;
            }
            ExprKind::CompareChain {
                left, comparators, ..
            } => {
                self.scan_expr(left)?;
                for cmp in comparators {
                    self.scan_expr(cmp)?;
                }
            }
            ExprKind::BoolOp { values, .. } => {
                for v in values {
                    self.scan_expr(v)?;
                }
            }
            ExprKind::Index { value, index } => {
                self.scan_expr(value)?;
                self.scan_expr(index)?;
            }
            ExprKind::Slice {
                value,
                start,
                end,
                step,
            } => {
                self.scan_expr(value)?;
                if let Some(s) = start {
                    self.scan_expr(s)?;
                }
                if let Some(e) = end {
                    self.scan_expr(e)?;
                }
                if let Some(st) = step.as_deref() {
                    self.scan_expr(st)?;
                }
            }
            ExprKind::ListComp { elt, iter, ifs, .. } => {
                self.scan_expr(elt)?;
                self.scan_expr(iter)?;
                for cond in ifs {
                    self.scan_expr(cond)?;
                }
            }
            ExprKind::SetComp { elt, iter, ifs, .. } => {
                self.uses.hash_set = true;
                self.scan_expr(elt)?;
                self.scan_expr(iter)?;
                for cond in ifs {
                    self.scan_expr(cond)?;
                }
            }
            ExprKind::Lambda { body, .. } => self.scan_expr(body)?,
            ExprKind::IfExpr { test, body, orelse } => {
                self.scan_expr(test)?;
                self.scan_expr(body)?;
                self.scan_expr(orelse)?;
            }
            ExprKind::Block { stmts } => {
                for stmt in stmts {
                    self.scan_stmt(stmt)?;
                }
            }
            ExprKind::UnionCtor { inner, .. } => self.scan_expr(inner)?,
            ExprKind::Literal(_) | ExprKind::Name(_) => {}
        }
        Ok(())
    }
}
