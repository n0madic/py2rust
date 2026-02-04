use super::util::collect_assign_counts;
use super::*;

impl<'a> Codegen<'a> {
    pub(crate) fn emit_header(&mut self) {
        self.push_line("#![allow(unused)]");
        if self.uses.hash_map {
            self.push_line("use std::collections::HashMap;");
        }
        if self.uses.hash_set {
            self.push_line("use std::collections::HashSet;");
        }
        if !self.ctx.globals.is_empty() {
            self.push_line("use std::sync::{Mutex, OnceLock};");
        }
        self.push_line("const __NAME__: &str = \"__main__\";");
        self.push_line("");
    }

    pub(crate) fn emit_helpers(&mut self) {
        // Emit PyError if any function throws
        if self.needs_py_error() {
            self.emit_py_error_enum();
        }

        if self.uses.print {
            self.push_line("fn py_print<T: std::fmt::Display>(v: T) {");
            self.indent += 1;
            self.push_line("println!(\"{v}\");");
            self.indent -= 1;
            self.push_line("}");
        }
        if self.uses.len {
            self.push_line("trait PyLen {");
            self.indent += 1;
            self.push_line("fn py_len(&self) -> i64;");
            self.indent -= 1;
            self.push_line("}");
            self.push_line("impl<T> PyLen for Vec<T> {");
            self.indent += 1;
            self.push_line("fn py_len(&self) -> i64 { self.len() as i64 }");
            self.indent -= 1;
            self.push_line("}");
            self.push_line("impl PyLen for String {");
            self.indent += 1;
            self.push_line("fn py_len(&self) -> i64 { self.len() as i64 }");
            self.indent -= 1;
            self.push_line("}");
            self.push_line("impl PyLen for &str {");
            self.indent += 1;
            self.push_line("fn py_len(&self) -> i64 { self.len() as i64 }");
            self.indent -= 1;
            self.push_line("}");
            self.push_line("impl<K, V> PyLen for std::collections::HashMap<K, V> {");
            self.indent += 1;
            self.push_line("fn py_len(&self) -> i64 { self.len() as i64 }");
            self.indent -= 1;
            self.push_line("}");
            self.push_line("impl<T> PyLen for std::collections::HashSet<T> {");
            self.indent += 1;
            self.push_line("fn py_len(&self) -> i64 { self.len() as i64 }");
            self.indent -= 1;
            self.push_line("}");
            self.push_line("fn py_len<T: PyLen>(v: &T) -> i64 { v.py_len() }");
        }
        if self.uses.range {
            self.push_line("fn py_range(end: i64) -> std::ops::Range<i64> { 0..end }");
        }
        if self.uses.range2 {
            self.push_line(
                "fn py_range2(start: i64, end: i64) -> std::ops::Range<i64> { start..end }",
            );
        }
        if self.uses.round {
            self.push_line("fn py_round(value: f64, ndigits: i64) -> f64 {");
            self.indent += 1;
            self.push_line("let factor = 10f64.powi(ndigits as i32);");
            self.push_line("(value * factor).round() / factor");
            self.indent -= 1;
            self.push_line("}");
        }
        if self.uses.type_name {
            self.push_line("fn py_type_name<T: ?Sized>(value: &T) -> String {");
            self.indent += 1;
            self.push_line("std::any::type_name_of_val(value).to_string()");
            self.indent -= 1;
            self.push_line("}");
        }
        if self.uses.py_max {
            self.push_line("fn py_max<T: Ord, I: IntoIterator<Item = T>>(iter: I) -> T {");
            self.indent += 1;
            self.push_line("iter.into_iter().max().expect(\"max() arg is an empty sequence\")");
            self.indent -= 1;
            self.push_line("}");
        }
        if self.uses.py_min {
            self.push_line("fn py_min<T: Ord, I: IntoIterator<Item = T>>(iter: I) -> T {");
            self.indent += 1;
            self.push_line("iter.into_iter().min().expect(\"min() arg is an empty sequence\")");
            self.indent -= 1;
            self.push_line("}");
        }
        if self.uses.py_parse_int {
            self.push_line("fn py_parse_int(s: &str) -> i64 {");
            self.indent += 1;
            self.push_line("s.trim().parse().unwrap_or_else(|_| panic!(\"invalid literal for int() with base 10: '{}'\", s.trim()))");
            self.indent -= 1;
            self.push_line("}");
        }
        if self.uses.py_parse_float {
            self.push_line("fn py_parse_float(s: &str) -> f64 {");
            self.indent += 1;
            self.push_line("s.trim().parse().unwrap_or_else(|_| panic!(\"could not convert string to float: '{}'\", s.trim()))");
            self.indent -= 1;
            self.push_line("}");
        }
        if self.uses.py_index {
            self.push_line("fn py_index(idx: i64, len: usize) -> usize {");
            self.indent += 1;
            self.push_line("if idx >= 0 { idx as usize } else { (len as i64 + idx) as usize }");
            self.indent -= 1;
            self.push_line("}");
        }
        if self.uses.py_str_slice {
            self.push_line(
                "fn py_str_slice(s: &str, start: Option<i64>, end: Option<i64>) -> String {",
            );
            self.indent += 1;
            self.push_line("let chars: Vec<char> = s.chars().collect();");
            self.push_line("let len = chars.len() as i64;");
            self.push_line("let start = start.map(|i| if i < 0 { (len + i).max(0) } else { i.min(len) }).unwrap_or(0) as usize;");
            self.push_line("let end = end.map(|i| if i < 0 { (len + i).max(0) } else { i.min(len) }).unwrap_or(len as i64) as usize;");
            self.push_line("chars[start..end.max(start)].iter().collect()");
            self.indent -= 1;
            self.push_line("}");
        }
        if self.uses.print
            || self.uses.len
            || self.uses.range
            || self.uses.range2
            || self.uses.round
            || self.uses.type_name
            || self.uses.py_max
            || self.uses.py_min
            || self.uses.py_parse_int
            || self.uses.py_parse_float
            || self.uses.py_index
            || self.uses.py_str_slice
        {
            self.push_line("");
        }
    }

