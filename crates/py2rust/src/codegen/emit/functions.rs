// Function and main emission plus signature helpers.

use super::super::util::{
    collect_assign_counts, collect_assign_counts_for_stmt_refs, mut_kw_for_param,
};
use super::super::*;
use std::collections::{HashMap, HashSet};

impl<'a> Codegen<'a> {
    /// Emit a function or method body.
    pub(crate) fn emit_function(
        &mut self,
        func: &Function,
        class: Option<&ClassDef>,
    ) -> Result<(), CompileError> {
        // Generator functions are emitted via a dedicated state wrapper that
        // supports iteration plus `.send(...)` / `.close()`.
        let is_generator = if let Some(class_def) = class {
            self.ctx
                .classes
                .get(&class_def.name)
                .and_then(|info| info.methods.get(&func.name))
                .is_some_and(|sig| sig.is_generator)
        } else {
            self.ctx
                .functions
                .get(&func.name)
                .is_some_and(|sig| sig.is_generator)
        };
        if is_generator {
            if class.is_some() {
                return Err(self.error(
                    func.span,
                    "Generator methods are not supported in this release",
                ));
            }
            return self.emit_generator_function(func);
        }

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

        // Seed refined callable types discovered by typecheck on call sites.
        // This is especially important for nested decorator patterns where the
        // declaration annotation may stay unknown, but a later call carries a
        // concrete callable shape on `call.func.ty`.
        let mut local_let_names: HashSet<String> = HashSet::new();
        for stmt in &func.body {
            if let StmtKind::Let { name, .. } = &stmt.kind {
                local_let_names.insert(name.clone());
            }
        }
        for stmt in &func.body {
            let StmtKind::Let { value, .. } = &stmt.kind else {
                continue;
            };
            let ExprKind::Call {
                func: call_func, ..
            } = &value.kind
            else {
                continue;
            };
            let ExprKind::Name(callee_name) = &call_func.kind else {
                continue;
            };
            if !local_let_names.contains(callee_name) {
                continue;
            }
            if let Some(ty @ Type::Lambda { .. }) = call_func.ty.clone() {
                self.set_local_var_type(callee_name, ty);
            }
        }

        // Precompute nonlocal declarations and cell-backed locals for this scope.
        let param_names: Vec<String> = func.params.iter().map(|p| p.name.clone()).collect();
        let nonlocal_info = self.collect_nonlocal_info_for_stmts(&func.body, &param_names);
        self.nonlocal_decls = Some(nonlocal_info.nonlocal_decls);
        self.cell_locals = Some(nonlocal_info.cell_locals);

        let classmethod_alias = class.and_then(|class_def| {
            let kind = self
                .ctx
                .classes
                .get(&class_def.name)
                .and_then(|info| info.method_kinds.get(&func.name))
                .copied()
                .unwrap_or(MethodKind::Instance);
            if matches!(kind, MethodKind::Class) {
                func.params
                    .first()
                    .map(|cls_param| (cls_param.name.clone(), class_def.name.clone()))
            } else {
                None
            }
        });
        if let Some((cls_name, class_name)) = &classmethod_alias {
            // CPython-compat divergence:
            // We do not model Python class objects as first-class runtime values for
            // classmethod `cls` parameters. Instead, we alias `cls` to the concrete
            // class name during codegen so class-attribute reads keep working.
            self.name_overrides
                .push((cls_name.clone(), class_name.clone()));
        }

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
        // Precompute dict key/value hints for this function.
        self.inferred_dict_kv = Some(self.collect_dict_kv_types_for_stmts(&func.body));
        // Precompute list storage strategy for this function's locals.
        self.local_list_storage =
            Some(self.collect_list_storage_for_stmts(&func.body, &self.shared_globals));
        // Precompute dict storage strategy for this function's locals.
        self.local_dict_storage =
            Some(self.collect_dict_storage_for_stmts(&func.body, &self.shared_globals));
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
        if classmethod_alias.is_some() {
            self.name_overrides.pop();
        }
        self.local_vars = None;
        self.nonlocal_decls = None;
        self.cell_locals = None;
        self.inferred_list_elems = None;
        self.inferred_dict_kv = None;
        self.local_list_storage = None;
        self.local_dict_storage = None;
        Ok(())
    }

