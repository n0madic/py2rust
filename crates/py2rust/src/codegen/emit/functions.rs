// Function and main emission plus signature helpers.

use super::super::util::collect_assign_counts;
use super::super::*;
use std::collections::{HashMap, HashSet};

impl<'a> Codegen<'a> {
    /// Emit a function or method body.
    pub(crate) fn emit_function(
        &mut self,
        func: &Function,
        class: Option<&ClassDef>,
    ) -> Result<(), CompileError> {
        // Set current function for tracking throws.
        self.current_function = Some(func.name.clone());
        self.local_vars = Some(HashMap::new());

        // Track parameter types and return type for option coercions.
        let mut param_types: Option<Vec<Type>> = None;
        let mut ret_ty: Option<Type> = None;
        if let Some(class_def) = class {
            if let Some(info) = self.ctx.classes.get(&class_def.name) {
                if let Some(sig) = info.methods.get(&func.name) {
                    param_types = Some(sig.params.clone());
                    ret_ty = Some(sig.ret.clone());
                }
            }
        } else if let Some(sig) = self.ctx.functions.get(&func.name) {
            param_types = Some(sig.params.clone());
            ret_ty = Some(sig.ret.clone());
        }
        if ret_ty.is_none() {
            ret_ty = Some(self.resolve_type_ref(&func.ret, func.span)?);
        }
        self.current_function_ret = ret_ty;

        if let Some(params) = param_types {
            for (param, ty) in func.params.iter().zip(params.into_iter()) {
                self.set_local_var_type(&param.name, ty);
            }
        } else {
            for param in &func.params {
                let ty = self.resolve_type_ref(&param.ann, param.span)?;
                self.set_local_var_type(&param.name, ty);
            }
        }

        // Precompute nonlocal declarations and cell-backed locals for this scope.
        let param_names: Vec<String> = func.params.iter().map(|p| p.name.clone()).collect();
        let nonlocal_info = self.collect_nonlocal_info_for_stmts(&func.body, &param_names);
        self.nonlocal_decls = Some(nonlocal_info.nonlocal_decls);
        self.cell_locals = Some(nonlocal_info.cell_locals);

        let sig = if let Some(class) = class {
            self.method_signature(func, class)?
        } else {
            self.function_signature(func)?
        };
        let vis = "pub ";
        self.push_line(&format!("{}fn {}{} {{", vis, func.name, sig));
        self.indent += 1;
        // Precompute list element type hints for this function.
        self.inferred_list_elems = Some(self.collect_list_elem_types_for_stmts(&func.body));
        // Precompute list storage strategy for this function's locals.
        self.local_list_storage =
            Some(self.collect_list_storage_for_stmts(&func.body, &HashSet::new()));
        // Precompute dict storage strategy for this function's locals.
        self.local_dict_storage =
            Some(self.collect_dict_storage_for_stmts(&func.body, &HashSet::new()));
        let mut_counts = collect_assign_counts(&func.body);
        for stmt in &func.body {
            self.emit_stmt(stmt, &mut_counts)?;
        }

        // If function can throw and doesn't end with explicit return, add Ok(()).
        let can_throw = self
            .ctx
            .functions
            .get(&func.name)
            .is_some_and(|s| s.can_throw);
        if can_throw && !self.ends_with_return(&func.body) {
            self.push_line("Ok(())");
        }

        self.indent -= 1;
        self.push_line("}");
        self.push_line("");

        // Clear current function.
        self.current_function = None;
        self.current_function_ret = None;
        self.local_vars = None;
        self.nonlocal_decls = None;
        self.cell_locals = None;
        self.inferred_list_elems = None;
        self.local_list_storage = None;
        self.local_dict_storage = None;
        Ok(())
    }

    /// Emit the top-level `main` function.
    pub(crate) fn emit_main(
        &mut self,
        program: &Program,
        body: &[Stmt],
    ) -> Result<(), CompileError> {
        // Top-level code has no nonlocal bindings.
        self.nonlocal_decls = None;
        self.cell_locals = None;
        // Check if top-level contains exception handling.
        let top_level_can_throw = self.analyze_top_level_throws(body);
        self.top_level_can_throw = top_level_can_throw;

        if top_level_can_throw {
            // Wrap in try closure that catches errors.
            self.push_line("fn main() {");
            self.indent += 1;
            self.push_line("let _result = (|| -> Result<(), PyError> {");
            self.indent += 1;

            // Initialize defaults and class attributes before running top-level code.
            self.emit_pre_main_inits(program)?;
            let mut_counts = collect_assign_counts(body);
            for stmt in body {
                self.emit_stmt(stmt, &mut_counts)?;
            }

            self.push_line("Ok(())");
            self.indent -= 1;
            self.push_line("})();");

            self.push_line("");
            self.push_line("if let Err(e) = _result {");
            self.indent += 1;
            self.push_line("eprintln!(\"Uncaught exception: {}\", e);");
            self.push_line("std::process::exit(1);");
            self.indent -= 1;
            self.push_line("}");

            self.indent -= 1;
            self.push_line("}");
        } else {
            // Normal main.
            self.push_line("fn main() {");
            self.indent += 1;
            // Initialize defaults and class attributes before running top-level code.
            self.emit_pre_main_inits(program)?;
            let mut_counts = collect_assign_counts(body);
            for stmt in body {
                self.emit_stmt(stmt, &mut_counts)?;
            }
            self.indent -= 1;
            self.push_line("}");
        }

        Ok(())
    }

