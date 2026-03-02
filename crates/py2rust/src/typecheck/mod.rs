use crate::diagnostic::{CompileError, Warning};
use crate::hir::*;
use crate::span::Span;
use crate::types::{Type, TypeRef};
use indexmap::IndexMap;
use std::collections::{HashMap, HashSet};

mod call;
mod check_class;
mod check_function;
mod context;
mod diag;
mod expr;
mod resolve;
mod scope;
mod signatures;
mod stmt;
mod throws;
mod type_ops;

pub use context::{ClassAttrInfo, ClassInfo, FunctionSig, PropertyInfo, TypeContext, UnionInfo};

/// Global scope tracking for top-level statements.
///
/// Python's `global` statement affects variable resolution. We track which
/// variables have been declared as global and detect uses before declaration.
#[derive(Debug, Default, Clone)]
struct GlobalScope {
    /// Variables explicitly declared with `global` statement
    declared: HashSet<String>,
    /// Variables used before being declared global (for error reporting)
    used_before_decl: HashMap<String, Span>,
}

/// Nonlocal scope tracking for nested functions.
///
/// This mirrors global tracking but resolves names to enclosing function scopes.
#[derive(Debug, Default, Clone)]
struct NonlocalScope {
    /// Variables explicitly declared with `nonlocal` statement
    declared: HashSet<String>,
    /// Variables used before being declared nonlocal (for error reporting)
    used_before_decl: HashMap<String, Span>,
}

/// The type checker performs type inference and validation on the HIR.
///
/// Type checking is a critical phase that:
/// 1. Resolves TypeRef annotations to concrete Type values
/// 2. Infers types for variables and expressions without annotations
/// 3. Validates type compatibility in assignments, function calls, operations
/// 4. Detects which functions can throw exceptions (for Result return types)
/// 5. Fills in the `ty` fields in HIR Expr nodes for codegen to use
///
/// The type checker maintains:
/// - TypeContext: Global registry of classes, unions, functions, and types
/// - Scope stack: For lexical scoping of local variables
/// - Global scope stack: For tracking Python's `global` declarations
/// - Lambda definitions: Inline lambda bodies that need type inference
///
/// Design notes:
/// - We use a simple scope stack rather than a symbol table because Python
///   has relatively simple scoping rules (no block scoping, just function scoping)
/// - Exception analysis is done in a separate pass AFTER type checking because
///   it needs complete type information to determine which operations can throw
/// - We track exception handler depth to properly handle bare `raise` statements
pub struct TypeChecker<'a> {
    source: &'a str,
    filename: &'a str,
    /// Type context containing all type information (output of type checking)
    ctx: TypeContext,
    /// Stack of local variable scopes (for nested functions, comprehensions, etc.)
    scopes: Vec<HashMap<String, Type>>,
    /// Stack of global scopes (for tracking `global` declarations)
    global_scopes: Vec<GlobalScope>,
    /// Stack of nonlocal scopes (for tracking `nonlocal` declarations)
    nonlocal_scopes: Vec<NonlocalScope>,
    /// Accumulated warnings (e.g., unused variables, potential issues)
    warnings: Vec<Warning>,
    /// Depth of nested exception handlers (for bare `raise` validation)
    except_handler_depth: usize,
    /// Lambda expression bodies (stored for deferred type inference)
    lambda_defs: HashMap<String, Expr>,
    /// Current class name when type checking methods, used for inference.
    current_class: Option<String>,
    /// Stack of scope indices marking the start of each function scope.
    function_scopes: Vec<usize>,
    /// Stack of inferred generator yield types for the current function nesting.
    generator_yield_stack: Vec<Option<Type>>,
    /// Active lambda names currently being re-inferred from stored lambda bodies.
    active_lambda_inference: HashSet<String>,
    /// Optional scope floor used to block outer-local capture in constrained contexts.
    capture_scope_floor: Option<usize>,
    /// Statement nesting depth for control-flow blocks.
    control_flow_depth: usize,
}

impl<'a> TypeChecker<'a> {
    /// Create a new type checker and collect top-level signatures.
    ///
    /// Before we can type check function bodies, we need to know the signatures
    /// of all functions and the structure of all classes/unions. This first pass
    /// collects that information.
    ///
    /// We do this in two passes because:
    /// 1. Functions can call each other recursively
    /// 2. Classes can reference each other (e.g., Node having a List[Node] field)
    /// 3. Union variants must be previously-defined classes
    pub fn new(
        program: &Program,
        source: &'a str,
        filename: &'a str,
    ) -> Result<Self, CompileError> {
        let mut classes = HashMap::new();
        let mut unions = HashMap::new();
        let functions = HashMap::new();
        let globals = HashMap::new();

        // First pass: collect union definitions
        for item in &program.items {
            if let Item::Union(def) = item {
                unions.insert(
                    def.name.clone(),
                    UnionInfo {
                        name: def.name.clone(),
                        variants: def.variants.clone(),
                    },
                );
            }
        }

        // Second pass: collect class definitions (need to know about unions first)
        for item in &program.items {
            if let Item::Class(class_def) = item {
                classes.insert(
                    class_def.name.clone(),
                    ClassInfo {
                        name: class_def.name.clone(),
                        owner_scope: None,
                        base: class_def.base.clone(),
                        fields: IndexMap::new(),
                        class_attrs: IndexMap::new(),
                        methods: HashMap::new(),
                        method_kinds: HashMap::new(),
                        properties: HashMap::new(),
                        init: None,
                        iter_return: None,
                        iter_item: None,
                        next_item: None,
                        match_args: class_def.match_args.clone(),
                    },
                );
            }
        }

        let mut checker = Self {
            source,
            filename,
            ctx: TypeContext {
                classes,
                unions,
                functions,
                globals,
            },
            scopes: Vec::new(),
            global_scopes: Vec::new(),
            nonlocal_scopes: Vec::new(),
            warnings: Vec::new(),
            except_handler_depth: 0,
            lambda_defs: HashMap::new(),
            current_class: None,
            function_scopes: Vec::new(),
            generator_yield_stack: Vec::new(),
            active_lambda_inference: HashSet::new(),
            capture_scope_floor: None,
            control_flow_depth: 0,
        };

        // Third pass: collect function and class signatures (methods, fields)
        checker.collect_signatures(program)?;

        Ok(checker)
    }