    pub(crate) fn emit_globals(&mut self) {
        if self.ctx.globals.is_empty() {
            return;
        }
        for (name, ty) in &self.ctx.globals {
            let ty_str = self.rust_type(ty);
            let gname = self.global_name(name);
            self.push_line(&format!(
                "static {}: OnceLock<Mutex<{}>> = OnceLock::new();",
                gname, ty_str
            ));
        }
        self.push_line("");
    }

    pub(crate) fn emit_union(&mut self, def: &UnionDef) -> Result<(), CompileError> {
        self.push_line("#[derive(Debug, Clone)]");
        self.push_line(&format!("pub enum {} {{", def.name));
        self.indent += 1;
        for variant in &def.variants {
            self.push_line(&format!("{}({}),", variant, variant));
        }
        self.indent -= 1;
        self.push_line("}");
        self.push_line("");
        Ok(())
    }

    pub(crate) fn emit_class(&mut self, class_def: &ClassDef) -> Result<(), CompileError> {
        let class_info = self.ctx.classes.get(&class_def.name).ok_or_else(|| {
            self.error(class_def.span, format!("Unknown class: {}", class_def.name))
        })?;

        self.push_line("#[derive(Debug, Clone)]");
        self.push_line(&format!("pub struct {} {{", class_def.name));
        self.indent += 1;
        for (field, ty) in &class_info.fields {
            let ty_str = self.rust_type(ty);
            self.push_line(&format!("pub {}: {},", field, ty_str));
        }
        self.indent -= 1;
        self.push_line("}");
        self.push_line("");

        self.push_line(&format!("impl {} {{", class_def.name));
        self.indent += 1;
        if let Some(init) = class_def.methods.iter().find(|m| m.name == "__init__") {
            self.emit_constructor(class_def, init)?;
        } else {
            self.push_line("// no __init__ defined");
        }

        for method in &class_def.methods {
            if method.name == "__init__" {
                continue;
            }
            if method.name == "next" && class_info.next_item.is_some() {
                continue;
            }
            self.emit_function(method, Some(class_def))?;
        }
        self.indent -= 1;
        self.push_line("}");
        self.push_line("");

        if let Some(item_ty) = &class_info.next_item {
            let item_ty = self.rust_type(item_ty);
            self.push_line(&format!("impl Iterator for {} {{", class_def.name));
            self.indent += 1;
            self.push_line(&format!("type Item = {};", item_ty));
            let next_method = class_def
                .methods
                .iter()
                .find(|m| m.name == "next")
                .ok_or_else(|| self.error(class_def.span, "Iterator class missing next method"))?;
            let ret_ty = self.resolve_type_ref(&next_method.ret, next_method.span)?;
            let ret_str = self.rust_type(&ret_ty);
            self.push_line(&format!("fn next(&mut self) -> {} {{", ret_str));
            self.indent += 1;
            let mut_counts = collect_assign_counts(&next_method.body);
            for stmt in &next_method.body {
                self.emit_stmt(stmt, &mut_counts)?;
            }
            self.indent -= 1;
            self.push_line("}");
            self.indent -= 1;
            self.push_line("}");
            self.push_line("");
        }

        if class_info.iter_return.is_some() || class_info.iter_item.is_some() {
            self.emit_into_iter(class_def, class_info)?;
        }

        Ok(())
    }