    fn function_signature(&mut self, func: &Function) -> Result<String, CompileError> {
        // Clear borrowed params from previous function.
        self.borrowed_params.clear();

        let mut params = Vec::new();
        let mut generics: Vec<String> = Vec::new();
        let mut param_types: HashMap<String, String> = HashMap::new();
        let mut generic_idx = 0usize;
        for param in &func.params {
            let ty = self.resolve_type_ref(&param.ann, param.span)?;
            let ty_str = if matches!(ty, Type::Unknown) {
                let name = format!("T{}", generic_idx);
                generic_idx += 1;
                generics.push(name.clone());
                name
            } else {
                // Convert to borrowed type for function parameters.
                let borrowed = self.to_borrowed_param_type(&ty);
                // Track if this parameter is borrowed.
                if self.is_borrowed_type(&borrowed) {
                    self.borrowed_params.insert(param.name.clone());
                }
                self.rust_type(&borrowed)
            };
            param_types.insert(param.name.clone(), ty_str.clone());
            params.push(format!("{}: {}", param.name, ty_str));
        }

        // Get the return type from context (already wrapped in Result if can_throw).
        let ret_ty = if let Some(sig) = self.ctx.functions.get(&func.name) {
            sig.ret.clone()
        } else {
            self.resolve_type_ref(&func.ret, func.span)?
        };

        let mut ret_str = if matches!(ret_ty, Type::Unknown) {
            "()".to_string()
        } else {
            self.rust_type(&ret_ty)
        };
        if matches!(ret_ty, Type::Unknown) {
            if let Some(ret_name) = identity_return_param(func) {
                if let Some(param_ty) = param_types.get(&ret_name) {
                    ret_str = param_ty.clone();
                }
            }
        }
        let generics = if generics.is_empty() {
            String::new()
        } else {
            format!("<{}>", generics.join(", "))
        };
        Ok(format!(
            "{}({}) -> {}",
            generics,
            params.join(", "),
            ret_str
        ))
    }

    /// Check if a type is a borrowed/reference type.
    fn is_borrowed_type(&self, ty: &Type) -> bool {
        matches!(ty, Type::Ref(_) | Type::MutRef(_) | Type::Slice(_))
    }

    /// Convert a type to its borrowed equivalent for function parameters.
    /// - list[T] -> Arc<Mutex<Vec<T>>> (shared list, no borrowing)
    /// - str -> &str
    /// - dict[K,V] -> Arc<Mutex<HashMap<K,V>>> (shared dict, no borrowing)
    /// - Primitives (int, float, bool) stay owned (Copy types)
    pub(crate) fn to_borrowed_param_type(&self, ty: &Type) -> Type {
        match ty {
            // Copy types stay as-is.
            Type::Int | Type::Float | Type::Bool | Type::None => ty.clone(),
            // String stays owned.
            Type::Str => Type::Str,
            // Bytes stay owned.
            Type::Bytes => Type::Bytes,
            // Lists are shared (Arc<Mutex<...>>), so keep owned.
            Type::List(_) => ty.clone(),
            // Dicts are shared (Arc<Mutex<...>>), so keep owned.
            Type::Dict(_, _) => ty.clone(),
            Type::Set(inner) => Type::Ref(Box::new(Type::Set(inner.clone()))),
            // Tuples stay owned (they can contain Copy types or be small).
            Type::Tuple(_) => ty.clone(),
            // Option stays owned.
            Type::Option(_) => ty.clone(),
            // Custom/Union types get borrowed.
            Type::Custom(name) => Type::Ref(Box::new(Type::Custom(name.clone()))),
            Type::Union(name) => Type::Ref(Box::new(Type::Union(name.clone()))),
            // Iterator stays as-is.
            Type::Iterator(_) => ty.clone(),
            // Lambda stays as-is.
            Type::Lambda { .. } => ty.clone(),
            // Reference types stay as-is.
            Type::Ref(_) | Type::MutRef(_) | Type::Slice(_) => ty.clone(),
            // Result and Exception stay as-is.
            Type::Result(_, _) | Type::Exception(_) => ty.clone(),
            // Unknown stays as-is.
            Type::Unknown => ty.clone(),
        }
    }

