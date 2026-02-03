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
                    if args.len() != 1 {
                        return Err(self.error(span, "list() expects one argument"));
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
                    if args.len() != 1 {
                        return Err(self.error(span, "max()/min() expect one argument"));
                    }
                    let iter_ty = self.check_expr(&mut args[0], None)?;
                    let item_ty = self.iter_item_type(&iter_ty, span)?;
                    return Ok(item_ty);
                }
                if name == "int" {
                    if args.len() != 1 {
                        return Err(self.error(span, "int() expects one argument"));
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
                    if args.len() != 1 {
                        return Err(self.error(span, "float() expects one argument"));
                    }
                    let _ = self.check_expr(&mut args[0], None)?;
                    if let ExprKind::Name(var) = &args[0].kind {
                        if matches!(self.lookup_var(var), Some(Type::Unknown)) {
                            self.set_var_type(var, Type::Str);
                        }
                    }
                    return Ok(Type::Float);
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
                if name == "type" {
                    if args.len() != 1 {
                        return Err(self.error(span, "type() expects one argument"));
                    }
                    let _ = self.check_expr(&mut args[0], None)?;
                    return Ok(Type::Unknown);
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
