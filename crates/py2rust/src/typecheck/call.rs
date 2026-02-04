use super::*;

impl<'a> TypeChecker<'a> {
    pub(super) fn check_call(
        &mut self,
        func: &mut Expr,
        args: &mut [Expr],
        expected: Option<&Type>,
        span: Span,
    ) -> Result<Type, CompileError> {
        match &mut func.kind {
            ExprKind::Name(name) => {
                if name == "print" {
                    for arg in args {
                        self.check_expr(arg, None)?;
                    }
                    return Ok(Type::None);
                }
                if name == "len" {
                    if args.len() != 1 {
                        return Err(self.error(span, "len() expects one argument"));
                    }
                    self.check_expr(&mut args[0], None)?;
                    return Ok(Type::Int);
                }
                if name == "range" {
                    if !args.is_empty() && args.len() <= 3 {
                        for arg in args.iter_mut() {
                            let arg_ty = self.check_expr(arg, Some(&Type::Int))?;
                            self.ensure_assignable(&arg_ty, &Type::Int, span)?;
                        }
                        return Ok(Type::Iterator(Box::new(Type::Int)));
                    }
                    return Err(self.error(span, "range() supports 1 to 3 arguments"));
                }
                if name == "round" {
                    if args.is_empty() || args.len() > 2 {
                        return Err(self.error(span, "round() supports 1 or 2 arguments"));
                    }
                    let first_ty = self.check_expr(&mut args[0], None)?;
                    if !first_ty.is_numeric() {
                        return Err(self.error(span, "round() expects a numeric value"));
                    }
                    if args.len() == 2 {
                        let second_ty = self.check_expr(&mut args[1], Some(&Type::Int))?;
                        self.ensure_assignable(&second_ty, &Type::Int, span)?;
                        return Ok(if matches!(first_ty, Type::Float) {
                            Type::Float
                        } else {
                            Type::Int
                        });
                    }
                    return Ok(Type::Int);
                }
                if name == "list" {
                    if args.len() > 1 {
                        return Err(self.error(span, "list() expects zero or one argument"));
                    }
                    if args.is_empty() {
                        if let Some(Type::List(inner)) = expected {
                            return Ok(Type::List(inner.clone()));
                        }
                        return Ok(Type::List(Box::new(Type::Unknown)));
                    }
                    let arg_ty = self.check_expr(&mut args[0], None)?;
                    let item_ty = match arg_ty {
                        Type::List(inner) => *inner,
                        Type::Set(inner) => *inner,
                        Type::Dict(key, _) => *key,
                        Type::Tuple(items) => {
                            if items.is_empty() {
                                Type::Unknown
                            } else if items.iter().all(|t| t == &items[0]) {
                                items[0].clone()
                            } else {
                                Type::Unknown
                            }
                        }
                        Type::Iterator(inner) => *inner,
                        Type::Str => Type::Str,
                        Type::Unknown => Type::Unknown,
                        _ => {
                            return Err(self.error(span, "list() expects an iterable"));
                        }
                    };
                    return Ok(Type::List(Box::new(item_ty)));
                }
                if name == "dict" {
                    if args.len() > 1 {
                        return Err(self.error(span, "dict() expects at most one argument"));
                    }
                    if args.is_empty() {
                        if let Some(Type::Dict(k, v)) = expected {
                            return Ok(Type::Dict(k.clone(), v.clone()));
                        }
                        return Ok(Type::Dict(Box::new(Type::Unknown), Box::new(Type::Unknown)));
                    }
                    let arg_ty = self.check_expr(&mut args[0], None)?;
                    return match arg_ty {
                        Type::Dict(k, v) => Ok(Type::Dict(k, v)),
                        Type::List(inner) | Type::Set(inner) | Type::Iterator(inner) => {
                            if let Type::Tuple(items) = *inner {
                                if items.len() == 2 {
                                    return Ok(Type::Dict(
                                        Box::new(items[0].clone()),
                                        Box::new(items[1].clone()),
                                    ));
                                }
                            }
                            Err(self.error(span, "dict() expects iterable of key/value pairs"))
                        }
                        _ => Err(self.error(span, "dict() expects a dict or iterable of pairs")),
                    };
                }
                if name == "enumerate" {
                    if args.len() != 1 {
                        return Err(self.error(span, "enumerate() expects one argument"));
                    }
                    let iter_ty = self.check_expr(&mut args[0], None)?;
                    let item_ty = self.iter_item_type(&iter_ty, span)?;
                    let tuple = Type::Tuple(vec![Type::Int, item_ty]);
                    return Ok(Type::Iterator(Box::new(tuple)));
                }
                if name == "zip" {
                    if args.len() != 2 {
                        return Err(self.error(span, "zip() expects two arguments"));
                    }
                    let left_ty = self.check_expr(&mut args[0], None)?;
                    let right_ty = self.check_expr(&mut args[1], None)?;
                    let left_item = self.iter_item_type(&left_ty, span)?;
                    let right_item = self.iter_item_type(&right_ty, span)?;
                    let tuple = Type::Tuple(vec![left_item, right_item]);
                    return Ok(Type::Iterator(Box::new(tuple)));
                }
                if name == "map" {
                    if args.len() != 2 {
                        return Err(self.error(span, "map() expects two arguments"));
                    }
                    let iter_ty = self.check_expr(&mut args[1], None)?;
                    let item_ty = self.iter_item_type(&iter_ty, span)?;
                    let out_ty = self.infer_callable_return(&mut args[0], &item_ty, span)?;
                    return Ok(Type::Iterator(Box::new(out_ty)));
                }
                if name == "filter" {
                    if args.len() != 2 {
                        return Err(self.error(span, "filter() expects two arguments"));
                    }
                    let iter_ty = self.check_expr(&mut args[1], None)?;
                    let item_ty = self.iter_item_type(&iter_ty, span)?;
                    let pred_ty = self.infer_callable_return(&mut args[0], &item_ty, span)?;
                    if !matches!(pred_ty, Type::Bool | Type::Unknown) {
                        return Err(self.error(span, "filter() predicate must return bool"));
                    }
                    return Ok(Type::Iterator(Box::new(item_ty)));
                }
                if name == "all" || name == "any" {
                    if args.len() != 1 {
                        return Err(self.error(span, "all()/any() expect one argument"));
                    }
                    let _ = self.check_expr(&mut args[0], None)?;
                    return Ok(Type::Bool);
                }
                if name == "reversed" {
                    if args.len() != 1 {
                        return Err(self.error(span, "reversed() expects one argument"));
                    }
                    let iter_ty = self.check_expr(&mut args[0], None)?;
                    let item_ty = self.iter_item_type(&iter_ty, span)?;
                    return Ok(Type::Iterator(Box::new(item_ty)));
                }
                if name == "max" || name == "min" {
                    if args.is_empty() {
                        return Err(self.error(span, "max()/min() expect at least one argument"));
                    }
                    if args.len() == 1 {
                        let iter_ty = self.check_expr(&mut args[0], None)?;
                        let item_ty = self.iter_item_type(&iter_ty, span)?;
                        return Ok(item_ty);
                    }
                    let mut has_float = false;
                    let mut has_int = false;
                    for arg in args.iter_mut() {
                        let ty = self.check_expr(arg, None)?;
                        match ty {
                            Type::Int => has_int = true,
                            Type::Float => has_float = true,
                            Type::Bool => has_int = true,
                            Type::Unknown => {}
                            _ => {
                                return Err(
                                    self.error(span, "max()/min() expect numeric arguments")
                                );
                            }
                        }
                    }
                    if has_float {
                        return Ok(Type::Float);
                    }
                    if has_int {
                        return Ok(Type::Int);
                    }
                    return Ok(Type::Unknown);
                }
                if name == "abs" {
                    if args.len() != 1 {
                        return Err(self.error(span, "abs() expects one argument"));
                    }
                    let arg_ty = self.check_expr(&mut args[0], None)?;
                    return Ok(match arg_ty {
                        Type::Int => Type::Int,
                        Type::Float => Type::Float,
                        Type::Bool => Type::Int,
                        Type::Unknown => Type::Unknown,
                        _ => {
                            return Err(self.error(span, "abs() expects int or float"));
                        }
                    });
                }
                if name == "pow" {
                    if args.len() != 2 {
                        return Err(self.error(span, "pow() expects two arguments"));
                    }
                    let left_ty = self.check_expr(&mut args[0], None)?;
                    let right_ty = self.check_expr(&mut args[1], None)?;
                    let left_ok = matches!(left_ty, Type::Int | Type::Float | Type::Unknown);
                    let right_ok = matches!(right_ty, Type::Int | Type::Float | Type::Unknown);
                    if !left_ok || !right_ok {
                        return Err(self.error(span, "pow() expects numeric arguments"));
                    }
                    return Ok(Type::Float);
                }
                if name == "sum" {
                    if args.is_empty() || args.len() > 2 {
                        return Err(self.error(span, "sum() expects one or two arguments"));
                    }
                    let iter_ty = self.check_expr(&mut args[0], None)?;
                    let item_ty = self.iter_item_type(&iter_ty, span)?;
                    let mut result_ty = match item_ty {
                        Type::Int => Type::Int,
                        Type::Float => Type::Float,
                        Type::Bool => Type::Int,
                        Type::Unknown => Type::Unknown,
                        _ => {
                            return Err(self.error(span, "sum() expects numeric items"));
                        }
                    };
                    if args.len() == 2 {
                        let start_ty = self.check_expr(&mut args[1], None)?;
                        match start_ty {
                            Type::Float => result_ty = Type::Float,
                            Type::Int | Type::Bool | Type::Unknown => {}
                            _ => {
                                return Err(self.error(span, "sum() start must be numeric"));
                            }
                        }
                    }
                    return Ok(result_ty);
                }
                if name == "int" {
                    if args.len() > 1 {
                        return Err(self.error(span, "int() expects zero or one argument"));
                    }
                    if args.is_empty() {
                        return Ok(Type::Int);
                    }
                    let _ = self.check_expr(&mut args[0], None)?;
                    if let ExprKind::Name(var) = &args[0].kind {
                        if matches!(self.lookup_var(var), Some(Type::Unknown)) {
                            self.set_var_type(var, Type::Str);
                        }
                    }
                    return Ok(Type::Int);
                }
                if name == "float" {
                    if args.len() > 1 {
                        return Err(self.error(span, "float() expects zero or one argument"));
                    }
                    if args.is_empty() {
                        return Ok(Type::Float);
                    }
                    let _ = self.check_expr(&mut args[0], None)?;
                    if let ExprKind::Name(var) = &args[0].kind {
                        if matches!(self.lookup_var(var), Some(Type::Unknown)) {
                            self.set_var_type(var, Type::Str);
                        }
                    }
                    return Ok(Type::Float);
                }
                if name == "bool" {
                    if args.len() > 1 {
                        return Err(self.error(span, "bool() expects zero or one argument"));
                    }
                    if args.len() == 1 {
                        let _ = self.check_expr(&mut args[0], None)?;
                    }
                    return Ok(Type::Bool);
                }
                if name == "str" {
                    if args.len() != 1 {
                        return Err(self.error(span, "str() expects one argument"));
                    }
                    let _ = self.check_expr(&mut args[0], None)?;
                    return Ok(Type::Str);
                }
                if name == "isinstance" {
                    if args.len() != 2 {
                        return Err(self.error(span, "isinstance() expects two arguments"));
                    }
                    let _ = self.check_expr(&mut args[0], None)?;
                    return Ok(Type::Bool);
                }
                if name == "chr" {
                    if args.len() != 1 {
                        return Err(self.error(span, "chr() expects one argument"));
                    }
                    let arg_ty = self.check_expr(&mut args[0], None)?;
                    if !matches!(arg_ty, Type::Int | Type::Unknown) {
                        return Err(self.error(span, "chr() expects int"));
                    }
                    return Ok(Type::Str);
                }
                if name == "ord" {
                    if args.len() != 1 {
                        return Err(self.error(span, "ord() expects one argument"));
                    }
                    let arg_ty = self.check_expr(&mut args[0], None)?;
                    if !matches!(arg_ty, Type::Str | Type::Unknown) {
                        return Err(self.error(span, "ord() expects str"));
                    }
                    return Ok(Type::Int);
                }
                if name == "hash" {
                    if args.len() != 1 {
                        return Err(self.error(span, "hash() expects one argument"));
                    }
                    let _ = self.check_expr(&mut args[0], None)?;
                    return Ok(Type::Int);
                }
                if name == "id" {
                    if args.len() != 1 {
                        return Err(self.error(span, "id() expects one argument"));
                    }
                    let _ = self.check_expr(&mut args[0], None)?;
                    return Ok(Type::Int);
                }
                if name == "divmod" {
                    if args.len() != 2 {
                        return Err(self.error(span, "divmod() expects two arguments"));
                    }
                    let left_ty = self.check_expr(&mut args[0], None)?;
                    let right_ty = self.check_expr(&mut args[1], None)?;
                    let numeric = |ty: &Type| {
                        matches!(ty, Type::Int | Type::Float | Type::Bool | Type::Unknown)
                    };
                    if !numeric(&left_ty) || !numeric(&right_ty) {
                        return Err(self.error(span, "divmod() expects numeric arguments"));
                    }
                    let use_float =
                        matches!(left_ty, Type::Float) || matches!(right_ty, Type::Float);
                    if use_float {
                        return Ok(Type::Tuple(vec![Type::Float, Type::Float]));
                    }
                    return Ok(Type::Tuple(vec![Type::Int, Type::Int]));
                }
                if name == "next" {
                    if args.len() != 1 {
                        return Err(self.error(span, "next() expects one argument"));
                    }
                    let iter_ty = self.check_expr(&mut args[0], None)?;
                    let item_ty = self.iter_item_type(&iter_ty, span)?;
                    return Ok(item_ty);
                }
                if name == "bin" || name == "hex" || name == "oct" {
                    if args.len() != 1 {
                        return Err(self.error(span, format!("{name}() expects one argument")));
                    }
                    let arg_ty = self.check_expr(&mut args[0], None)?;
                    if !matches!(arg_ty, Type::Int | Type::Unknown) {
                        return Err(self.error(span, format!("{name}() expects int")));
                    }
                    return Ok(Type::Str);
                }
                if name == "repr" {
                    if args.len() != 1 {
                        return Err(self.error(span, "repr() expects one argument"));
                    }
                    let _ = self.check_expr(&mut args[0], None)?;
                    return Ok(Type::Str);
                }
                if name == "tuple" {
                    if args.len() > 1 {
                        return Err(self.error(span, "tuple() expects zero or one argument"));
                    }
                    if args.is_empty() {
                        return Ok(Type::List(Box::new(Type::Unknown)));
                    }
                    let iter_ty = self.check_expr(&mut args[0], None)?;
                    let item_ty = self.iter_item_type(&iter_ty, span)?;
                    return Ok(Type::List(Box::new(item_ty)));
                }
                if name == "type" {
                    if args.len() != 1 {
                        return Err(self.error(span, "type() expects one argument"));
                    }
                    let _ = self.check_expr(&mut args[0], None)?;
                    return Ok(Type::Str);
                }
                if name == "exit" {
                    if args.len() > 1 {
                        return Err(self.error(span, "exit() expects zero or one argument"));
                    }
                    if args.len() == 1 {
                        let arg_ty = self.check_expr(&mut args[0], Some(&Type::Int))?;
                        self.ensure_assignable(&arg_ty, &Type::Int, span)?;
                    }
                    return Ok(Type::None);
                }
                if let Some(class_info) = self.ctx.classes.get(name) {
                    let init_sig = class_info.init.clone().ok_or_else(|| {
                        self.error(span, format!("Class {name} is missing __init__"))
                    })?;
                    self.check_call_args(&init_sig, args, span, true)?;
                    if let Some(Type::Union(union_name)) = expected {
                        let variants = self
                            .ctx
                            .unions
                            .get(union_name)
                            .ok_or_else(|| {
                                self.error(span, format!("Unknown union: {union_name}"))
                            })?
                            .variants
                            .clone();
                        if variants.contains(name) {
                            return Ok(Type::Union(union_name.clone()));
                        }
                    }
                    return Ok(Type::Custom(name.clone()));
                }
                if let Some(sig) = self.ctx.functions.get(name).cloned() {
                    self.check_call_args(&sig, args, span, false)?;
                    return Ok(sig.ret.clone());
                }
                if let Some(var_ty) = self.lookup_var(name) {
                    if let Type::Lambda { params, ret } = var_ty {
                        if params.len() != args.len() && !params.is_empty() {
                            return Err(self.error(span, "Argument count mismatch"));
                        }
                        for (arg, param_ty) in args.iter_mut().zip(params.iter()) {
                            if !matches!(param_ty, Type::Unknown) {
                                let arg_ty = self.check_expr(arg, Some(param_ty))?;
                                self.ensure_assignable(&arg_ty, param_ty, span)?;
                            } else {
                                let _ = self.check_expr(arg, None)?;
                            }
                        }
                        return Ok(*ret);
                    }
                    if matches!(var_ty, Type::Unknown) {
                        let mut param_tys = Vec::new();
                        for arg in args.iter_mut() {
                            let arg_ty = self.check_expr(arg, None)?;
                            param_tys.push(arg_ty);
                        }
                        let lambda = Type::Lambda {
                            params: param_tys,
                            ret: Box::new(Type::Unknown),
                        };
                        self.set_var_type(name, lambda);
                        return Ok(Type::Unknown);
                    }
                }
                Err(self.error(span, "Unknown call target"))
            }
            ExprKind::Attr { value, attr } => {
                let obj_ty = self.check_expr(value, None)?;
                if let Type::List(inner) = &obj_ty {
                    if attr == "append" {
                        if args.len() != 1 {
                            return Err(self.error(span, "list.append() expects one argument"));
                        }
                        let arg_ty = self.check_expr(&mut args[0], Some(inner))?;
                        if !matches!(arg_ty, Type::Unknown)
                            && !matches!(inner.as_ref(), Type::Unknown)
                        {
                            self.ensure_assignable(&arg_ty, inner, span)?;
                        }
                        if matches!(inner.as_ref(), Type::Unknown)
                            && !matches!(arg_ty, Type::Unknown)
                        {
                            if let ExprKind::Name(name) = &value.kind {
                                self.set_var_type(name, Type::List(Box::new(arg_ty.clone())));
                            }
                        }
                        return Ok(Type::None);
                    }
                }
                if let Type::Set(inner) = &obj_ty {
                    if attr == "add" {
                        if args.len() != 1 {
                            return Err(self.error(span, "set.add() expects one argument"));
                        }
                        let arg_ty = self.check_expr(&mut args[0], Some(inner))?;
                        if !matches!(arg_ty, Type::Unknown)
                            && !matches!(inner.as_ref(), Type::Unknown)
                        {
                            self.ensure_assignable(&arg_ty, inner, span)?;
                        }
                        if matches!(inner.as_ref(), Type::Unknown)
                            && !matches!(arg_ty, Type::Unknown)
                        {
                            if let ExprKind::Name(name) = &value.kind {
                                self.set_var_type(name, Type::Set(Box::new(arg_ty.clone())));
                            }
                        }
                        return Ok(Type::None);
                    }
                    if attr == "remove" {
                        if args.len() != 1 {
                            return Err(self.error(span, "set.remove() expects one argument"));
                        }
                        let arg_ty = self.check_expr(&mut args[0], Some(inner))?;
                        if !matches!(arg_ty, Type::Unknown)
                            && !matches!(inner.as_ref(), Type::Unknown)
                        {
                            self.ensure_assignable(&arg_ty, inner, span)?;
                        }
                        return Ok(Type::None);
                    }
                }
                if attr == "format" {
                    if args.is_empty() {
                        return Ok(Type::Str);
                    }
                    for arg in args.iter_mut() {
                        let arg_ty = self.check_expr(arg, None)?;
                        if !matches!(arg_ty, Type::Str) {
                            let inner = arg.clone();
                            *arg = Expr {
                                kind: ExprKind::Call {
                                    func: Box::new(Expr {
                                        kind: ExprKind::Name("str".to_string()),
                                        span: arg.span,
                                        ty: Some(Type::Str),
                                    }),
                                    args: vec![inner],
                                },
                                span: arg.span,
                                ty: Some(Type::Str),
                            };
                        }
                    }
                    return Ok(Type::Str);
                }
                if let Type::Custom(class_name) = obj_ty {
                    let sig = self
                        .ctx
                        .classes
                        .get(&class_name)
                        .ok_or_else(|| self.error(span, format!("Unknown class: {class_name}")))?
                        .methods
                        .get(attr)
                        .cloned();
                    if let Some(sig) = sig {
                        self.check_call_args(&sig, args, span, true)?;
                        return Ok(sig.ret.clone());
                    }
                }
                Err(self.error(span, "Unsupported method call"))
            }
            _ => Err(self.error(span, "Unsupported call target")),
        }
    }