    fn emit_constructor(
        &mut self,
        class_def: &ClassDef,
        init: &Function,
    ) -> Result<(), CompileError> {
        let mut params = Vec::new();
        for param in init.params.iter().skip(1) {
            let ty = self.resolve_type_ref(&param.ann, param.span)?;
            let ty_str = self.rust_type(&ty);
            params.push(format!("{}: {}", param.name, ty_str));
        }
        let sig = format!("({}) -> {}", params.join(", "), class_def.name);
        self.push_line(&format!("pub fn new{} {{", sig));
        self.indent += 1;
        let mut field_inits: HashMap<String, String> = HashMap::new();
        for stmt in &init.body {
            match &stmt.kind {
                StmtKind::Assign { target, value } => {
                    if let AssignTarget::Attr { value: obj, attr } = target {
                        if matches!(&obj.kind, ExprKind::Name(n) if n == "self") {
                            let expr = self.gen_expr(value)?;
                            field_inits.insert(attr.clone(), expr);
                        } else {
                            return Err(
                                self.error(stmt.span, "__init__ may only assign to self fields")
                            );
                        }
                    } else {
                        return Err(
                            self.error(stmt.span, "__init__ may only assign to self fields")
                        );
                    }
                }
                StmtKind::Expr(expr) => {
                    if matches!(expr.kind, ExprKind::Literal(Literal::None)) {
                        continue;
                    }
                    return Err(
                        self.error(stmt.span, "__init__ may only contain field assignments")
                    );
                }
                _ => {
                    return Err(self.error(stmt.span, "__init__ may only contain field assignments"))
                }
            }
        }
        let class_info = self
            .ctx
            .classes
            .get(&class_def.name)
            .ok_or_else(|| self.error(class_def.span, "Unknown class"))?;
        for field in class_info.fields.keys() {
            if !field_inits.contains_key(field) {
                return Err(self.error(
                    init.span,
                    format!("Field {field} not initialized in __init__"),
                ));
            }
        }
        self.push_line(&format!("{} {{", class_def.name));
        self.indent += 1;
        for (field, _) in &class_info.fields {
            let expr = field_inits.get(field).unwrap();
            self.push_line(&format!("{}: {},", field, expr));
        }
        self.indent -= 1;
        self.push_line("}");
        self.indent -= 1;
        self.push_line("}");
        Ok(())
    }

    fn emit_into_iter(
        &mut self,
        class_def: &ClassDef,
        info: &ClassInfo,
    ) -> Result<(), CompileError> {
        if let Some(iter_return) = &info.iter_return {
            let iter_info = self.ctx.classes.get(iter_return).ok_or_else(|| {
                self.error(
                    class_def.span,
                    format!("Unknown iterator class: {iter_return}"),
                )
            })?;
            let item_ty = iter_info
                .next_item
                .as_ref()
                .ok_or_else(|| self.error(class_def.span, "Iterator class missing next()"))?;
            let item_ty = self.rust_type(item_ty);
            self.push_line(&format!("impl IntoIterator for {} {{", class_def.name));
            self.indent += 1;
            self.push_line(&format!("type Item = {};", item_ty));
            self.push_line(&format!("type IntoIter = {};", iter_return));
            self.push_line("fn into_iter(self) -> Self::IntoIter {");
            self.indent += 1;
            self.push_line("self.__iter__()");
            self.indent -= 1;
            self.push_line("}");
            self.indent -= 1;
            self.push_line("}");
            self.push_line("");
        } else if let Some(item_ty) = &info.iter_item {
            let item_ty = self.rust_type(item_ty);
            self.push_line(&format!("impl IntoIterator for {} {{", class_def.name));
            self.indent += 1;
            self.push_line(&format!("type Item = {};", item_ty));
            self.push_line("type IntoIter = Box<dyn Iterator<Item = Self::Item>>;");
            self.push_line("fn into_iter(self) -> Self::IntoIter {");
            self.indent += 1;
            self.push_line("Box::new(self.__iter__())");
            self.indent -= 1;
            self.push_line("}");
            self.indent -= 1;
            self.push_line("}");
            self.push_line("");
        }
        Ok(())
    }

