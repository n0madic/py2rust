use crate::diagnostic::CompileError;
use crate::hir::*;
use crate::lower::Lowerer;
use crate::span::Span;
use crate::stdlib::registry::resolve_module;
use crate::types::TypeRef;
use rustpython_parser::ast;
use rustpython_parser::Parse;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

/// Resolve user-module imports and merge imported module HIR into one program.
///
/// This pass keeps stdlib modules (`typing`, `os`, `sys`) as virtual imports,
/// while loading user modules/packages from files and rewriting symbol usage to
/// namespace-safe names.
pub fn resolve_program_imports(
    mut entry_program: Program,
    source: &str,
    filename: &str,
) -> Result<Program, CompileError> {
    let entry_dir = resolve_entry_dir(filename)?;
    let mut resolver = ImportResolver::new(entry_dir);
    resolver.resolve_module_program(&mut entry_program, None, false, source, filename, false)?;

    let mut merged_items = Vec::new();
    for module_name in &resolver.module_order {
        if let Some(ModuleCacheEntry::Loaded(module)) = resolver.modules.get(module_name) {
            merged_items.extend(module.items.clone());
        }
    }
    merged_items.extend(entry_program.items);

    Ok(Program {
        items: merged_items,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SymbolKind {
    Function,
    Class,
    Union,
    Value,
}

#[derive(Debug, Clone)]
struct ExportEntry {
    symbol: String,
    kind: SymbolKind,
}

#[derive(Debug, Clone)]
struct ResolvedModule {
    items: Vec<Item>,
    exports: HashMap<String, ExportEntry>,
}

#[derive(Debug, Clone)]
enum ModuleCacheEntry {
    Loading,
    Loaded(ResolvedModule),
}

struct ImportResolver {
    entry_dir: PathBuf,
    modules: HashMap<String, ModuleCacheEntry>,
    module_order: Vec<String>,
}

impl ImportResolver {
    fn new(entry_dir: PathBuf) -> Self {
        Self {
            entry_dir,
            modules: HashMap::new(),
            module_order: Vec::new(),
        }
    }

    fn resolve_module_program(
        &mut self,
        program: &mut Program,
        current_module: Option<&str>,
        current_is_package: bool,
        source: &str,
        filename: &str,
        namespace_items: bool,
    ) -> Result<HashMap<String, ExportEntry>, CompileError> {
        let mut imported_name_renames: HashMap<String, String> = HashMap::new();
        let mut imported_type_renames: HashMap<String, String> = HashMap::new();
        let mut imported_module_members: HashMap<String, HashMap<String, ExportEntry>> =
            HashMap::new();
        let mut imported_exports: HashMap<String, ExportEntry> = HashMap::new();

        // Scan top-level imports and resolve user modules/packages.
        for item in &program.items {
            let Item::Stmt(stmt) = item else {
                continue;
            };
            match &stmt.kind {
                StmtKind::Import { names } => {
                    for binding in names {
                        if is_virtual_module(binding.module.as_str()) {
                            continue;
                        }

                        let resolved_for_alias = if binding.alias.is_none() {
                            binding
                                .module
                                .split('.')
                                .next()
                                .unwrap_or(binding.module.as_str())
                                .to_string()
                        } else {
                            binding.module.clone()
                        };

                        let target_module = self.load_user_module(
                            resolved_for_alias.as_str(),
                            stmt.span,
                            source,
                            filename,
                        )?;

                        // Keep Python import binding semantics:
                        // - import a.b as c => c
                        // - import a.b     => a
                        let bound_name = binding.alias.clone().unwrap_or_else(|| {
                            binding
                                .module
                                .split('.')
                                .next()
                                .unwrap_or(binding.module.as_str())
                                .to_string()
                        });

                        imported_module_members.insert(bound_name, target_module.exports.clone());
                    }
                }
                StmtKind::ImportFrom { module, names } => {
                    let resolved_module_name = self.resolve_from_module_name(
                        current_module,
                        current_is_package,
                        module.as_str(),
                        stmt.span,
                        source,
                        filename,
                    )?;
                    if is_virtual_module(resolved_module_name.as_str()) {
                        continue;
                    }

                    let target_module = self.load_user_module(
                        resolved_module_name.as_str(),
                        stmt.span,
                        source,
                        filename,
                    )?;

                    for binding in names {
                        if binding.name == "*" {
                            return Err(CompileError::new(
                                "from import * is not supported",
                                stmt.span,
                                source,
                                filename,
                            ));
                        }

                        let bound_name = binding
                            .alias
                            .clone()
                            .unwrap_or_else(|| binding.name.clone());

                        if let Some(export) = target_module.exports.get(binding.name.as_str()) {
                            imported_name_renames.insert(bound_name.clone(), export.symbol.clone());
                            if matches!(export.kind, SymbolKind::Class | SymbolKind::Union) {
                                imported_type_renames
                                    .insert(bound_name.clone(), export.symbol.clone());
                            }
                            imported_exports.insert(bound_name, export.clone());
                            continue;
                        }

                        // Support `from package import submodule`.
                        let submodule_name =
                            format!("{}.{}", resolved_module_name, binding.name.as_str());
                        let submodule = match self.load_user_module(
                            submodule_name.as_str(),
                            stmt.span,
                            source,
                            filename,
                        ) {
                            Ok(module) => module,
                            Err(_) => {
                                return Err(CompileError::new(
                                    format!(
                                        "module '{}' has no member '{}'",
                                        resolved_module_name, binding.name
                                    ),
                                    stmt.span,
                                    source,
                                    filename,
                                ));
                            }
                        };
                        imported_module_members.insert(bound_name, submodule.exports.clone());
                    }
                }
                _ => {}
            }
        }

        let top_level_symbols = collect_top_level_symbols(program);
        let mut target_renames: HashMap<String, String> = HashMap::new();
        let mut local_type_renames: HashMap<String, String> = HashMap::new();
        let mut local_exports: HashMap<String, ExportEntry> = HashMap::new();

        if namespace_items {
            let prefix = module_prefix(current_module.unwrap_or("__module"));
            for (name, kind) in &top_level_symbols {
                let renamed = format!("{prefix}{name}");
                target_renames.insert(name.clone(), renamed.clone());
                if matches!(kind, SymbolKind::Class | SymbolKind::Union) {
                    local_type_renames.insert(name.clone(), renamed.clone());
                }
                local_exports.insert(
                    name.clone(),
                    ExportEntry {
                        symbol: renamed,
                        kind: *kind,
                    },
                );
            }
        } else {
            for (name, kind) in &top_level_symbols {
                local_exports.insert(
                    name.clone(),
                    ExportEntry {
                        symbol: name.clone(),
                        kind: *kind,
                    },
                );
                if matches!(kind, SymbolKind::Class | SymbolKind::Union) {
                    local_type_renames.insert(name.clone(), name.clone());
                }
            }
        }

        let mut expr_renames = imported_name_renames.clone();
        for (name, mapped) in &target_renames {
            expr_renames.insert(name.clone(), mapped.clone());
        }

        let mut type_renames = imported_type_renames;
        for (name, mapped) in &local_type_renames {
            type_renames.insert(name.clone(), mapped.clone());
        }

        let mut rewriter = ModuleRewriter {
            expr_renames,
            type_renames,
            target_renames,
            module_alias_members: imported_module_members,
            scopes: Vec::new(),
            source,
            filename,
        };
        rewriter.rewrite_program(program)?;

        // Imported names are also module attributes in Python.
        let mut exports = imported_exports;
        for (name, export) in local_exports {
            exports.insert(name, export);
        }

        Ok(exports)
    }

    fn load_user_module(
        &mut self,
        module_name: &str,
        span: Span,
        source: &str,
        filename: &str,
    ) -> Result<ResolvedModule, CompileError> {
        if let Some(entry) = self.modules.get(module_name) {
            match entry {
                ModuleCacheEntry::Loading => {
                    return Err(CompileError::new(
                        format!("circular import detected for module '{module_name}'"),
                        span,
                        source,
                        filename,
                    ))
                }
                ModuleCacheEntry::Loaded(module) => return Ok(module.clone()),
            }
        }

        self.modules
            .insert(module_name.to_string(), ModuleCacheEntry::Loading);

        let Some((path, is_package)) = self.resolve_user_module_path(module_name) else {
            return Err(CompileError::new(
                format!("Cannot resolve module '{module_name}'"),
                span,
                source,
                filename,
            ));
        };

        let module_filename = path.to_string_lossy().to_string();
        let module_source = fs::read_to_string(&path).map_err(|err| {
            CompileError::new(
                format!("failed to read {}: {}", path.display(), err),
                span,
                source,
                filename,
            )
        })?;

        let suite = ast::Suite::parse(&module_source, module_filename.as_str()).map_err(|err| {
            CompileError::new(
                err.to_string(),
                Span::new(0, 0),
                &module_source,
                &module_filename,
            )
        })?;

        let mut program = Lowerer::new(&module_source, &module_filename).lower(&suite)?;
        let exports = self.resolve_module_program(
            &mut program,
            Some(module_name),
            is_package,
            &module_source,
            &module_filename,
            true,
        )?;

        let resolved = ResolvedModule {
            items: program.items,
            exports,
        };

        self.modules.insert(
            module_name.to_string(),
            ModuleCacheEntry::Loaded(resolved.clone()),
        );
        self.module_order.push(module_name.to_string());

        Ok(resolved)
    }

    fn resolve_user_module_path(&self, module_name: &str) -> Option<(PathBuf, bool)> {
        if module_name.is_empty() {
            return None;
        }
        let mut relative = PathBuf::new();
        for part in module_name.split('.') {
            relative.push(part);
        }

        let package_init = self.entry_dir.join(relative.clone()).join("__init__.py");
        if package_init.is_file() {
            return Some((package_init, true));
        }

        let module_file = self.entry_dir.join(relative).with_extension("py");
        if module_file.is_file() {
            return Some((module_file, false));
        }

        None
    }

    fn resolve_from_module_name(
        &self,
        current_module: Option<&str>,
        current_is_package: bool,
        module_spec: &str,
        span: Span,
        source: &str,
        filename: &str,
    ) -> Result<String, CompileError> {
        let (level, maybe_module) = parse_module_spec(module_spec);
        if level == 0 {
            return maybe_module
                .map(str::to_string)
                .ok_or_else(|| CompileError::new("Unsupported import", span, source, filename));
        }

        let Some(current_module) = current_module else {
            return Err(CompileError::new(
                "Relative import is only supported inside package modules",
                span,
                source,
                filename,
            ));
        };

        let mut package_parts: Vec<&str> = current_module.split('.').collect();
        if !current_is_package {
            let _ = package_parts.pop();
        }

        let levels_up = level.saturating_sub(1);
        if levels_up > package_parts.len() {
            return Err(CompileError::new(
                "Relative import goes beyond top-level package",
                span,
                source,
                filename,
            ));
        }
        let keep = package_parts.len() - levels_up;
        package_parts.truncate(keep);

        let mut parts: Vec<String> = package_parts.into_iter().map(str::to_string).collect();
        if let Some(module) = maybe_module {
            if !module.is_empty() {
                parts.extend(module.split('.').map(str::to_string));
            }
        }

        if parts.is_empty() {
            return Err(CompileError::new(
                "Unsupported import",
                span,
                source,
                filename,
            ));
        }
        Ok(parts.join("."))
    }
}

struct ModuleRewriter<'a> {
    expr_renames: HashMap<String, String>,
    type_renames: HashMap<String, String>,
    target_renames: HashMap<String, String>,
    module_alias_members: HashMap<String, HashMap<String, ExportEntry>>,
    scopes: Vec<HashSet<String>>,
    source: &'a str,
    filename: &'a str,
}

impl<'a> ModuleRewriter<'a> {
    fn rewrite_program(&mut self, program: &mut Program) -> Result<(), CompileError> {
        for item in &mut program.items {
            self.rewrite_item(item)?;
        }
        Ok(())
    }

    fn rewrite_item(&mut self, item: &mut Item) -> Result<(), CompileError> {
        match item {
            Item::Function(func) => self.rewrite_function(func, true),
            Item::Class(class_def) => self.rewrite_class(class_def),
            Item::Union(union) => {
                if let Some(mapped) = self.target_renames.get(union.name.as_str()) {
                    union.name = mapped.clone();
                }
                for variant in &mut union.variants {
                    if let Some(mapped) = self.type_renames.get(variant.as_str()) {
                        *variant = mapped.clone();
                    }
                }
                Ok(())
            }
            Item::Stmt(stmt) => self.rewrite_stmt(stmt.as_mut(), true),
        }
    }

    fn rewrite_function(
        &mut self,
        func: &mut Function,
        rename_function_name: bool,
    ) -> Result<(), CompileError> {
        if rename_function_name {
            if let Some(mapped) = self.target_renames.get(func.name.as_str()) {
                func.name = mapped.clone();
            }
        }

        for param in &mut func.params {
            self.rewrite_type_ref(&mut param.ann);
            if let Some(default) = param.default.as_mut() {
                self.rewrite_expr(default)?;
            }
        }
        self.rewrite_type_ref(&mut func.ret);

        self.push_scope();
        for param in &func.params {
            self.bind_local(param.name.as_str());
        }
        for stmt in &mut func.body {
            self.rewrite_stmt(stmt, false)?;
        }
        self.pop_scope();

        Ok(())
    }

    fn rewrite_class(&mut self, class_def: &mut ClassDef) -> Result<(), CompileError> {
        if let Some(mapped) = self.target_renames.get(class_def.name.as_str()) {
            class_def.name = mapped.clone();
        }
        if let Some(base) = class_def.base.as_mut() {
            if let Some(mapped) = self.type_renames.get(base.as_str()) {
                *base = mapped.clone();
            }
        }

        for field in &mut class_def.fields {
            self.rewrite_type_ref(&mut field.ty);
        }
        for attr in &mut class_def.class_attrs {
            if let Some(ann) = attr.ann.as_mut() {
                self.rewrite_type_ref(ann);
            }
            self.rewrite_expr(&mut attr.value)?;
        }
        for method in &mut class_def.methods {
            self.rewrite_function(method, false)?;
        }

        Ok(())
    }

    fn rewrite_stmt(&mut self, stmt: &mut Stmt, top_level: bool) -> Result<(), CompileError> {
        match &mut stmt.kind {
            StmtKind::Let { name, ann, value } => {
                self.rewrite_expr(value)?;
                if let Some(ann) = ann.as_mut() {
                    self.rewrite_type_ref(ann);
                }
                if top_level {
                    if let Some(mapped) = self.target_renames.get(name.as_str()) {
                        *name = mapped.clone();
                    }
                } else {
                    self.bind_local(name.as_str());
                }
            }
            StmtKind::Assign { target, value } => {
                self.rewrite_expr(value)?;
                self.rewrite_assign_target(target, top_level)?;
            }
            StmtKind::Return { value } => {
                if let Some(value) = value.as_mut() {
                    self.rewrite_expr(value)?;
                }
            }
            StmtKind::If { test, body, orelse } => {
                self.rewrite_expr(test)?;
                for stmt in body {
                    self.rewrite_stmt(stmt, top_level)?;
                }
                for stmt in orelse {
                    self.rewrite_stmt(stmt, top_level)?;
                }
            }
            StmtKind::While { test, body } => {
                self.rewrite_expr(test)?;
                for stmt in body {
                    self.rewrite_stmt(stmt, top_level)?;
                }
            }
            StmtKind::For { target, iter, body } => {
                self.rewrite_expr(iter)?;
                match target {
                    ForTarget::Name(name) => {
                        if top_level {
                            if let Some(mapped) = self.target_renames.get(name.as_str()) {
                                *name = mapped.clone();
                            }
                        } else {
                            self.bind_local(name.as_str());
                        }
                    }
                    ForTarget::Tuple(names) => {
                        for name in names {
                            if top_level {
                                if let Some(mapped) = self.target_renames.get(name.as_str()) {
                                    *name = mapped.clone();
                                }
                            } else {
                                self.bind_local(name.as_str());
                            }
                        }
                    }
                }
                for stmt in body {
                    self.rewrite_stmt(stmt, top_level)?;
                }
            }
            StmtKind::Import { .. } => {}
            StmtKind::ImportFrom { .. } => {}
            StmtKind::Global { names } | StmtKind::Nonlocal { names } => {
                for name in names {
                    if let Some(mapped) = self.target_renames.get(name.as_str()) {
                        *name = mapped.clone();
                    }
                }
            }
            StmtKind::Break | StmtKind::Continue => {}
            StmtKind::Expr(expr) => {
                self.rewrite_expr(expr)?;
            }
            StmtKind::Assert { test, msg } => {
                self.rewrite_expr(test)?;
                if let Some(msg) = msg.as_mut() {
                    self.rewrite_expr(msg)?;
                }
            }
            StmtKind::Match { subject, cases } => {
                self.rewrite_expr(subject)?;
                for case in cases {
                    if let Some(mapped) = self.type_renames.get(case.variant.as_str()) {
                        case.variant = mapped.clone();
                    }
                    for stmt in &mut case.body {
                        self.rewrite_stmt(stmt, top_level)?;
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
                    self.rewrite_stmt(stmt, top_level)?;
                }
                for handler in handlers {
                    if let Some(exc_type) = handler.exc_type.as_mut() {
                        if let Some(mapped) = self.type_renames.get(exc_type.as_str()) {
                            *exc_type = mapped.clone();
                        }
                    }
                    if let Some(name) = handler.name.as_ref() {
                        if !top_level {
                            self.bind_local(name.as_str());
                        }
                    }
                    for stmt in &mut handler.body {
                        self.rewrite_stmt(stmt, top_level)?;
                    }
                }
                for stmt in orelse {
                    self.rewrite_stmt(stmt, top_level)?;
                }
                for stmt in finalbody {
                    self.rewrite_stmt(stmt, top_level)?;
                }
            }
            StmtKind::Raise { exc, cause } => {
                if let Some(exc) = exc.as_mut() {
                    self.rewrite_expr(exc)?;
                }
                if let Some(cause) = cause.as_mut() {
                    self.rewrite_expr(cause)?;
                }
            }
        }
        Ok(())
    }

    fn rewrite_assign_target(
        &mut self,
        target: &mut AssignTarget,
        top_level: bool,
    ) -> Result<(), CompileError> {
        match target {
            AssignTarget::Name(name) => {
                if top_level {
                    if let Some(mapped) = self.target_renames.get(name.as_str()) {
                        *name = mapped.clone();
                    }
                } else {
                    self.bind_local(name.as_str());
                }
            }
            AssignTarget::Attr { value, .. } => {
                self.rewrite_expr(value)?;
            }
            AssignTarget::Index { value, index } => {
                self.rewrite_expr(value)?;
                self.rewrite_expr(index)?;
            }
            AssignTarget::Tuple(items) | AssignTarget::List(items) => {
                for item in items {
                    self.rewrite_assign_target(item, top_level)?;
                }
            }
            AssignTarget::Starred(inner) => {
                self.rewrite_assign_target(inner.as_mut(), top_level)?;
            }
        }
        Ok(())
    }

    fn rewrite_expr(&mut self, expr: &mut Expr) -> Result<(), CompileError> {
        match &mut expr.kind {
            ExprKind::Literal(_) => {}
            ExprKind::Name(name) => {
                if !self.is_shadowed(name.as_str()) {
                    if let Some(mapped) = self.expr_renames.get(name.as_str()) {
                        *name = mapped.clone();
                    }
                }
            }
            ExprKind::Yield { value } => {
                if let Some(value) = value.as_mut() {
                    self.rewrite_expr(value.as_mut())?;
                }
            }
            ExprKind::Call {
                func,
                args,
                keywords,
            } => {
                self.rewrite_expr(func.as_mut())?;
                for arg in args {
                    self.rewrite_expr(arg)?;
                }
                for kw in keywords {
                    self.rewrite_expr(&mut kw.value)?;
                }
            }
            ExprKind::Starred { value } => {
                self.rewrite_expr(value.as_mut())?;
            }
            ExprKind::Attr { value, attr } => {
                self.rewrite_expr(value.as_mut())?;
                if let ExprKind::Name(base) = &value.kind {
                    if !self.is_shadowed(base.as_str()) {
                        if let Some(exports) = self.module_alias_members.get(base.as_str()) {
                            let Some(export) = exports.get(attr.as_str()) else {
                                return Err(CompileError::new(
                                    format!("module '{base}' has no member '{attr}'"),
                                    expr.span,
                                    self.source,
                                    self.filename,
                                ));
                            };
                            expr.kind = ExprKind::Name(export.symbol.clone());
                        }
                    }
                }
            }
            ExprKind::Binary { left, right, .. } => {
                self.rewrite_expr(left.as_mut())?;
                self.rewrite_expr(right.as_mut())?;
            }
            ExprKind::Unary { expr: inner, .. } => {
                self.rewrite_expr(inner.as_mut())?;
            }
            ExprKind::Compare { left, right, .. } => {
                self.rewrite_expr(left.as_mut())?;
                self.rewrite_expr(right.as_mut())?;
            }
            ExprKind::CompareChain {
                left, comparators, ..
            } => {
                self.rewrite_expr(left.as_mut())?;
                for cmp in comparators {
                    self.rewrite_expr(cmp)?;
                }
            }
            ExprKind::BoolOp { values, .. } => {
                for value in values {
                    self.rewrite_expr(value)?;
                }
            }
            ExprKind::List(items) | ExprKind::Tuple(items) | ExprKind::Set(items) => {
                for item in items {
                    self.rewrite_expr(item)?;
                }
            }
            ExprKind::Dict(items) => {
                for (key, value) in items {
                    self.rewrite_expr(key)?;
                    self.rewrite_expr(value)?;
                }
            }
            ExprKind::Index { value, index } => {
                self.rewrite_expr(value.as_mut())?;
                self.rewrite_expr(index.as_mut())?;
            }
            ExprKind::Slice {
                value,
                start,
                end,
                step,
            } => {
                self.rewrite_expr(value.as_mut())?;
                if let Some(start) = start.as_mut() {
                    self.rewrite_expr(start.as_mut())?;
                }
                if let Some(end) = end.as_mut() {
                    self.rewrite_expr(end.as_mut())?;
                }
                if let Some(step) = step.as_mut() {
                    self.rewrite_expr(step.as_mut())?;
                }
            }
            ExprKind::ListComp {
                elt,
                target,
                iter,
                ifs,
                generators,
            }
            | ExprKind::SetComp {
                elt,
                target,
                iter,
                ifs,
                generators,
            } => {
                self.push_scope();
                for clause in generators.iter_mut() {
                    self.rewrite_expr(clause.iter.as_mut())?;
                    self.bind_local(clause.target.as_str());
                    for cond in &mut clause.ifs {
                        self.rewrite_expr(cond)?;
                    }
                }
                self.rewrite_expr(elt.as_mut())?;
                if let Some(first) = generators.first() {
                    *target = first.target.clone();
                    *iter = first.iter.clone();
                    *ifs = first.ifs.clone();
                }
                self.pop_scope();
            }
            ExprKind::UnionCtor {
                union,
                variant,
                inner,
            } => {
                if let Some(mapped) = self.type_renames.get(union.as_str()) {
                    *union = mapped.clone();
                }
                if let Some(mapped) = self.type_renames.get(variant.as_str()) {
                    *variant = mapped.clone();
                }
                self.rewrite_expr(inner.as_mut())?;
            }
            ExprKind::Lambda { params, body } => {
                self.push_scope();
                for param in params.iter() {
                    self.bind_local(param.as_str());
                }
                self.rewrite_expr(body.as_mut())?;
                self.pop_scope();
            }
            ExprKind::IfExpr { test, body, orelse } => {
                self.rewrite_expr(test.as_mut())?;
                self.rewrite_expr(body.as_mut())?;
                self.rewrite_expr(orelse.as_mut())?;
            }
            ExprKind::Block { stmts } => {
                for stmt in stmts {
                    self.rewrite_stmt(stmt, false)?;
                }
            }
        }
        Ok(())
    }

    fn rewrite_type_ref(&self, ty: &mut TypeRef) {
        match ty {
            TypeRef::Name(name) => {
                if let Some(mapped) = self.type_renames.get(name.as_str()) {
                    *name = mapped.clone();
                }
            }
            TypeRef::List(inner)
            | TypeRef::Optional(inner)
            | TypeRef::Iterator(inner)
            | TypeRef::Set(inner) => self.rewrite_type_ref(inner.as_mut()),
            TypeRef::Dict(key, value) | TypeRef::Result(key, value) => {
                self.rewrite_type_ref(key.as_mut());
                self.rewrite_type_ref(value.as_mut());
            }
            TypeRef::Tuple(items) | TypeRef::Union(items) => {
                for item in items {
                    self.rewrite_type_ref(item);
                }
            }
            TypeRef::Lambda { params, ret } => {
                for param in params {
                    self.rewrite_type_ref(param);
                }
                self.rewrite_type_ref(ret.as_mut());
            }
            TypeRef::Exception(name) => {
                if let Some(mapped) = self.type_renames.get(name.as_str()) {
                    *name = mapped.clone();
                }
            }
            TypeRef::Unknown | TypeRef::None => {}
        }
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashSet::new());
    }

    fn pop_scope(&mut self) {
        let _ = self.scopes.pop();
    }

    fn bind_local(&mut self, name: &str) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string());
        }
    }

    fn is_shadowed(&self, name: &str) -> bool {
        self.scopes.iter().rev().any(|scope| scope.contains(name))
    }
}