    pub(super) fn check_call_args(
        &mut self,
        sig: &FunctionSig,
        args: &mut [Expr],
        span: Span,
        allow_self: bool,
    ) -> Result<(), CompileError> {
        let expected_params = sig.params.len();
        let mut arg_offset = 0;
        if allow_self && expected_params > 0 && args.len() + 1 == expected_params {
            arg_offset = 1;
        }
        if args.len() + arg_offset != expected_params {
            return Err(self.error(span, "Argument count mismatch"));
        }
        for (arg, param_ty) in args.iter_mut().zip(sig.params.iter().skip(arg_offset)) {
            let mut arg_ty = self.check_expr(arg, Some(param_ty))?;
            if matches!(param_ty, Type::Str)
                && !matches!(arg_ty, Type::Str)
                && matches!(arg_ty, Type::Int | Type::Float | Type::Bool)
            {
                let inner = arg.clone();
                *arg = Expr {
                    kind: ExprKind::Call {
                        func: Box::new(Expr {
                            kind: ExprKind::Name("str".to_string()),
                            span: arg.span,
                            ty: Some(Type::Str),
                        }),
                        args: vec![inner],
                    },
                    span: arg.span,
                    ty: Some(Type::Str),
                };
                arg_ty = Type::Str;
            }
            self.ensure_assignable(&arg_ty, param_ty, span)?;
        }
        Ok(())
    }