    /// Emit the top-level `main` function.
    pub(crate) fn emit_main(
        &mut self,
        program: &Program,
        body: &[&Stmt],
    ) -> Result<(), CompileError> {
        // Track top-level locals so reassignments can reuse declared types.
        self.local_vars = Some(HashMap::new());
        // Top-level code has no nonlocal bindings.
        self.nonlocal_decls = None;
        self.cell_locals = None;
        // Check if top-level contains exception handling.
        let top_level_can_throw = self.analyze_top_level_throws_refs(body);
        self.top_level_can_throw = top_level_can_throw;

        if top_level_can_throw {
            // Wrap in try closure that catches errors.
            self.push_line("fn main() {");
            self.indent += 1;
            self.push_line("let _result = (|| -> Result<(), PyError> {");
            self.indent += 1;

            // Initialize defaults and class attributes before running top-level code.
            self.emit_pre_main_inits(program)?;
            let mut_counts = collect_assign_counts_for_stmt_refs(body);
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
            let mut_counts = collect_assign_counts_for_stmt_refs(body);
            for stmt in body {
                self.emit_stmt(stmt, &mut_counts)?;
            }
            self.indent -= 1;
            self.push_line("}");
        }

        self.local_vars = None;
        Ok(())
    }

    fn function_signature(&mut self, func: &Function) -> Result<String, CompileError> {
        // Clear borrowed params from previous function.
        self.borrowed_params.clear();
        let mut_counts = collect_assign_counts(&func.body);

        let inferred_param_types = self
            .ctx
            .functions
            .get(&func.name)
            .filter(|sig| sig.params.len() == func.params.len())
            .map(|sig| sig.params.clone());
        let mut params = Vec::new();
        let mut generics: Vec<String> = Vec::new();
        let mut param_types: HashMap<String, String> = HashMap::new();
        let mut generic_idx = 0usize;
        for (idx, param) in func.params.iter().enumerate() {
            let ty = inferred_param_types
                .as_ref()
                .and_then(|types| types.get(idx))
                .cloned()
                .unwrap_or(self.resolve_decl_param_type(param)?);
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
            let mut_kw = mut_kw_for_param(&param.name, &mut_counts);
            params.push(format!("{}{}: {}", mut_kw, param.name, ty_str));
        }

        // Get the return type from context (already wrapped in Result if can_throw).
        let ret_ty = if let Some(sig) = self.ctx.functions.get(&func.name) {
            sig.ret.clone()
        } else {
            self.resolve_type_ref(&func.ret, func.span)?
        };
        let is_generator = self
            .ctx
            .functions
            .get(&func.name)
            .is_some_and(|sig| sig.is_generator);

        let mut ret_str = if is_generator {
            format!("__PyGen_{}", func.name)
        } else if matches!(ret_ty, Type::Unknown) {
            "()".to_string()
        } else {
            self.rust_type(&ret_ty)
        };
        if !is_generator && matches!(ret_ty, Type::Unknown) {
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
    /// - dict[K,V] -> Arc<Mutex<IndexMap<K,V>>> (shared dict, no borrowing)
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
            // Import bindings are compile-time only and should never be borrowed at runtime.
            Type::Module(_) | Type::StdlibFunction { .. } => ty.clone(),
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
        let mut_counts = collect_assign_counts(&func.body);

        let mut params = Vec::new();
        let kind = self
            .ctx
            .classes
            .get(&class.name)
            .and_then(|info| info.method_kinds.get(&func.name))
            .copied()
            .unwrap_or(MethodKind::Instance);
        let inferred_param_types = self
            .ctx
            .classes
            .get(&class.name)
            .and_then(|info| info.methods.get(&func.name))
            .filter(|sig| sig.params.len() == func.params.len())
            .map(|sig| sig.params.clone());
        let mut start_idx = 0usize;
        if matches!(kind, MethodKind::Instance) {
            if let Some(self_param) = func.params.first() {
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
            start_idx = 1;
        } else if matches!(kind, MethodKind::Class) && !func.params.is_empty() {
            // CPython-compat divergence:
            // We drop runtime `cls` parameters from emitted classmethod signatures and
            // alias uses of `cls` to the concrete class symbol during body emission.
            start_idx = 1;
        }
        for (idx, param) in func.params.iter().enumerate().skip(start_idx) {
            let ty = inferred_param_types
                .as_ref()
                .and_then(|types| types.get(idx))
                .cloned()
                .unwrap_or(self.resolve_decl_param_type(param)?);
            let ty_str = if func.name == "__exit__" && matches!(ty, Type::Unknown) {
                "i64".to_string()
            } else if matches!(ty, Type::Unknown) {
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
            let mut_kw = mut_kw_for_param(&param.name, &mut_counts);
            params.push(format!("{}{}: {}", mut_kw, param.name, ty_str));
        }
        let ret_ty = self
            .ctx
            .classes
            .get(&class.name)
            .and_then(|info| info.methods.get(&func.name))
            .map(|sig| sig.ret.clone())
            .unwrap_or(self.resolve_type_ref(&func.ret, func.span)?);
        let ret_str = if matches!(ret_ty, Type::Unknown) {
            "()".to_string()
        } else {
            self.rust_type(&ret_ty)
        };
        Ok(format!("({}) -> {}", params.join(", "), ret_str))
    }

    /// Resolve a parameter declaration type for Rust signatures.
    ///
    /// Variadic annotations are element/value annotations in Python, so we
    /// wrap them into concrete collection parameter types in Rust.
    fn resolve_decl_param_type(&self, param: &Param) -> Result<Type, CompileError> {
        let base = self.resolve_type_ref(&param.ann, param.span)?;
        Ok(match param.kind {
            ParamKind::PositionalOnly | ParamKind::PositionalOrKeyword | ParamKind::KeywordOnly => {
                base
            }
            ParamKind::VarArgs => Type::List(Box::new(base)),
            ParamKind::VarKeywords => Type::Dict(Box::new(Type::Str), Box::new(base)),
        })
    }

    pub(crate) fn method_is_mutating(&self, func: &Function) -> bool {
        fn stmt_mutates(stmt: &Stmt) -> bool {
            match &stmt.kind {
                StmtKind::Assign { target, .. } => {
                    matches!(
                        target.as_ref(),
                        AssignTarget::Attr { value, .. }
                            if matches!(&value.kind, ExprKind::Name(n) if n == "self")
                    )
                }
                StmtKind::Delete { target } => {
                    matches!(
                        target.as_ref(),
                        AssignTarget::Attr { value, .. }
                            if matches!(&value.kind, ExprKind::Name(n) if n == "self")
                    )
                }
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

    /// Emit a generator function as a replay-based iterator wrapper.
    ///
    /// The generated wrapper records resume values (`next` => `None`, `send(v)` => `Some(v)`)
    /// and replays the function body to reconstruct yielded items deterministically.
    /// This keeps generator semantics available without introducing a separate runtime crate.
    fn emit_generator_function(&mut self, func: &Function) -> Result<(), CompileError> {
        let sig = self
            .ctx
            .functions
            .get(&func.name)
            .ok_or_else(|| self.error(func.span, format!("Unknown function: {}", func.name)))?;
        let item_ty = match &sig.ret {
            Type::Iterator(inner) => inner.as_ref().clone(),
            _ => {
                return Err(self.error(
                    func.span,
                    "Internal error: generator function missing Iterator[T] return type",
                ))
            }
        };
        let item_ty_str = self.rust_type(&item_ty);
        let struct_name = format!("__PyGen_{}", func.name);

        let mut param_types = Vec::with_capacity(func.params.len());
        for param in &func.params {
            let ty = self.resolve_decl_param_type(param)?;
            let ty_str = if matches!(ty, Type::Unknown) {
                "()".to_string()
            } else {
                self.rust_type(&ty)
            };
            param_types.push((param.name.clone(), ty, ty_str));
        }

        self.push_line("#[derive(Clone)]");
        self.push_line(&format!("pub struct {} {{", struct_name));
        self.indent += 1;
        self.push_line(&format!("__resume_values: Vec<Option<{}>>,", item_ty_str));
        self.push_line("__emitted: usize,");
        self.push_line("__closed: bool,");
        for (name, _, ty_str) in &param_types {
            self.push_line(&format!("{name}: {ty_str},"));
        }
        self.indent -= 1;
        self.push_line("}");
        self.push_line("");

        // Save/restore mutable codegen state while emitting replay body.
        let saved_current_function = self.current_function.clone();
        let saved_current_function_ret = self.current_function_ret.clone();
        let saved_local_vars = self.local_vars.take();
        self.current_function = Some(func.name.clone());
        self.current_function_ret = Some(sig.ret.clone());
        self.local_vars = Some(HashMap::new());
        for (name, ty, _) in &param_types {
            self.set_local_var_type(name, ty.clone());
        }

        self.push_line(&format!("impl {} {{", struct_name));
        self.indent += 1;
        self.push_line(&format!("fn __replay(&self) -> Vec<{}> {{", item_ty_str));
        self.indent += 1;
        self.push_line("let __py_resume_values = &self.__resume_values;");
        self.push_line("let mut __py_yield_index: usize = 0;");
        self.push_line(&format!(
            "let mut __py_yields: Vec<{}> = Vec::new();",
            item_ty_str
        ));
        for (name, ty, _) in &param_types {
            if self.is_copy_type(ty) {
                self.push_line(&format!("let mut {name} = self.{name};"));
            } else {
                self.push_line(&format!("let mut {name} = self.{name}.clone();"));
            }
        }
        for stmt in &func.body {
            self.emit_generator_replay_stmt(stmt)?;
        }
        self.push_line("__py_yields");
        self.indent -= 1;
        self.push_line("}");
        self.push_line("");

        self.push_line(&format!(
            "fn send(&mut self, value: {}) -> {} {{",
            item_ty_str, item_ty_str
        ));
        self.indent += 1;
        self.push_line("if self.__emitted == 0 {");
        self.indent += 1;
        self.push_line(
            "panic!(\"TypeError: can't send non-None value to a just-started generator\");",
        );
        self.indent -= 1;
        self.push_line("}");
        self.push_line("self.__resume_values.push(Some(value));");
        self.push_line("match self.next() {");
        self.indent += 1;
        self.push_line("Some(v) => v,");
        self.push_line("None => panic!(\"StopIteration\"),");
        self.indent -= 1;
        self.push_line("}");
        self.indent -= 1;
        self.push_line("}");
        self.push_line("");

        self.push_line("fn close(&mut self) {");
        self.indent += 1;
        self.push_line("self.__closed = true;");
        self.indent -= 1;
        self.push_line("}");
        self.indent -= 1;
        self.push_line("}");
        self.push_line("");

        self.push_line(&format!("impl Iterator for {} {{", struct_name));
        self.indent += 1;
        self.push_line(&format!("type Item = {};", item_ty_str));
        self.push_line("fn next(&mut self) -> Option<Self::Item> {");
        self.indent += 1;
        self.push_line("if self.__closed {");
        self.indent += 1;
        self.push_line("return None;");
        self.indent -= 1;
        self.push_line("}");
        self.push_line("if self.__emitted > 0 && self.__resume_values.len() < self.__emitted {");
        self.indent += 1;
        self.push_line("self.__resume_values.push(None);");
        self.indent -= 1;
        self.push_line("}");
        self.push_line("let __py_yields = self.__replay();");
        self.push_line("if self.__emitted < __py_yields.len() {");
        self.indent += 1;
        self.push_line("let __py_out = __py_yields[self.__emitted].clone();");
        self.push_line("self.__emitted += 1;");
        self.push_line("Some(__py_out)");
        self.indent -= 1;
        self.push_line("} else {");
        self.indent += 1;
        self.push_line("None");
        self.indent -= 1;
        self.push_line("}");
        self.indent -= 1;
        self.push_line("}");
        self.indent -= 1;
        self.push_line("}");
        self.push_line("");

        let ctor_params = param_types
            .iter()
            .map(|(name, _, ty_str)| format!("{name}: {ty_str}"))
            .collect::<Vec<_>>()
            .join(", ");
        self.push_line(&format!(
            "pub fn {}({}) -> {} {{",
            func.name, ctor_params, struct_name
        ));
        self.indent += 1;
        self.push_line(&format!("{} {{", struct_name));
        self.indent += 1;
        self.push_line("__resume_values: Vec::new(),");
        self.push_line("__emitted: 0,");
        self.push_line("__closed: false,");
        for (name, _, _) in &param_types {
            self.push_line(&format!("{name},"));
        }
        self.indent -= 1;
        self.push_line("}");
        self.indent -= 1;
        self.push_line("}");
        self.push_line("");

        self.current_function = saved_current_function;
        self.current_function_ret = saved_current_function_ret;
        self.local_vars = saved_local_vars;
        Ok(())
    }

    /// Emit one replay-body statement used by generated generator wrappers.
    fn emit_generator_replay_stmt(&mut self, stmt: &Stmt) -> Result<(), CompileError> {
        match &stmt.kind {
            StmtKind::Let { name, value, .. } => {
                if let ExprKind::Yield { value: yield_value } = &value.kind {
                    self.emit_generator_yield(
                        yield_value.as_deref(),
                        Some((name.as_str(), true)),
                        stmt.span,
                    )?;
                    if let Some(ty) = value.ty.clone() {
                        self.set_local_var_type(name, ty);
                    }
                } else {
                    let expr = self.gen_expr(value)?;
                    self.push_line(&format!("let mut {name} = {expr};"));
                    if let Some(ty) = value.ty.clone() {
                        self.set_local_var_type(name, ty);
                    }
                }
            }
            StmtKind::Assign { target, value } => {
                if let AssignTarget::Name(name) = target.as_ref() {
                    if let ExprKind::Yield { value: yield_value } = &value.kind {
                        self.emit_generator_yield(
                            yield_value.as_deref(),
                            Some((name.as_str(), false)),
                            stmt.span,
                        )?;
                    } else {
                        let expr = self.gen_expr(value)?;
                        self.push_line(&format!("{name} = {expr};"));
                    }
                } else {
                    return Err(self.error(
                        stmt.span,
                        "Unsupported assignment target inside generator function",
                    ));
                }
            }
            StmtKind::Expr(expr) => {
                if let ExprKind::Yield { value } = &expr.kind {
                    self.emit_generator_yield(value.as_deref(), None, expr.span)?;
                } else {
                    let rendered = self.gen_expr(expr)?;
                    self.push_line(&format!("{};", rendered));
                }
            }
            StmtKind::While { test, body } => {
                let test_expr = self.gen_expr(test)?;
                self.push_line(&format!("while {} {{", test_expr));
                self.indent += 1;
                for stmt in body {
                    self.emit_generator_replay_stmt(stmt)?;
                }
                self.indent -= 1;
                self.push_line("}");
            }
            StmtKind::If { test, body, orelse } => {
                let test_expr = self.gen_expr(test)?;
                self.push_line(&format!("if {} {{", test_expr));
                self.indent += 1;
                for stmt in body {
                    self.emit_generator_replay_stmt(stmt)?;
                }
                self.indent -= 1;
                self.push_line("} else {");
                self.indent += 1;
                for stmt in orelse {
                    self.emit_generator_replay_stmt(stmt)?;
                }
                self.indent -= 1;
                self.push_line("}");
            }
            StmtKind::Return { .. } => {
                self.push_line("return __py_yields;");
            }
            StmtKind::Break => self.push_line("break;"),
            StmtKind::Continue => self.push_line("continue;"),
            _ => {
                return Err(self.error(stmt.span, "Unsupported statement inside generator function"))
            }
        }
        Ok(())
    }

    /// Emit replay logic for a `yield` site.
    fn emit_generator_yield(
        &mut self,
        yielded_value: Option<&Expr>,
        assign_to: Option<(&str, bool)>,
        span: Span,
    ) -> Result<(), CompileError> {
        let yielded_expr = if let Some(value) = yielded_value {
            self.gen_expr(value)?
        } else {
            "()".to_string()
        };
        self.push_line(&format!("__py_yields.push(({}).clone());", yielded_expr));
        self.push_line("if __py_yield_index >= __py_resume_values.len() {");
        self.indent += 1;
        self.push_line("return __py_yields;");
        self.indent -= 1;
        self.push_line("}");
        self.push_line("let __py_resume = __py_resume_values[__py_yield_index].clone();");
        self.push_line("__py_yield_index += 1;");

        if let Some((name, declare)) = assign_to {
            let binding = if declare { "let mut " } else { "" };
            self.push_line(&format!(
                "{}{} = match __py_resume {{ Some(v) => v, None => panic!(\"generator send(None) is not supported for this yield site\") }};",
                binding, name
            ));
        }

        // Keep a span-aware touchpoint for unsupported future extensions.
        if yielded_value.is_none() && assign_to.is_some() {
            return Err(self.error(
                span,
                "Unsupported bare-yield assignment inside generator function",
            ));
        }
        Ok(())
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