    /// Main entry point for type checking a program.
    ///
    /// Type checking happens in several phases:
    /// 1. Check top-level statements (initializers, assignments)
    /// 2. Check all function bodies
    /// 3. Check all class methods
    /// 4. Run exception analysis to determine which functions can throw
    /// 5. Update function signatures with Result types if they can throw
    ///
    /// The order matters: we need to type check all code before we can
    /// analyze exception propagation, because exception analysis needs
    /// to know the types of all function calls.
    pub fn check_program(&mut self, program: &mut Program) -> Result<TypeContext, CompileError> {
        // Set up top-level scope with __name__
        self.scopes.push(HashMap::new());
        self.insert_var("__name__", Type::Str, Span::new(0, 0))?;

        // Phase 1: Type check top-level statements
        for item in &mut program.items {
            if let Item::Stmt(stmt) = item {
                self.check_stmt(stmt.as_mut(), None)?;
            }
        }

        // Save global variable types (excluding __name__ which is a constant)
        if let Some(scope) = self.scopes.last() {
            for (name, ty) in scope.iter() {
                if name.as_str() == "__name__" {
                    continue;
                }
                if matches!(ty, Type::Module(_) | Type::StdlibFunction { .. }) {
                    // Import bindings are compile-time only and must not become runtime globals.
                    continue;
                }
                self.ctx.globals.insert(name.clone(), ty.clone());
            }
        }

        // Phase 2: Type check all functions and classes
        for item in &mut program.items {
            match item {
                Item::Function(func) => self.check_function(func, None, false)?,
                Item::Class(class) => self.check_class(class)?,
                Item::Stmt(_) => {}
                Item::Union(_) => {}
            }
        }

        // Phase 2.1: Infer f64 fields from usage patterns before iterative refinement.
        // This breaks bootstrapping cycles where field types stay Unknown because the
        // init param can't be refined (conflicting call-site arg types), but method
        // bodies clearly use the field as a float (e.g., self.data used with .exp()).
        self.infer_float_fields_from_usage(program);

        // Phase 2.5: Iterative call-site type inference for functions with Unknown params.
        // Run inference → re-check loop until convergence. This resolves cascading type
        // dependencies (e.g., main→gpt→rmsnorm→linear) across multiple levels.
        for _iteration in 0..5 {
            // Snapshot current signatures (params + ret) to detect changes.
            let prev_sigs: HashMap<String, (Vec<Type>, Type)> = self
                .ctx
                .functions
                .iter()
                .map(|(k, v)| (k.clone(), (v.params.clone(), v.ret.clone())))
                .collect();

            self.infer_param_types_from_callsites(program);

            // Collect names of functions whose params were updated.
            let mut updated_params: HashSet<String> = HashSet::new();

            // Update HIR param annotations from refined signatures.
            for item in &mut program.items {
                if let Item::Function(func) = item {
                    let sig = match self.ctx.functions.get(&func.name).cloned() {
                        Some(s) => s,
                        None => continue,
                    };
                    let mut any_refined = false;
                    for (i, param) in func.params.iter_mut().enumerate() {
                        if let Some(ty) = sig.params.get(i) {
                            // Allow partial types (e.g. List(List(Unknown))) to propagate.
                            // Body-based refinement (e.g. .append()) can resolve the
                            // remaining Unknowns during re-check.
                            if !matches!(ty, Type::Unknown) {
                                // Unwrap varargs/kwargs container so resolve_param_type
                                // can re-wrap correctly on re-check.
                                let ann_ty = match param.kind {
                                    ParamKind::VarArgs => match ty {
                                        Type::List(inner) => inner.as_ref(),
                                        _ => ty,
                                    },
                                    ParamKind::VarKeywords => match ty {
                                        Type::Dict(_, v) => v.as_ref(),
                                        _ => ty,
                                    },
                                    _ => ty,
                                };
                                let new_ref = Self::type_to_ref(ann_ty);
                                if param.ann != new_ref {
                                    param.ann = new_ref;
                                    any_refined = true;
                                }
                            }
                        }
                    }
                    if any_refined {
                        updated_params.insert(func.name.clone());
                    }
                }
            }

            // Re-check functions whose params were refined.
            for item in &mut program.items {
                if let Item::Function(func) = item {
                    if updated_params.contains(&func.name) {
                        // Reset return annotation so the re-check can re-infer the
                        // return type from the body with the updated param types.
                        // Without this, a concrete but stale return type (inferred
                        // from an earlier pass with Unknown params) would stick.
                        func.ret = TypeRef::Unknown;
                        let _ = self.check_function(func, None, false);
                    }
                }
            }

            // Detect which function signatures changed (params or return type).
            let changed_sigs: HashSet<String> = self
                .ctx
                .functions
                .iter()
                .filter(|(k, v)| match prev_sigs.get(*k) {
                    Some((prev_params, prev_ret)) => &v.params != prev_params || &v.ret != prev_ret,
                    None => true,
                })
                .map(|(k, _)| k.clone())
                .collect();

            if changed_sigs.is_empty() {
                break;
            }

            // Also re-check functions that CALL functions whose signatures changed,
            // so that expression types in the caller's body get updated.
            for item in &mut program.items {
                match item {
                    Item::Function(func) => {
                        if updated_params.contains(&func.name) {
                            continue; // Already re-checked above.
                        }
                        // Check if this function calls any function whose sig changed.
                        let calls_changed = Self::function_calls_any(&func.body, &changed_sigs);
                        if calls_changed {
                            let _ = self.check_function(func, None, false);
                        }
                    }
                    Item::Stmt(_) => {
                        // Top-level statements are handled in bulk below.
                    }
                    _ => {}
                }
            }

            // Refresh call expression types in all top-level statements.
            // This updates `.ty` on Call expressions whose target function
            // has an updated return type, without requiring a full re-check
            // (which can fail due to scoping issues in nested loops).
            if !changed_sigs.is_empty() {
                self.refresh_call_types_in_items(program, &changed_sigs);
            }
        }

        // Final backward propagation pass: function param types may have been refined
        // by body-based inference (e.g. .append()) but the call-site variable types
        // in top-level code weren't updated. Run refresh again to propagate param types
        // backward to call-site variables, then forward to their declarations.
        // Multiple passes with shared env: the first pass propagates function
        // param types backward to call-site variables (updating env). Subsequent
        // passes pick up those env updates at Let/Assign declarations that
        // precede the call sites in program order.
        let all_funcs: HashSet<String> = self.ctx.functions.keys().cloned().collect();
        self.refresh_call_types_in_items_multi_pass(program, &all_funcs, 3);

        // Update HIR field and __init__ param annotations for classes whose types
        // were refined by constructor call-site inference.
        for item in &mut program.items {
            if let Item::Class(class_def) = item {
                if let Some(class_info) = self.ctx.classes.get(&class_def.name) {
                    // Update field annotations in the HIR.
                    for field in &mut class_def.fields {
                        if matches!(field.ty, TypeRef::Unknown) {
                            if let Some(ty) = class_info.fields.get(&field.name) {
                                if !matches!(ty, Type::Unknown) {
                                    field.ty = Self::type_to_ref(ty);
                                }
                            }
                        }
                    }
                    // Update __init__ param annotations.
                    if let Some(init_sig) = &class_info.init {
                        for method in &mut class_def.methods {
                            if method.name == "__init__" {
                                for (i, param) in method.params.iter_mut().enumerate() {
                                    if matches!(param.ann, TypeRef::Unknown) {
                                        if let Some(ty) = init_sig.params.get(i) {
                                            if !matches!(ty, Type::Unknown) {
                                                param.ann = Self::type_to_ref(ty);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    // Heuristic: infer f64 for fields used with .exp(), .ln(), .pow(),
                    // or in f64 arithmetic. This breaks bootstrapping cycles.
                    let class_info = self.ctx.classes.get(&class_def.name).cloned();
                    if let Some(ci) = &class_info {
                        let mut inferred_f64_fields = Vec::new();
                        for (fname, fty) in &ci.fields {
                            if matches!(fty, Type::Unknown) {
                                // For Unknown fields, use broad heuristics.
                                let is_float_field =
                                    Self::field_used_as_float(&class_def.methods, fname, false);
                                if is_float_field {
                                    inferred_f64_fields.push(fname.clone());
                                }
                            } else if matches!(fty, Type::Int) {
                                // For Int fields, only promote with strong evidence:
                                // float literals in assignment or augmented-assignment
                                // involving multiplication/division.
                                let is_float_field =
                                    Self::field_used_as_float(&class_def.methods, fname, true);
                                if is_float_field {
                                    inferred_f64_fields.push(fname.clone());
                                }
                            }
                        }
                        for fname in &inferred_f64_fields {
                            if let Some(ci) = self.ctx.classes.get_mut(&class_def.name) {
                                ci.fields.insert(fname.clone(), Type::Float);
                            }
                            // Also update field in HIR.
                            for field in &mut class_def.fields {
                                let should_update = field.name == *fname
                                    && match &field.ty {
                                        TypeRef::Unknown => true,
                                        TypeRef::Name(n) => n == "int",
                                        _ => false,
                                    };
                                if should_update {
                                    field.ty = TypeRef::Name("float".to_string());
                                }
                            }
                            // Update __init__ param if it maps to this field.
                            if let Some(ci) = self.ctx.classes.get_mut(&class_def.name) {
                                if let Some(init) = &mut ci.init {
                                    for (i, pname) in init.param_names.iter().enumerate() {
                                        if pname == fname {
                                            if let Some(pty) = init.params.get_mut(i) {
                                                if matches!(pty, Type::Unknown) {
                                                    *pty = Type::Float;
                                                }
                                            }
                                        }
                                    }
                                }
                                if let Some(method_sig) = ci.methods.get_mut("__init__") {
                                    for (i, pname) in method_sig.param_names.iter().enumerate() {
                                        if pname == fname {
                                            if let Some(pty) = method_sig.params.get_mut(i) {
                                                if matches!(pty, Type::Unknown) {
                                                    *pty = Type::Float;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            // Update HIR __init__ param annotation.
                            for method in &mut class_def.methods {
                                if method.name == "__init__" {
                                    for param in &mut method.params {
                                        if param.name == *fname
                                            && matches!(param.ann, TypeRef::Unknown)
                                        {
                                            param.ann = TypeRef::Name("float".to_string());
                                        }
                                    }
                                }
                            }
                        }
                    }
                    // Re-check class methods with refined field/param types.
                    let _ = self.check_class(class_def);
                }
            }
        }

        // Phase 3: Re-check top-level statements now that function return types
        // are known from their bodies. This allows call sites to infer concrete
        // types for functions that had Unknown return type in phase 1.
        for item in &mut program.items {
            if let Item::Stmt(stmt) = item {
                // Silently re-check — errors were already reported in phase 1,
                // we just want to refine types.
                let _ = self.check_stmt(stmt.as_mut(), None);
            }
        }

        // Update global variable types after re-checking.
        if let Some(scope) = self.scopes.last() {
            for (name, ty) in scope.iter() {
                if name.as_str() == "__name__" {
                    continue;
                }
                if matches!(ty, Type::Module(_) | Type::StdlibFunction { .. }) {
                    continue;
                }
                self.ctx.globals.insert(name.clone(), ty.clone());
            }
        }

        // Run exception analysis AFTER type checking
        // This determines which functions can throw exceptions and need Result return types
        let mut throw_analyzer = throws::ThrowAnalyzer::new(&self.ctx);
        let throw_map = throw_analyzer.analyze_program(program);

        // Update function signatures with exception information
        // Functions that can throw get their return type wrapped in Result<T, PyError>
        for (func_name, can_throw) in throw_map {
            if let Some(sig) = self.ctx.functions.get_mut(&func_name) {
                sig.can_throw = can_throw;
                if can_throw {
                    // Wrap return type in Result
                    sig.ret = sig
                        .ret
                        .clone()
                        .wrap_result(Type::Exception("PyError".to_string()));
                }
            }
        }

        self.scopes.pop();
        Ok(self.ctx.clone())
    }

    /// Walk all expressions in the program collecting argument types at call sites
    /// for user-defined functions. When a function has Unknown param types and call
    /// sites provide concrete types, update the function signature.
    fn infer_param_types_from_callsites(&mut self, program: &Program) {
        // Collect: function name → Vec<arg types> from each call site.
        let mut call_arg_types: HashMap<String, Vec<Vec<Type>>> = HashMap::new();

        fn visit_expr(expr: &Expr, out: &mut HashMap<String, Vec<Vec<Type>>>) {
            match &expr.kind {
                ExprKind::Call {
                    func,
                    args,
                    keywords: _,
                } => {
                    if let ExprKind::Name(name) = &func.kind {
                        let arg_types: Vec<Type> = args
                            .iter()
                            .map(|a| a.ty.clone().unwrap_or(Type::Unknown))
                            .collect();
                        out.entry(name.clone()).or_default().push(arg_types);
                    }
                    // Continue visiting subexpressions.
                    visit_expr(func, out);
                    for arg in args {
                        visit_expr(arg, out);
                    }
                }
                ExprKind::Binary { left, right, .. } => {
                    visit_expr(left, out);
                    visit_expr(right, out);
                }
                ExprKind::Unary { expr: inner, .. } => visit_expr(inner, out),
                ExprKind::Compare { left, right, .. } => {
                    visit_expr(left, out);
                    visit_expr(right, out);
                }
                ExprKind::CompareChain {
                    left, comparators, ..
                } => {
                    visit_expr(left, out);
                    for c in comparators {
                        visit_expr(c, out);
                    }
                }
                ExprKind::BoolOp { values, .. } => {
                    for v in values {
                        visit_expr(v, out);
                    }
                }
                ExprKind::Index { value, index } => {
                    visit_expr(value, out);
                    visit_expr(index, out);
                }
                ExprKind::Attr { value, .. } => visit_expr(value, out),
                ExprKind::List(items) | ExprKind::Tuple(items) | ExprKind::Set(items) => {
                    for item in items {
                        visit_expr(item, out);
                    }
                }
                ExprKind::Dict(items) => {
                    for item in items {
                        match item {
                            DictEntry::Item { key, value } => {
                                visit_expr(key, out);
                                visit_expr(value, out);
                            }
                            DictEntry::Unpack { value } => visit_expr(value, out),
                        }
                    }
                }
                ExprKind::ListComp {
                    elt,
                    iter,
                    ifs,
                    generators,
                    ..
                } => {
                    visit_expr(elt, out);
                    visit_expr(iter, out);
                    for cond in ifs {
                        visit_expr(cond, out);
                    }
                    for gen in generators {
                        visit_expr(&gen.iter, out);
                        for cond in &gen.ifs {
                            visit_expr(cond, out);
                        }
                    }
                }
                ExprKind::SetComp {
                    elt,
                    iter,
                    ifs,
                    generators,
                    ..
                } => {
                    visit_expr(elt, out);
                    visit_expr(iter, out);
                    for cond in ifs {
                        visit_expr(cond, out);
                    }
                    for gen in generators {
                        visit_expr(&gen.iter, out);
                        for cond in &gen.ifs {
                            visit_expr(cond, out);
                        }
                    }
                }
                ExprKind::Lambda { body, .. } => visit_expr(body, out),
                ExprKind::IfExpr { test, body, orelse } => {
                    visit_expr(test, out);
                    visit_expr(body, out);
                    visit_expr(orelse, out);
                }
                ExprKind::Slice {
                    value,
                    start,
                    end,
                    step,
                } => {
                    visit_expr(value, out);
                    if let Some(s) = start {
                        visit_expr(s, out);
                    }
                    if let Some(e) = end {
                        visit_expr(e, out);
                    }
                    if let Some(st) = step.as_deref() {
                        visit_expr(st, out);
                    }
                }
                ExprKind::Starred { value } => visit_expr(value, out),
                ExprKind::Yield { value } => {
                    if let Some(v) = value {
                        visit_expr(v, out);
                    }
                }
                ExprKind::Block { stmts } => {
                    for stmt in stmts {
                        visit_stmt(stmt, out);
                    }
                }
                ExprKind::UnionCtor { inner, .. } => visit_expr(inner, out),
                ExprKind::Literal(_) | ExprKind::Name(_) => {}
            }
        }

        fn visit_stmt(stmt: &Stmt, out: &mut HashMap<String, Vec<Vec<Type>>>) {
            match &stmt.kind {
                StmtKind::Let { value, .. } => visit_expr(value, out),
                StmtKind::Assign { value, .. } => visit_expr(value, out),
                StmtKind::Expr(expr) => visit_expr(expr, out),
                StmtKind::Return { value } => {
                    if let Some(expr) = value {
                        visit_expr(expr, out);
                    }
                }
                StmtKind::If { test, body, orelse } => {
                    visit_expr(test, out);
                    for s in body {
                        visit_stmt(s, out);
                    }
                    for s in orelse {
                        visit_stmt(s, out);
                    }
                }
                StmtKind::While { test, body } => {
                    visit_expr(test, out);
                    for s in body {
                        visit_stmt(s, out);
                    }
                }
                StmtKind::For { iter, body, .. } => {
                    visit_expr(iter, out);
                    for s in body {
                        visit_stmt(s, out);
                    }
                }
                StmtKind::Match { subject, cases } => {
                    visit_expr(subject, out);
                    for case in cases {
                        for s in &case.body {
                            visit_stmt(s, out);
                        }
                    }
                }
                StmtKind::Try {
                    body,
                    handlers,
                    orelse,
                    finalbody,
                } => {
                    for s in body {
                        visit_stmt(s, out);
                    }
                    for h in handlers {
                        for s in &h.body {
                            visit_stmt(s, out);
                        }
                    }
                    for s in orelse {
                        visit_stmt(s, out);
                    }
                    for s in finalbody {
                        visit_stmt(s, out);
                    }
                }
                StmtKind::Assert { test, msg } => {
                    visit_expr(test, out);
                    if let Some(m) = msg {
                        visit_expr(m, out);
                    }
                }
                StmtKind::Raise { exc, cause } => {
                    if let Some(e) = exc {
                        visit_expr(e, out);
                    }
                    if let Some(c) = cause {
                        visit_expr(c, out);
                    }
                }
                StmtKind::Delete { .. }
                | StmtKind::Import { .. }
                | StmtKind::ImportFrom { .. }
                | StmtKind::Global { .. }
                | StmtKind::Nonlocal { .. }
                | StmtKind::Break
                | StmtKind::Continue => {}
                StmtKind::Class { def } => {
                    for method in &def.methods {
                        for s in &method.body {
                            visit_stmt(s, out);
                        }
                    }
                }
            }
        }

        // Walk all items (top-level stmts, function bodies, class methods).
        for item in &program.items {
            match item {
                Item::Stmt(stmt) => visit_stmt(stmt, &mut call_arg_types),
                Item::Function(func) => {
                    for s in &func.body {
                        visit_stmt(s, &mut call_arg_types);
                    }
                }
                Item::Class(cls) => {
                    for method in &cls.methods {
                        for s in &method.body {
                            visit_stmt(s, &mut call_arg_types);
                        }
                    }
                }
                Item::Union(_) => {}
            }
        }

        /// Refine a single parameter type from call-site argument types.
        /// If all call sites agree, use that type. If they disagree and the
        /// disagreement is between tuples of different lengths with the same
        /// element type, coerce to `Vec<T>` (variable-arity tuple pattern).
        fn refine_param_from_calls(calls: &[Vec<Type>], param_idx: usize) -> Option<Type> {
            let mut candidate: Option<Type> = None;
            for call_args in calls {
                let arg_ty = match call_args.get(param_idx) {
                    // Skip fully Unknown arguments, but accept partial types like List(Unknown).
                    Some(ty) if !matches!(ty, Type::Unknown) => ty,
                    _ => continue,
                };
                // Prefer arguments without any nested Unknown.
                // If current candidate has Unknown inside, replace with a more concrete one.
                if let Some(ref prev) = candidate {
                    if prev.contains_unknown() && !arg_ty.contains_unknown() {
                        candidate = Some(arg_ty.clone());
                        continue;
                    }
                }
                match &candidate {
                    None => candidate = Some(arg_ty.clone()),
                    Some(prev) if prev == arg_ty => {}
                    Some(prev) => {
                        // Try numeric widening (int + float → float).
                        if let Some(unified) = types_compatible_for_vec(prev, arg_ty) {
                            candidate = Some(unified);
                            continue;
                        }
                        // Check for variable-arity tuple → Vec coercion.
                        let prev_is_tuple = matches!(prev, Type::Tuple(_));
                        let arg_is_tuple = matches!(arg_ty, Type::Tuple(_));
                        let prev_is_vec = matches!(prev, Type::List(_));

                        if prev_is_tuple && arg_is_tuple {
                            let prev_elem = tuple_common_elem(prev);
                            let arg_elem = tuple_common_elem(arg_ty);
                            match (prev_elem, arg_elem) {
                                (Some(pe), Some(ae)) => {
                                    if matches!(pe, Type::None) {
                                        candidate = Some(Type::List(Box::new(ae)));
                                    } else if matches!(ae, Type::None) {
                                        candidate = Some(Type::List(Box::new(pe)));
                                    } else if let Some(unified) = types_compatible_for_vec(&pe, &ae)
                                    {
                                        candidate = Some(Type::List(Box::new(unified)));
                                    } else {
                                        return None;
                                    }
                                }
                                _ => return None,
                            }
                        } else if prev_is_vec && arg_is_tuple {
                            let Type::List(inner) = prev else {
                                unreachable!()
                            };
                            if let Some(ae) = tuple_common_elem(arg_ty) {
                                if matches!(ae, Type::None) {
                                    continue;
                                }
                                if let Some(unified) = types_compatible_for_vec(inner, &ae) {
                                    candidate = Some(Type::List(Box::new(unified)));
                                } else {
                                    return None;
                                }
                            } else {
                                return None;
                            }
                        } else if prev_is_tuple && matches!(arg_ty, Type::List(_)) {
                            let Type::List(arg_inner) = arg_ty else {
                                unreachable!()
                            };
                            if let Some(pe) = tuple_common_elem(prev) {
                                if matches!(pe, Type::None) {
                                    candidate = Some(arg_ty.clone());
                                } else if let Some(unified) =
                                    types_compatible_for_vec(&pe, arg_inner)
                                {
                                    candidate = Some(Type::List(Box::new(unified)));
                                } else {
                                    return None;
                                }
                            } else {
                                return None;
                            }
                        } else {
                            return None;
                        }
                    }
                }
            }
            candidate
        }

        /// Check if two types are compatible for Vec coercion (with numeric widening).
        /// Recursively handles container types like List(Unknown) vs List(Value).
        fn types_compatible_for_vec(a: &Type, b: &Type) -> Option<Type> {
            if a == b {
                return Some(a.clone());
            }
            // int + float → float
            if matches!((a, b), (Type::Int, Type::Float) | (Type::Float, Type::Int)) {
                return Some(Type::Float);
            }
            // Unknown is compatible with anything concrete.
            if matches!(a, Type::Unknown) {
                return Some(b.clone());
            }
            if matches!(b, Type::Unknown) {
                return Some(a.clone());
            }
            // Recursively unify container types.
            match (a, b) {
                (Type::List(ai), Type::List(bi)) => {
                    types_compatible_for_vec(ai, bi).map(|t| Type::List(Box::new(t)))
                }
                (Type::Set(ai), Type::Set(bi)) => {
                    types_compatible_for_vec(ai, bi).map(|t| Type::Set(Box::new(t)))
                }
                (Type::Option(ai), Type::Option(bi)) => {
                    types_compatible_for_vec(ai, bi).map(|t| Type::Option(Box::new(t)))
                }
                (Type::Dict(ak, av), Type::Dict(bk, bv)) => {
                    let k = types_compatible_for_vec(ak, bk)?;
                    let v = types_compatible_for_vec(av, bv)?;
                    Some(Type::Dict(Box::new(k), Box::new(v)))
                }
                _ => None,
            }
        }

        fn tuple_common_elem(ty: &Type) -> Option<Type> {
            match ty {
                Type::Tuple(items) if items.is_empty() => Some(Type::None),
                Type::Tuple(items) => {
                    let mut common = items[0].clone();
                    for item in items.iter().skip(1) {
                        match types_compatible_for_vec(&common, item) {
                            Some(unified) => common = unified,
                            None => return None,
                        }
                    }
                    Some(common)
                }
                _ => None,
            }
        }

        // For each user function with Unknown params (including nested Unknown), try to refine.
        let func_names: Vec<String> = self.ctx.functions.keys().cloned().collect();
        for name in func_names {
            let sig = match self.ctx.functions.get(&name) {
                Some(s) => s.clone(),
                None => continue,
            };
            if !sig.params.iter().any(|t| t.contains_unknown()) {
                continue;
            }
            let calls = match call_arg_types.get(&name) {
                Some(c) => c,
                None => continue,
            };
            let mut refined_params = sig.params.clone();
            for (i, param_ty) in refined_params.iter_mut().enumerate() {
                if !param_ty.contains_unknown() {
                    continue;
                }
                if let Some(ty) = refine_param_from_calls(calls, i) {
                    *param_ty = ty;
                }
            }
            if let Some(sig) = self.ctx.functions.get_mut(&name) {
                sig.params = refined_params;
            }
        }

        // Also refine class __init__ params from constructor call sites.
        let class_names: Vec<String> = self.ctx.classes.keys().cloned().collect();
        for class_name in class_names {
            let init_sig = match self
                .ctx
                .classes
                .get(&class_name)
                .and_then(|c| c.init.clone())
            {
                Some(s) => s,
                None => continue,
            };
            // __init__ params include self; call-site args skip self.
            let has_unknown = init_sig
                .params
                .iter()
                .skip(1)
                .any(|t| matches!(t, Type::Unknown));
            if !has_unknown {
                continue;
            }
            let calls = match call_arg_types.get(&class_name) {
                Some(c) => c,
                None => continue,
            };
            let mut refined_params = init_sig.params.clone();
            for (i, param_ty) in refined_params.iter_mut().enumerate().skip(1) {
                if !matches!(param_ty, Type::Unknown) {
                    continue;
                }
                // Call-site arg index is i-1 (no self in call args).
                if let Some(ty) = refine_param_from_calls(calls, i - 1) {
                    *param_ty = ty;
                }
            }
            // Update the __init__ signature.
            if let Some(class_info) = self.ctx.classes.get_mut(&class_name) {
                if let Some(init) = &mut class_info.init {
                    init.params = refined_params.clone();
                }
                if let Some(method_sig) = class_info.methods.get_mut("__init__") {
                    method_sig.params = refined_params.clone();
                }
                // Also update field types from the refined __init__ params.
                // Scan __init__ body for `self.field = param` assignments to build
                // field→param mapping (field names may differ from param names,
                // e.g., self._children = children).
                let field_to_param = Self::build_field_param_map(&program.items, &class_name);
                for (field_name, param_name) in &field_to_param {
                    if let Some(field_ty) = class_info.fields.get(field_name) {
                        if matches!(field_ty, Type::Unknown) || matches!(field_ty, Type::Tuple(_)) {
                            // Find param index and use refined type.
                            if let Some(idx) =
                                init_sig.param_names.iter().position(|n| n == param_name)
                            {
                                if let Some(refined_ty) = refined_params.get(idx) {
                                    if !matches!(refined_ty, Type::Unknown) {
                                        class_info
                                            .fields
                                            .insert(field_name.clone(), refined_ty.clone());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Build a mapping from field names to __init__ parameter names by scanning
    /// the __init__ body for `self.field = param` assignments.
    fn build_field_param_map(items: &[Item], class_name: &str) -> HashMap<String, String> {
        let mut map = HashMap::new();
        for item in items {
            if let Item::Class(cls) = item {
                if cls.name != class_name {
                    continue;
                }
                for method in &cls.methods {
                    if method.name != "__init__" {
                        continue;
                    }
                    let param_names: HashSet<String> =
                        method.params.iter().map(|p| p.name.clone()).collect();
                    for stmt in &method.body {
                        // Look for `self.field = param_name` pattern.
                        if let StmtKind::Assign { target, value } = &stmt.kind {
                            if let AssignTarget::Attr {
                                value: obj,
                                attr: field_name,
                            } = target.as_ref()
                            {
                                if let ExprKind::Name(obj_name) = &obj.kind {
                                    if obj_name == "self" {
                                        if let ExprKind::Name(rhs_name) = &value.kind {
                                            if param_names.contains(rhs_name) {
                                                map.insert(field_name.clone(), rhs_name.clone());
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        map
    }

    /// Refresh call expression types and propagate through variable assignments
    /// in all top-level statements. Uses a variable environment to track types
    /// that have been updated by call type changes.
    fn refresh_call_types_in_items(&self, program: &mut Program, _changed_sigs: &HashSet<String>) {
        let mut env: HashMap<String, Type> = HashMap::new();
        for item in &mut program.items {
            if let Item::Stmt(stmt) = item {
                Self::refresh_types_in_stmts(
                    std::slice::from_mut(stmt.as_mut()),
                    &self.ctx.functions,
                    &mut env,
                );
            }
        }
    }

    /// Run multiple refresh passes with a shared env to propagate backward types
    /// to declarations that precede call sites in program order.
    fn refresh_call_types_in_items_multi_pass(
        &self,
        program: &mut Program,
        _changed_sigs: &HashSet<String>,
        passes: usize,
    ) {
        let mut env: HashMap<String, Type> = HashMap::new();
        for _ in 0..passes {
            for item in &mut program.items {
                if let Item::Stmt(stmt) = item {
                    Self::refresh_types_in_stmts(
                        std::slice::from_mut(stmt.as_mut()),
                        &self.ctx.functions,
                        &mut env,
                    );
                }
            }
        }
    }

    /// Refresh expression types in a block of statements, tracking variable
    /// bindings in `env` for Name expression resolution.
    fn refresh_types_in_stmts(
        stmts: &mut [Stmt],
        functions: &HashMap<String, crate::typecheck::context::FunctionSig>,
        env: &mut HashMap<String, Type>,
    ) {
        for stmt in stmts.iter_mut() {
            Self::refresh_types_in_stmt(stmt, functions, env);
        }
    }

    fn refresh_types_in_stmt(
        stmt: &mut Stmt,
        functions: &HashMap<String, crate::typecheck::context::FunctionSig>,
        env: &mut HashMap<String, Type>,
    ) {
        match &mut stmt.kind {
            StmtKind::Let { name, value, .. } => {
                Self::refresh_types_in_expr(value, functions, env);
                // If env already has a more specific type (from backward propagation
                // through function params), update the value's type to match.
                if let Some(env_ty) = env.get(name.as_str()) {
                    if !env_ty.contains_unknown() {
                        if value.ty.as_ref().map_or(true, |t| t.contains_unknown()) {
                            value.ty = Some(env_ty.clone());
                        }
                    }
                }
                // Track this variable's type for downstream Name expressions.
                if let Some(ty) = &value.ty {
                    if !matches!(ty, Type::Unknown) {
                        env.insert(name.clone(), ty.clone());
                    }
                }
            }
            StmtKind::Assign { target, value } => {
                Self::refresh_types_in_expr(value, functions, env);
                if let AssignTarget::Name(name) = target.as_ref() {
                    if let Some(ty) = &value.ty {
                        if !matches!(ty, Type::Unknown) {
                            env.insert(name.clone(), ty.clone());
                        }
                    }
                }
            }
            StmtKind::Expr(expr) => {
                Self::refresh_types_in_expr(expr, functions, env);
            }
            StmtKind::Return { value } => {
                if let Some(v) = value {
                    Self::refresh_types_in_expr(v, functions, env);
                }
            }
            StmtKind::For { iter, body, .. } => {
                Self::refresh_types_in_expr(iter, functions, env);
                Self::refresh_types_in_stmts(body, functions, env);
            }
            StmtKind::While { test, body } => {
                Self::refresh_types_in_expr(test, functions, env);
                Self::refresh_types_in_stmts(body, functions, env);
            }
            StmtKind::If { test, body, orelse } => {
                Self::refresh_types_in_expr(test, functions, env);
                Self::refresh_types_in_stmts(body, functions, env);
                Self::refresh_types_in_stmts(orelse, functions, env);
            }
            StmtKind::Try {
                body,
                handlers,
                orelse,
                finalbody,
            } => {
                Self::refresh_types_in_stmts(body, functions, env);
                for h in handlers.iter_mut() {
                    Self::refresh_types_in_stmts(&mut h.body, functions, env);
                }
                Self::refresh_types_in_stmts(orelse, functions, env);
                Self::refresh_types_in_stmts(finalbody, functions, env);
            }
            _ => {}
        }
    }

    /// Recursively update `.ty` on Call and Name expressions using current
    /// function signatures and the variable environment.
    fn refresh_types_in_expr(
        expr: &mut Expr,
        functions: &HashMap<String, crate::typecheck::context::FunctionSig>,
        env: &mut HashMap<String, Type>,
    ) {
        match &mut expr.kind {
            ExprKind::Name(name) => {
                // Update Name type from environment if we have a newer type.
                if let Some(ty) = env.get(name.as_str()) {
                    if expr.ty.as_ref().map_or(true, |t| t.contains_unknown()) {
                        expr.ty = Some(ty.clone());
                    }
                }
            }
            ExprKind::Call {
                func,
                args,
                keywords,
            } => {
                // Update this call's return type from the current function signature.
                if let ExprKind::Name(name) = &func.kind {
                    if let Some(sig) = functions.get(name.as_str()) {
                        if !sig.ret.contains_unknown() {
                            expr.ty = Some(sig.ret.clone());
                        }
                        // Backward propagation: update call-site argument variable
                        // types from the refined function param types. This handles
                        // cases where a variable is initialized as List(List(Unknown))
                        // but the called function's params were refined to a more
                        // specific type through body-based inference.
                        for (i, arg) in args.iter_mut().enumerate() {
                            if let ExprKind::Name(arg_name) = &arg.kind {
                                if let Some(param_ty) = sig.params.get(i) {
                                    if !param_ty.contains_unknown() {
                                        let current_unknown =
                                            arg.ty.as_ref().map_or(true, |t| t.contains_unknown());
                                        if current_unknown {
                                            arg.ty = Some(param_ty.clone());
                                            env.insert(arg_name.clone(), param_ty.clone());
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                Self::refresh_types_in_expr(func, functions, env);
                for arg in args.iter_mut() {
                    Self::refresh_types_in_expr(arg, functions, env);
                }
                for kw in keywords.iter_mut() {
                    Self::refresh_types_in_expr(&mut kw.value, functions, env);
                }
            }
            ExprKind::Binary { left, right, .. } => {
                Self::refresh_types_in_expr(left, functions, env);
                Self::refresh_types_in_expr(right, functions, env);
            }
            ExprKind::Unary { expr: inner, .. } | ExprKind::Starred { value: inner } => {
                Self::refresh_types_in_expr(inner, functions, env);
            }
            ExprKind::Compare { left, right, .. } => {
                Self::refresh_types_in_expr(left, functions, env);
                Self::refresh_types_in_expr(right, functions, env);
            }
            ExprKind::CompareChain {
                left, comparators, ..
            } => {
                Self::refresh_types_in_expr(left, functions, env);
                for c in comparators.iter_mut() {
                    Self::refresh_types_in_expr(c, functions, env);
                }
            }
            ExprKind::BoolOp { values, .. } => {
                for v in values.iter_mut() {
                    Self::refresh_types_in_expr(v, functions, env);
                }
            }
            ExprKind::Index { value, index } => {
                Self::refresh_types_in_expr(value, functions, env);
                Self::refresh_types_in_expr(index, functions, env);
            }
            ExprKind::Attr { value, .. } => {
                Self::refresh_types_in_expr(value, functions, env);
            }
            ExprKind::List(items) | ExprKind::Tuple(items) | ExprKind::Set(items) => {
                for item in items.iter_mut() {
                    Self::refresh_types_in_expr(item, functions, env);
                }
            }
            ExprKind::ListComp {
                elt,
                iter,
                ifs,
                generators,
                ..
            } => {
                Self::refresh_types_in_expr(elt, functions, env);
                Self::refresh_types_in_expr(iter, functions, env);
                for cond in ifs.iter_mut() {
                    Self::refresh_types_in_expr(cond, functions, env);
                }
                for gen in generators.iter_mut() {
                    Self::refresh_types_in_expr(&mut gen.iter, functions, env);
                }
            }
            ExprKind::IfExpr { test, body, orelse } => {
                Self::refresh_types_in_expr(test, functions, env);
                Self::refresh_types_in_expr(body, functions, env);
                Self::refresh_types_in_expr(orelse, functions, env);
            }
            ExprKind::Lambda { body, .. } => {
                Self::refresh_types_in_expr(body, functions, env);
            }
            ExprKind::Slice {
                value,
                start,
                end,
                step,
            } => {
                Self::refresh_types_in_expr(value, functions, env);
                if let Some(s) = start {
                    Self::refresh_types_in_expr(s, functions, env);
                }
                if let Some(e) = end {
                    Self::refresh_types_in_expr(e, functions, env);
                }
                if let Some(st) = step.as_deref_mut() {
                    Self::refresh_types_in_expr(st, functions, env);
                }
            }
            _ => {}
        }
    }

    /// Check if any statement in a function body calls a function whose name is
    /// in the `targets` set. Used to find callers that need re-checking when a
    /// callee's signature changes.
    fn function_calls_any(body: &[Stmt], targets: &HashSet<String>) -> bool {
        fn expr_calls_any(expr: &Expr, targets: &HashSet<String>) -> bool {
            match &expr.kind {
                ExprKind::Call {
                    func,
                    args,
                    keywords,
                } => {
                    if let ExprKind::Name(name) = &func.kind {
                        if targets.contains(name) {
                            return true;
                        }
                    }
                    if expr_calls_any(func, targets) {
                        return true;
                    }
                    for arg in args {
                        if expr_calls_any(arg, targets) {
                            return true;
                        }
                    }
                    for kw in keywords {
                        if expr_calls_any(&kw.value, targets) {
                            return true;
                        }
                    }
                    false
                }
                ExprKind::Binary { left, right, .. } | ExprKind::Compare { left, right, .. } => {
                    expr_calls_any(left, targets) || expr_calls_any(right, targets)
                }
                ExprKind::Unary { expr: inner, .. } | ExprKind::Starred { value: inner } => {
                    expr_calls_any(inner, targets)
                }
                ExprKind::Attr { value, .. } => expr_calls_any(value, targets),
                ExprKind::Index { value, index, .. } => {
                    expr_calls_any(value, targets) || expr_calls_any(index, targets)
                }
                ExprKind::List(items) | ExprKind::Tuple(items) | ExprKind::Set(items) => {
                    items.iter().any(|i| expr_calls_any(i, targets))
                }
                ExprKind::Dict(entries) => entries.iter().any(|e| match e {
                    DictEntry::Item { key, value } => {
                        expr_calls_any(key, targets) || expr_calls_any(value, targets)
                    }
                    DictEntry::Unpack { value } => expr_calls_any(value, targets),
                }),
                ExprKind::BoolOp { values, .. } => {
                    values.iter().any(|v| expr_calls_any(v, targets))
                }
                ExprKind::IfExpr { test, body, orelse } => {
                    expr_calls_any(test, targets)
                        || expr_calls_any(body, targets)
                        || expr_calls_any(orelse, targets)
                }
                ExprKind::Lambda { body, .. } => expr_calls_any(body, targets),
                ExprKind::ListComp {
                    elt,
                    iter,
                    ifs,
                    generators,
                    ..
                }
                | ExprKind::SetComp {
                    elt,
                    iter,
                    ifs,
                    generators,
                    ..
                } => {
                    expr_calls_any(elt, targets)
                        || expr_calls_any(iter, targets)
                        || ifs.iter().any(|c| expr_calls_any(c, targets))
                        || generators.iter().any(|g| {
                            expr_calls_any(&g.iter, targets)
                                || g.ifs.iter().any(|c| expr_calls_any(c, targets))
                        })
                }
                ExprKind::CompareChain {
                    left, comparators, ..
                } => {
                    expr_calls_any(left, targets)
                        || comparators.iter().any(|c| expr_calls_any(c, targets))
                }
                ExprKind::Slice {
                    value,
                    start,
                    end,
                    step,
                } => {
                    expr_calls_any(value, targets)
                        || start.as_ref().is_some_and(|s| expr_calls_any(s, targets))
                        || end.as_ref().is_some_and(|e| expr_calls_any(e, targets))
                        || step.as_deref().is_some_and(|s| expr_calls_any(s, targets))
                }
                ExprKind::Yield { value } => {
                    value.as_ref().is_some_and(|v| expr_calls_any(v, targets))
                }
                ExprKind::UnionCtor { inner, .. } => expr_calls_any(inner, targets),
                ExprKind::Literal(_) | ExprKind::Name(_) => false,
                _ => false,
            }
        }

        fn stmts_call_any(stmts: &[Stmt], targets: &HashSet<String>) -> bool {
            for stmt in stmts {
                let found = match &stmt.kind {
                    StmtKind::Let { value, .. } => expr_calls_any(value, targets),
                    StmtKind::Assign { value, .. } => expr_calls_any(value, targets),
                    StmtKind::Expr(expr) => expr_calls_any(expr, targets),
                    StmtKind::Return { value } => {
                        value.as_ref().is_some_and(|v| expr_calls_any(v, targets))
                    }
                    StmtKind::If { test, body, orelse } => {
                        expr_calls_any(test, targets)
                            || stmts_call_any(body, targets)
                            || stmts_call_any(orelse, targets)
                    }
                    StmtKind::While { test, body } => {
                        expr_calls_any(test, targets) || stmts_call_any(body, targets)
                    }
                    StmtKind::For { iter, body, .. } => {
                        expr_calls_any(iter, targets) || stmts_call_any(body, targets)
                    }
                    StmtKind::Assert { test, msg } => {
                        expr_calls_any(test, targets)
                            || msg.as_ref().is_some_and(|m| expr_calls_any(m, targets))
                    }
                    StmtKind::Raise { exc, cause } => {
                        exc.as_ref().is_some_and(|e| expr_calls_any(e, targets))
                            || cause.as_ref().is_some_and(|c| expr_calls_any(c, targets))
                    }
                    StmtKind::Try {
                        body,
                        handlers,
                        orelse,
                        finalbody,
                    } => {
                        stmts_call_any(body, targets)
                            || handlers.iter().any(|h| stmts_call_any(&h.body, targets))
                            || stmts_call_any(orelse, targets)
                            || stmts_call_any(finalbody, targets)
                    }
                    StmtKind::Match { subject, cases } => {
                        expr_calls_any(subject, targets)
                            || cases.iter().any(|c| stmts_call_any(&c.body, targets))
                    }
                    _ => false,
                };
                if found {
                    return true;
                }
            }
            false
        }

        stmts_call_any(body, targets)
    }

    /// Infer f64 for class fields that are Unknown but used in float-like contexts.
    /// This is applied early (before iterative refinement) to break bootstrapping
    /// cycles where init param can't be refined from call sites (e.g., conflicting
    /// Custom("Value") vs Float arg types) but method bodies clearly treat the field
    /// as numeric.
    fn infer_float_fields_from_usage(&mut self, program: &mut Program) {
        for item in &mut program.items {
            let Item::Class(class_def) = item else {
                continue;
            };
            let class_info = match self.ctx.classes.get(&class_def.name).cloned() {
                Some(ci) => ci,
                None => continue,
            };
            let mut inferred_f64_fields = Vec::new();
            for (fname, fty) in &class_info.fields {
                if !matches!(fty, Type::Unknown) {
                    continue;
                }
                if Self::field_used_as_float(&class_def.methods, fname, false) {
                    inferred_f64_fields.push(fname.clone());
                }
            }
            for fname in &inferred_f64_fields {
                // Update class info field type.
                if let Some(ci) = self.ctx.classes.get_mut(&class_def.name) {
                    ci.fields.insert(fname.clone(), Type::Float);
                    // Update __init__ param if it maps to this field.
                    if let Some(init) = &mut ci.init {
                        for (i, pname) in init.param_names.iter().enumerate() {
                            if pname == fname {
                                if let Some(pty) = init.params.get_mut(i) {
                                    if matches!(pty, Type::Unknown) {
                                        *pty = Type::Float;
                                    }
                                }
                            }
                        }
                    }
                    if let Some(method_sig) = ci.methods.get_mut("__init__") {
                        for (i, pname) in method_sig.param_names.iter().enumerate() {
                            if pname == fname {
                                if let Some(pty) = method_sig.params.get_mut(i) {
                                    if matches!(pty, Type::Unknown) {
                                        *pty = Type::Float;
                                    }
                                }
                            }
                        }
                    }
                }
                // Update HIR field annotation.
                for field in &mut class_def.fields {
                    if field.name == *fname && matches!(field.ty, TypeRef::Unknown) {
                        field.ty = TypeRef::Name("float".to_string());
                    }
                }
                // Update HIR __init__ param annotation.
                for method in &mut class_def.methods {
                    if method.name == "__init__" {
                        for param in &mut method.params {
                            if param.name == *fname && matches!(param.ann, TypeRef::Unknown) {
                                param.ann = TypeRef::Name("float".to_string());
                            }
                        }
                    }
                }
            }
            // Re-check class methods with the refined field types.
            if !inferred_f64_fields.is_empty() {
                let _ = self.check_class(class_def);
            }
        }
    }

    /// Check if a field is used in float-like contexts (method calls like .exp(),
    /// .ln(), .pow(), or arithmetic with float literals).
    /// When `strict` is true (for Int→Float promotion), only strong signals trigger
    /// (float literals, augmented assignments with mul/div). Weak signals like
    /// "arithmetic with another attr" are disabled to avoid false positives.
    fn field_used_as_float(methods: &[Function], field_name: &str, strict: bool) -> bool {
        fn check_expr_for_float_field(expr: &Expr, field_name: &str, strict: bool) -> bool {
            match &expr.kind {
                // self.field.exp() or self.field.ln() → float
                ExprKind::Call { func, args, .. } => {
                    if let ExprKind::Attr { value, attr } = &func.kind {
                        if is_self_field(value, field_name) {
                            let float_methods = ["exp", "ln", "log", "sqrt", "abs", "pow"];
                            if float_methods.contains(&attr.as_str()) {
                                return true;
                            }
                        }
                        // math.exp(self.field), math.log(self.field) → float
                        if let ExprKind::Name(mod_name) = &value.kind {
                            if mod_name == "math" {
                                let math_float_fns =
                                    ["exp", "log", "log2", "log10", "sqrt", "sin", "cos", "pow"];
                                if math_float_fns.contains(&attr.as_str()) {
                                    for arg in args {
                                        if is_self_field(arg, field_name) {
                                            return true;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    // Recurse into func and args.
                    if check_expr_for_float_field(func, field_name, strict) {
                        return true;
                    }
                    for arg in args {
                        if check_expr_for_float_field(arg, field_name, strict) {
                            return true;
                        }
                    }
                }
                // self.field + 0.0 or self.field > 0 → comparison with numeric = float
                ExprKind::Binary { left, right, .. } | ExprKind::Compare { left, right, .. } => {
                    let left_is_field = is_self_field(left, field_name);
                    let right_is_field = is_self_field(right, field_name);
                    let left_is_float = matches!(&left.kind, ExprKind::Literal(Literal::Float(_)));
                    let right_is_float =
                        matches!(&right.kind, ExprKind::Literal(Literal::Float(_)));
                    if (left_is_field && right_is_float) || (right_is_field && left_is_float) {
                        return true;
                    }
                    // self.field ** expr (power) → field is likely float
                    if let ExprKind::Binary { op: BinOp::Pow, .. } = &expr.kind {
                        if left_is_field {
                            return true;
                        }
                    }
                    // Weak signal: self.field + other.field → only use for Unknown fields.
                    if !strict {
                        if let ExprKind::Binary { .. } = &expr.kind {
                            if left_is_field || right_is_field {
                                let other_is_attr = if left_is_field {
                                    matches!(&right.kind, ExprKind::Attr { .. })
                                } else {
                                    matches!(&left.kind, ExprKind::Attr { .. })
                                };
                                if other_is_attr {
                                    return true;
                                }
                            }
                        }
                    }
                    if check_expr_for_float_field(left, field_name, strict) {
                        return true;
                    }
                    if check_expr_for_float_field(right, field_name, strict) {
                        return true;
                    }
                }
                ExprKind::Unary { expr: inner, .. } => {
                    if check_expr_for_float_field(inner, field_name, strict) {
                        return true;
                    }
                }
                ExprKind::Tuple(items) | ExprKind::List(items) => {
                    for item in items {
                        if check_expr_for_float_field(item, field_name, strict) {
                            return true;
                        }
                    }
                }
                ExprKind::IfExpr { test, body, orelse } => {
                    return check_expr_for_float_field(test, field_name, strict)
                        || check_expr_for_float_field(body, field_name, strict)
                        || check_expr_for_float_field(orelse, field_name, strict);
                }
                _ => {}
            }
            false
        }

        fn is_self_field(expr: &Expr, field_name: &str) -> bool {
            if let ExprKind::Attr { value, attr } = &expr.kind {
                if attr == field_name {
                    // Match self.field, variable.field — any name access to this field.
                    if let ExprKind::Name(_) = &value.kind {
                        return true;
                    }
                }
            }
            false
        }

        /// Check if expression is an augmented assignment pattern involving
        /// multiplication or division, which strongly suggests float accumulation.
        /// E.g. `obj.grad = obj.grad + (local_grad * v.grad)`.
        fn augmented_assign_involves_mul_div(expr: &Expr, field_name: &str) -> bool {
            match &expr.kind {
                ExprKind::Binary { op, left, right } => {
                    let field_in_tree =
                        is_self_field(left, field_name) || is_self_field(right, field_name);
                    let has_mul_div = matches!(op, BinOp::Mul | BinOp::Div | BinOp::Pow);
                    if field_in_tree && has_mul_div {
                        return true;
                    }
                    augmented_assign_involves_mul_div(left, field_name)
                        || augmented_assign_involves_mul_div(right, field_name)
                }
                _ => false,
            }
        }

        /// Check if an expression tree contains a float literal anywhere.
        fn contains_float_literal(expr: &Expr) -> bool {
            match &expr.kind {
                ExprKind::Literal(Literal::Float(_)) => true,
                ExprKind::Binary { left, right, .. } | ExprKind::Compare { left, right, .. } => {
                    contains_float_literal(left) || contains_float_literal(right)
                }
                ExprKind::Unary { expr: inner, .. } => contains_float_literal(inner),
                ExprKind::Call { func, args, .. } => {
                    contains_float_literal(func) || args.iter().any(|a| contains_float_literal(a))
                }
                ExprKind::Tuple(items) | ExprKind::List(items) => {
                    items.iter().any(|i| contains_float_literal(i))
                }
                _ => false,
            }
        }

        fn check_stmt_for_float_field(stmt: &Stmt, field_name: &str, strict: bool) -> bool {
            match &stmt.kind {
                StmtKind::Let { value, .. } => {
                    check_expr_for_float_field(value, field_name, strict)
                }
                StmtKind::Assign { target, value } => {
                    // Check if the assignment target IS the field and the RHS has float context.
                    // This catches augmented assignments like `obj.grad += float_expr` which
                    // lower to `obj.grad = obj.grad + float_expr`.
                    if let AssignTarget::Attr { attr, .. } = target.as_ref() {
                        if attr == field_name {
                            if contains_float_literal(value) {
                                return true;
                            }
                            if augmented_assign_involves_mul_div(value, field_name) {
                                return true;
                            }
                        }
                    }
                    check_expr_for_float_field(value, field_name, strict)
                }
                StmtKind::Return { value: Some(expr) } | StmtKind::Expr(expr) => {
                    check_expr_for_float_field(expr, field_name, strict)
                }
                StmtKind::If { test, body, orelse } => {
                    check_expr_for_float_field(test, field_name, strict)
                        || body
                            .iter()
                            .any(|s| check_stmt_for_float_field(s, field_name, strict))
                        || orelse
                            .iter()
                            .any(|s| check_stmt_for_float_field(s, field_name, strict))
                }
                StmtKind::For { iter, body, .. } => {
                    check_expr_for_float_field(iter, field_name, strict)
                        || body
                            .iter()
                            .any(|s| check_stmt_for_float_field(s, field_name, strict))
                }
                StmtKind::While { test, body } => {
                    check_expr_for_float_field(test, field_name, strict)
                        || body
                            .iter()
                            .any(|s| check_stmt_for_float_field(s, field_name, strict))
                }
                _ => false,
            }
        }

        for method in methods {
            for stmt in &method.body {
                if check_stmt_for_float_field(stmt, field_name, strict) {
                    return true;
                }
            }
        }
        false
    }

    /// Return true when a name refers to a built-in Python exception type we support.
    pub(super) fn is_builtin_exception_name(name: &str) -> bool {
        matches!(
            name,
            "Exception"
                | "ValueError"
                | "TypeError"
                | "RuntimeError"
                | "KeyError"
                | "IndexError"
                | "AttributeError"
                | "ZeroDivisionError"
                | "SyntaxError"
                | "NameError"
                | "AssertionError"
                | "StopIteration"
                | "NotImplementedError"
                | "IOError"
                | "OverflowError"
                | "GeneratorExit"
                | "MemoryError"
        )
    }

    /// Resolve a custom exception class name to its built-in root variant.
    ///
    /// For built-ins this returns the same name. For user-defined exceptions this walks
    /// the class inheritance chain until it reaches a built-in exception base.
    pub(super) fn resolve_exception_variant_name(&self, name: &str) -> Option<String> {
        if Self::is_builtin_exception_name(name) {
            return Some(name.to_string());
        }

        let mut current = name;
        let mut seen: HashSet<&str> = HashSet::new();
        loop {
            if !seen.insert(current) {
                // Defensive cycle guard for malformed inheritance graphs.
                return None;
            }
            let class_info = self.ctx.classes.get(current)?;
            let base = class_info.base.as_deref()?;
            if Self::is_builtin_exception_name(base) {
                return Some(base.to_string());
            }
            current = base;
        }
    }

    /// Run a lambda inference pass guarded against recursive re-entry for the same name.
    pub(super) fn with_lambda_inference_guard<T>(
        &mut self,
        name: &str,
        span: Span,
        f: impl FnOnce(&mut Self) -> Result<T, CompileError>,
    ) -> Result<T, CompileError> {
        if !self.active_lambda_inference.insert(name.to_string()) {
            return Err(self.error(
                span,
                format!("Recursive lambda type inference cycle for '{name}'"),
            ));
        }
        let result = f(self);
        self.active_lambda_inference.remove(name);
        result
    }
}