    pub(super) fn infer_callable_return(
        &mut self,
        func: &mut Expr,
        arg_ty: &Type,
        span: Span,
    ) -> Result<Type, CompileError> {
        match &mut func.kind {
            ExprKind::Name(name) => {
                match name.as_str() {
                    "str" => return Ok(Type::Str),
                    "int" => return Ok(Type::Int),
                    "float" => return Ok(Type::Float),
                    _ => {}
                }
                if let Some(sig) = self.ctx.functions.get(name).cloned() {
                    if sig.params.len() != 1 {
                        return Err(self.error(span, "Callable must take exactly one argument"));
                    }
                    if !matches!(sig.params[0], Type::Unknown) {
                        self.ensure_assignable(arg_ty, &sig.params[0], span)?;
                    }
                    return Ok(sig.ret);
                }
                if let Some(Type::Lambda { params, ret }) = self.lookup_var(name) {
                    let needs_refine = matches!(ret.as_ref(), Type::Unknown)
                        || params.iter().any(|p| matches!(p, Type::Unknown));
                    if needs_refine {
                        if let Some(lambda_expr) = self.lambda_defs.get(name).cloned() {
                            let expected = Type::Lambda {
                                params: vec![arg_ty.clone()],
                                ret: Box::new(Type::Unknown),
                            };
                            let mut expr_clone = lambda_expr.clone();
                            let inferred = self.check_expr(&mut expr_clone, Some(&expected))?;
                            if let Type::Lambda { params, ret } = inferred {
                                let updated = Type::Lambda {
                                    params: params.clone(),
                                    ret: ret.clone(),
                                };
                                self.set_var_type(name, updated);
                                return Ok(*ret);
                            }
                        }
                    }
                    if !params.is_empty() && params.len() != 1 {
                        return Err(self.error(span, "Callable must take exactly one argument"));
                    }
                    if let Some(param) = params.first() {
                        if !matches!(param, Type::Unknown) {
                            self.ensure_assignable(arg_ty, param, span)?;
                        }
                    }
                    return Ok(*ret);
                }
                let ty = self.check_expr(func, None)?;
                if let Type::Lambda { ret, .. } = ty {
                    return Ok(*ret);
                }
                Ok(Type::Unknown)
            }
            ExprKind::Lambda { params, body } => {
                if params.len() != 1 {
                    return Err(self.error(span, "Callable must take exactly one argument"));
                }
                self.scopes.push(HashMap::new());
                self.insert_var(&params[0], arg_ty.clone(), span)?;
                let ret_ty = self.check_expr(body, None)?;
                self.scopes.pop();
                func.ty = Some(Type::Lambda {
                    params: vec![arg_ty.clone()],
                    ret: Box::new(ret_ty.clone()),
                });
                Ok(ret_ty)
            }
            _ => {
                let ty = self.check_expr(func, None)?;
                if let Type::Lambda { ret, .. } = ty {
                    Ok(*ret)
                } else {
                    Ok(Type::Unknown)
                }
            }
        }
    }
}