fn resolve_entry_dir(filename: &str) -> Result<PathBuf, CompileError> {
    let parent = Path::new(filename)
        .parent()
        .unwrap_or_else(|| Path::new("."));
    if parent.is_absolute() {
        return Ok(parent.to_path_buf());
    }
    let cwd = std::env::current_dir().map_err(|err| {
        CompileError::new(
            format!("failed to determine current directory: {err}"),
            Span::new(0, 0),
            "",
            filename,
        )
    })?;
    Ok(cwd.join(parent))
}

fn is_virtual_module(module: &str) -> bool {
    module == "typing" || resolve_module(module).is_some()
}

fn parse_module_spec(module_spec: &str) -> (usize, Option<&str>) {
    let level = module_spec.chars().take_while(|ch| *ch == '.').count();
    if level == 0 {
        if module_spec.is_empty() {
            return (0, None);
        }
        return (0, Some(module_spec));
    }
    let rest = &module_spec[level..];
    if rest.is_empty() {
        (level, None)
    } else {
        (level, Some(rest))
    }
}

fn module_prefix(module_name: &str) -> String {
    let mut out = String::from("__mod_");
    for ch in module_name.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    out.push_str("__");
    out
}

fn collect_top_level_symbols(program: &Program) -> HashMap<String, SymbolKind> {
    let mut symbols = HashMap::new();
    for item in &program.items {
        match item {
            Item::Function(func) => {
                symbols.insert(func.name.clone(), SymbolKind::Function);
            }
            Item::Class(class_def) => {
                symbols.insert(class_def.name.clone(), SymbolKind::Class);
            }
            Item::Union(union_def) => {
                symbols.insert(union_def.name.clone(), SymbolKind::Union);
            }
            Item::Stmt(stmt) => match &stmt.kind {
                StmtKind::Let { name, .. } => {
                    symbols.insert(name.clone(), SymbolKind::Value);
                }
                StmtKind::Assign { target, .. } => {
                    for name in collect_assign_target_names(target) {
                        symbols.insert(name, SymbolKind::Value);
                    }
                }
                _ => {}
            },
        }
    }
    symbols
}

fn collect_assign_target_names(target: &AssignTarget) -> Vec<String> {
    let mut names = Vec::new();
    collect_assign_target_names_inner(target, &mut names);
    names
}

fn collect_assign_target_names_inner(target: &AssignTarget, out: &mut Vec<String>) {
    match target {
        AssignTarget::Name(name) => out.push(name.clone()),
        AssignTarget::Tuple(items) | AssignTarget::List(items) => {
            for item in items {
                collect_assign_target_names_inner(item, out);
            }
        }
        AssignTarget::Starred(inner) => {
            collect_assign_target_names_inner(inner.as_ref(), out);
        }
        AssignTarget::Attr { .. } | AssignTarget::Index { .. } => {}
    }
}