    fn method_signature(
        &mut self,
        func: &Function,
        class: &ClassDef,
    ) -> Result<String, CompileError> {
        // Clear borrowed params from previous function.
        self.borrowed_params.clear();

        let mut params = Vec::new();
        let kind = class
            .method_kinds
            .get(&func.name)
            .copied()
            .unwrap_or(MethodKind::Instance);
        let mut iter = func.params.iter();
        if matches!(kind, MethodKind::Instance) {
            if let Some(self_param) = iter.next() {
                let self_ty = self.resolve_type_ref(&self_param.ann, self_param.span)?;
                let is_mut = self.method_is_mutating(func);
                let receiver = if is_mut { "&mut self" } else { "&self" };
                params.push(receiver.to_string());
                // self is always a borrowed reference in methods.
                self.borrowed_params.insert(self_param.name.clone());
                if let Type::Custom(name) = self_ty {
                    if !class.name.is_empty() && name != class.name {
                        // ignore mismatch
                    }
                }
            }
        } else if matches!(kind, MethodKind::Class) {
            // For classmethods, consume the cls parameter and generate cls: ()
            if let Some(cls_param) = iter.next() {
                params.push(format!("{}: ()", cls_param.name));
            }
        }
        for param in iter {
            let ty = self.resolve_type_ref(&param.ann, param.span)?;
            let ty_str = if matches!(ty, Type::Unknown) {
                "()".to_string()
            } else {
                // Convert to borrowed type for method parameters.
                let borrowed = self.to_borrowed_param_type(&ty);
                // Track if this parameter is borrowed.
                if self.is_borrowed_type(&borrowed) {
                    self.borrowed_params.insert(param.name.clone());
                }
                self.rust_type(&borrowed)
            };
            params.push(format!("{}: {}", param.name, ty_str));
        }
        let ret_ty = self.resolve_type_ref(&func.ret, func.span)?;
        let ret_str = if matches!(ret_ty, Type::Unknown) {
            "()".to_string()
        } else {
            self.rust_type(&ret_ty)
        };
        Ok(format!("({}) -> {}", params.join(", "), ret_str))
    }

    pub(crate) fn method_is_mutating(&self, func: &Function) -> bool {
        fn stmt_mutates(stmt: &Stmt) -> bool {
            match &stmt.kind {
                StmtKind::Assign {
                    target: AssignTarget::Attr { value, .. },
                    ..
                } => matches!(&value.kind, ExprKind::Name(n) if n == "self"),
                StmtKind::Assign { .. } => false,
                StmtKind::If { body, orelse, .. } => {
                    body.iter().any(stmt_mutates) || orelse.iter().any(stmt_mutates)
                }
                StmtKind::While { body, .. } => body.iter().any(stmt_mutates),
                StmtKind::For { body, .. } => body.iter().any(stmt_mutates),
                StmtKind::Match { cases, .. } => {
                    cases.iter().any(|c| c.body.iter().any(stmt_mutates))
                }
                _ => false,
            }
        }
        func.body.iter().any(stmt_mutates)
    }

    fn ends_with_return(&self, stmts: &[Stmt]) -> bool {
        if let Some(last) = stmts.last() {
            match &last.kind {
                StmtKind::Return { .. } | StmtKind::Raise { .. } => true,
                // Try with value return ends with return (all match branches return).
                StmtKind::Try { body, handlers, .. } => {
                    // If try body has return with value and handlers have return, it ends with return.
                    let body_has_return = self.has_value_return(body);
                    let handlers_return = handlers.iter().all(|h| self.ends_with_return(&h.body));
                    body_has_return && handlers_return
                }
                _ => false,
            }
        } else {
            false
        }
    }

    fn has_value_return(&self, stmts: &[Stmt]) -> bool {
        for stmt in stmts {
            match &stmt.kind {
                StmtKind::Return { value: Some(expr) } => {
                    if let Some(ty) = &expr.ty {
                        if !matches!(ty, Type::None) {
                            return true;
                        }
                    }
                }
                StmtKind::If { body, orelse, .. } => {
                    if self.has_value_return(body) || self.has_value_return(orelse) {
                        return true;
                    }
                }
                StmtKind::While { body, .. } | StmtKind::For { body, .. } => {
                    if self.has_value_return(body) {
                        return true;
                    }
                }
                _ => {}
            }
        }
        false
    }
}

fn identity_return_param(func: &Function) -> Option<String> {
    if func.body.len() != 1 {
        return None;
    }
    match &func.body[0].kind {
        StmtKind::Return { value: Some(expr) } => {
            if let ExprKind::Name(name) = &expr.kind {
                return Some(name.clone());
            }
            None
        }
        _ => None,
    }
}