    pub(crate) fn emit_function(
        &mut self,
        func: &Function,
        class: Option<&ClassDef>,
    ) -> Result<(), CompileError> {
        // Set current function for tracking throws
        self.current_function = Some(func.name.clone());

        let sig = if let Some(class) = class {
            self.method_signature(func, class)?
        } else {
            self.function_signature(func)?
        };
        let vis = "pub ";
        self.push_line(&format!("{}fn {}{} {{", vis, func.name, sig));
        self.indent += 1;
        let mut_counts = collect_assign_counts(&func.body);
        for stmt in &func.body {
            self.emit_stmt(stmt, &mut_counts)?;
        }

        // If function can throw and doesn't end with explicit return, add Ok(())
        let can_throw = self
            .ctx
            .functions
            .get(&func.name)
            .map_or(false, |s| s.can_throw);
        if can_throw && !self.ends_with_return(&func.body) {
            self.push_line("Ok(())");
        }

        self.indent -= 1;
        self.push_line("}");
        self.push_line("");

        // Clear current function
        self.current_function = None;
        Ok(())
    }

    pub(crate) fn emit_main(&mut self, body: &[Stmt]) -> Result<(), CompileError> {
        // Check if top-level contains exception handling
        let top_level_can_throw = self.analyze_top_level_throws(body);

        if top_level_can_throw {
            // Wrap in try closure that catches errors
            self.push_line("fn main() {");
            self.indent += 1;
            self.push_line("let _result = (|| -> Result<(), PyError> {");
            self.indent += 1;

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
            // Normal main
            self.push_line("fn main() {");
            self.indent += 1;
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
        // Clear borrowed params from previous function
        self.borrowed_params.clear();

        let mut params = Vec::new();
        for param in &func.params {
            let ty = self.resolve_type_ref(&param.ann, param.span)?;
            let ty_str = if matches!(ty, Type::Unknown) {
                "()".to_string()
            } else {
                // Convert to borrowed type for function parameters
                let borrowed = self.to_borrowed_param_type(&ty);
                // Track if this parameter is borrowed
                if self.is_borrowed_type(&borrowed) {
                    self.borrowed_params.insert(param.name.clone());
                }
                self.rust_type(&borrowed)
            };
            params.push(format!("{}: {}", param.name, ty_str));
        }

        // Get the return type from context (already wrapped in Result if can_throw)
        let ret_ty = if let Some(sig) = self.ctx.functions.get(&func.name) {
            sig.ret.clone()
        } else {
            self.resolve_type_ref(&func.ret, func.span)?
        };

        let ret_str = if matches!(ret_ty, Type::Unknown) {
            "()".to_string()
        } else {
            self.rust_type(&ret_ty)
        };
        Ok(format!("({}) -> {}", params.join(", "), ret_str))
    }

    /// Check if a type is a borrowed/reference type
    fn is_borrowed_type(&self, ty: &Type) -> bool {
        matches!(ty, Type::Ref(_) | Type::MutRef(_) | Type::Slice(_))
    }

    /// Convert a type to its borrowed equivalent for function parameters.
    /// - list[T] -> &[T] (slice)
    /// - str -> &str
    /// - dict[K,V] -> &HashMap<K,V>
    /// - Primitives (int, float, bool) stay owned (Copy types)
    pub(crate) fn to_borrowed_param_type(&self, ty: &Type) -> Type {
        match ty {
            // Copy types stay as-is
            Type::Int | Type::Float | Type::Bool | Type::None => ty.clone(),
            // String -> &str
            Type::Str => Type::Ref(Box::new(Type::Str)),
            // Vec<T> -> &[T]
            Type::List(inner) => Type::Slice(inner.clone()),
            // HashMap/HashSet -> borrowed reference
            Type::Dict(k, v) => Type::Ref(Box::new(Type::Dict(k.clone(), v.clone()))),
            Type::Set(inner) => Type::Ref(Box::new(Type::Set(inner.clone()))),
            // Tuples stay owned (they can contain Copy types or be small)
            Type::Tuple(_) => ty.clone(),
            // Option stays owned
            Type::Option(_) => ty.clone(),
            // Custom/Union types get borrowed
            Type::Custom(name) => Type::Ref(Box::new(Type::Custom(name.clone()))),
            Type::Union(name) => Type::Ref(Box::new(Type::Union(name.clone()))),
            // Iterator stays as-is
            Type::Iterator(_) => ty.clone(),
            // Lambda stays as-is
            Type::Lambda { .. } => ty.clone(),
            // Reference types stay as-is
            Type::Ref(_) | Type::MutRef(_) | Type::Slice(_) => ty.clone(),
            // Result and Exception stay as-is
            Type::Result(_, _) | Type::Exception(_) => ty.clone(),
            // Unknown stays as-is
            Type::Unknown => ty.clone(),
        }
    }

    fn method_signature(
        &mut self,
        func: &Function,
        class: &ClassDef,
    ) -> Result<String, CompileError> {
        // Clear borrowed params from previous function
        self.borrowed_params.clear();

        let mut params = Vec::new();
        let mut iter = func.params.iter();
        if let Some(self_param) = iter.next() {
            let self_ty = self.resolve_type_ref(&self_param.ann, self_param.span)?;
            let is_mut = self.method_is_mutating(func);
            let receiver = if is_mut { "&mut self" } else { "&self" };
            params.push(receiver.to_string());
            // self is always a borrowed reference in methods
            self.borrowed_params.insert(self_param.name.clone());
            if let Type::Custom(name) = self_ty {
                if !class.name.is_empty() && name != class.name {
                    // ignore mismatch
                }
            }
        }
        for param in iter {
            let ty = self.resolve_type_ref(&param.ann, param.span)?;
            let ty_str = if matches!(ty, Type::Unknown) {
                "()".to_string()
            } else {
                // Convert to borrowed type for method parameters
                let borrowed = self.to_borrowed_param_type(&ty);
                // Track if this parameter is borrowed
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

    fn method_is_mutating(&self, func: &Function) -> bool {
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

    fn needs_py_error(&self) -> bool {
        self.ctx.functions.values().any(|sig| sig.can_throw)
    }

    fn emit_py_error_enum(&mut self) {
        self.push_line("#[derive(Debug, Clone)]");
        self.push_line("pub enum PyError {");
        self.indent += 1;

        // Built-in exceptions
        self.push_line("ValueError(String),");
        self.push_line("TypeError(String),");
        self.push_line("RuntimeError(String),");
        self.push_line("KeyError(String),");
        self.push_line("IndexError(String),");
        self.push_line("AttributeError(String),");
        self.push_line("ZeroDivisionError(String),");
        self.push_line("NameError(String),");
        self.push_line("AssertionError(String),");

        self.indent -= 1;
        self.push_line("}");
        self.push_line("");

        // Implement Display
        self.push_line("impl std::fmt::Display for PyError {");
        self.indent += 1;
        self.push_line("fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {");
        self.indent += 1;
        self.push_line("match self {");
        self.indent += 1;
        self.push_line("PyError::ValueError(msg) => write!(f, \"ValueError: {}\", msg),");
        self.push_line("PyError::TypeError(msg) => write!(f, \"TypeError: {}\", msg),");
        self.push_line("PyError::RuntimeError(msg) => write!(f, \"RuntimeError: {}\", msg),");
        self.push_line("PyError::KeyError(msg) => write!(f, \"KeyError: {}\", msg),");
        self.push_line("PyError::IndexError(msg) => write!(f, \"IndexError: {}\", msg),");
        self.push_line("PyError::AttributeError(msg) => write!(f, \"AttributeError: {}\", msg),");
        self.push_line(
            "PyError::ZeroDivisionError(msg) => write!(f, \"ZeroDivisionError: {}\", msg),",
        );
        self.push_line("PyError::NameError(msg) => write!(f, \"NameError: {}\", msg),");
        self.push_line("PyError::AssertionError(msg) => write!(f, \"AssertionError: {}\", msg),");
        self.indent -= 1;
        self.push_line("}");
        self.indent -= 1;
        self.push_line("}");
        self.indent -= 1;
        self.push_line("}");
        self.push_line("");

        // Implement std::error::Error
        self.push_line("impl std::error::Error for PyError {}");
        self.push_line("");
    }

    fn ends_with_return(&self, stmts: &[Stmt]) -> bool {
        if let Some(last) = stmts.last() {
            matches!(last.kind, StmtKind::Return { .. } | StmtKind::Raise { .. })
        } else {
            false
        }
    }

    fn analyze_top_level_throws(&self, stmts: &[Stmt]) -> bool {
        for stmt in stmts {
            if self.stmt_can_throw(stmt) {
                return true;
            }
        }
        false
    }

    fn stmt_can_throw(&self, stmt: &Stmt) -> bool {
        match &stmt.kind {
            StmtKind::Raise { .. } => true,
            StmtKind::Try { .. } => true, // Has exception handling
            StmtKind::Expr(expr) => self.expr_can_throw(expr),
            StmtKind::Let { value, .. } | StmtKind::Assign { value, .. } => {
                self.expr_can_throw(value)
            }
            StmtKind::Return { value } => value.as_ref().map_or(false, |e| self.expr_can_throw(e)),
            StmtKind::If { test, body, orelse } => {
                self.expr_can_throw(test)
                    || body.iter().any(|s| self.stmt_can_throw(s))
                    || orelse.iter().any(|s| self.stmt_can_throw(s))
            }
            StmtKind::While { test, body } => {
                self.expr_can_throw(test) || body.iter().any(|s| self.stmt_can_throw(s))
            }
            StmtKind::For { iter, body, .. } => {
                self.expr_can_throw(iter) || body.iter().any(|s| self.stmt_can_throw(s))
            }
            StmtKind::Match { subject, cases } => {
                self.expr_can_throw(subject)
                    || cases
                        .iter()
                        .any(|c| c.body.iter().any(|s| self.stmt_can_throw(s)))
            }
            _ => false,
        }
    }

    fn expr_can_throw(&self, expr: &Expr) -> bool {
        match &expr.kind {
            ExprKind::Call { func, .. } => {
                if let ExprKind::Name(name) = &func.kind {
                    self.ctx
                        .functions
                        .get(name)
                        .map_or(false, |sig| sig.can_throw)
                } else {
                    false
                }
            }
            ExprKind::Binary { left, right, .. } => {
                self.expr_can_throw(left) || self.expr_can_throw(right)
            }
            ExprKind::Unary { expr, .. } => self.expr_can_throw(expr),
            ExprKind::Compare { left, right, .. } => {
                self.expr_can_throw(left) || self.expr_can_throw(right)
            }
            ExprKind::BoolOp { values, .. } => values.iter().any(|v| self.expr_can_throw(v)),
            ExprKind::List(items) | ExprKind::Tuple(items) | ExprKind::Set(items) => {
                items.iter().any(|e| self.expr_can_throw(e))
            }
            ExprKind::Dict(pairs) => pairs
                .iter()
                .any(|(k, v)| self.expr_can_throw(k) || self.expr_can_throw(v)),
            ExprKind::Index { value, index } => {
                self.expr_can_throw(value) || self.expr_can_throw(index)
            }
            ExprKind::Slice { value, start, end } => {
                self.expr_can_throw(value)
                    || start.as_ref().map_or(false, |s| self.expr_can_throw(s))
                    || end.as_ref().map_or(false, |e| self.expr_can_throw(e))
            }
            ExprKind::ListComp { elt, iter, ifs, .. } => {
                self.expr_can_throw(elt)
                    || self.expr_can_throw(iter)
                    || ifs.iter().any(|i| self.expr_can_throw(i))
            }
            ExprKind::UnionCtor { inner, .. } => self.expr_can_throw(inner),
            ExprKind::Lambda { body, .. } => self.expr_can_throw(body),
            ExprKind::IfExpr { test, body, orelse } => {
                self.expr_can_throw(test)
                    || self.expr_can_throw(body)
                    || self.expr_can_throw(orelse)
            }
            ExprKind::Attr { value, .. } => self.expr_can_throw(value),
            _ => false,
        }
    }
}
