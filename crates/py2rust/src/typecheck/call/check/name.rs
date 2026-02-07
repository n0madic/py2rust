// Name-based call target type checking.

use super::super::super::*;
use crate::builtin::registry::find_builtin;
use crate::callspec::validate_call_shape;
use crate::stdlib::registry::{find_stdlib_method, resolve_module};

impl<'a> TypeChecker<'a> {
    pub(super) fn check_call_name(
        &mut self,
        func: &mut Expr,
        args: &mut [Expr],
        keywords: &mut [KeywordArg],
        expected: Option<&Type>,
        span: Span,
    ) -> Result<Type, CompileError> {
        let ExprKind::Name(name) = &mut func.kind else {
            return Err(self.error(span, "Internal error: expected name call target"));
        };
        let builtin_spec = find_builtin(name.as_str()).copied();
        if let Some(spec) = builtin_spec {
            let callable = format!("{name}()");
            let keyword_names: Vec<Option<&str>> =
                keywords.iter().map(|kw| kw.name.as_deref()).collect();
            if let Err(shape_err) =
                validate_call_shape(&callable, spec.shape, args.len(), &keyword_names)
            {
                return Err(self.error(span, shape_err.message()));
            }
        }
        let stdlib_function_accepts_keywords =
            matches!(self.lookup_var(name), Some(Type::StdlibFunction { .. }));
        if !keywords.is_empty()
            && builtin_spec.is_none()
            && !stdlib_function_accepts_keywords
            && !self.ctx.functions.contains_key(name)
            && !self.ctx.classes.contains_key(name)
            && !matches!(self.lookup_var(name), Some(Type::Lambda { .. }))
        {
            return Err(self.error(
                span,
                format!("Keyword arguments are not supported for {name}()"),
            ));
        }
        if name == "print" {
            for arg in args {
                self.check_expr(arg, None)?;
            }
            let mut seen_sep = false;
            let mut seen_end = false;
            for kw in keywords.iter_mut() {
                let Some(kw_name) = kw.name.as_deref() else {
                    return Err(self.error(
                        span,
                        "Call-site **kwargs unpacking is not supported for print()",
                    ));
                };
                if kw_name == "sep" {
                    if seen_sep {
                        return Err(self.error(span, "Multiple values for keyword argument `sep`"));
                    }
                    seen_sep = true;
                } else if kw_name == "end" {
                    if seen_end {
                        return Err(self.error(span, "Multiple values for keyword argument `end`"));
                    }
                    seen_end = true;
                } else {
                    return Err(self.error(
                        span,
                        format!("Unknown keyword argument `{kw_name}` for print()"),
                    ));
                }
                let kw_ty = self.check_expr(&mut kw.value, Some(&Type::Str))?;
                self.ensure_assignable(&kw_ty, &Type::Str, span)?;
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
        if name == "open" {
            if args.is_empty() || args.len() > 2 {
                return Err(self.error(span, "open() expects one or two arguments"));
            }
            let path_ty = self.check_expr(&mut args[0], Some(&Type::Str))?;
            self.ensure_assignable(&path_ty, &Type::Str, span)?;
            if args.len() == 2 {
                let mode_ty = self.check_expr(&mut args[1], Some(&Type::Str))?;
                self.ensure_assignable(&mode_ty, &Type::Str, span)?;
            }
            return Ok(Type::Custom("__py2rust_file".to_string()));
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
            // Keep float inputs as float for one-arg round().
            // This preserves stable formatting in string-heavy call sites.
            return Ok(if matches!(first_ty, Type::Float) {
                Type::Float
            } else {
                Type::Int
            });
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
        if name == "set" {
            if args.len() > 1 {
                return Err(self.error(span, "set() expects zero or one argument"));
            }
            if args.is_empty() {
                if let Some(Type::Set(inner)) = expected {
                    return Ok(Type::Set(inner.clone()));
                }
                return Ok(Type::Set(Box::new(Type::Unknown)));
            }
            let iter_ty = self.check_expr(&mut args[0], None)?;
            let item_ty = self.iter_item_type(&iter_ty, span)?;
            return Ok(Type::Set(Box::new(item_ty)));
        }
        if name == "bytes" {
            if args.len() > 2 {
                return Err(self.error(span, "bytes() expects up to two arguments"));
            }
            if args.is_empty() {
                return Ok(Type::Bytes);
            }
            if args.len() == 2 {
                let first_ty = self.check_expr(&mut args[0], Some(&Type::Str))?;
                let enc_ty = self.check_expr(&mut args[1], Some(&Type::Str))?;
                self.ensure_assignable(&first_ty, &Type::Str, span)?;
                self.ensure_assignable(&enc_ty, &Type::Str, span)?;
                return Ok(Type::Bytes);
            }
            let arg_ty = self.check_expr(&mut args[0], None)?;
            match arg_ty {
                Type::Bytes => Ok(Type::Bytes),
                Type::Int => Ok(Type::Bytes),
                Type::List(inner) | Type::Set(inner) | Type::Iterator(inner) => {
                    if !matches!(inner.as_ref(), Type::Unknown) {
                        self.ensure_assignable(inner.as_ref(), &Type::Int, span)?;
                    }
                    Ok(Type::Bytes)
                }
                Type::Tuple(items) => {
                    if items.is_empty() {
                        return Ok(Type::Bytes);
                    }
                    if !items.iter().all(|t| t == &items[0]) {
                        return Err(self.error(span, "bytes() tuple argument must be homogeneous"));
                    }
                    if !matches!(items[0], Type::Unknown) {
                        self.ensure_assignable(&items[0], &Type::Int, span)?;
                    }
                    Ok(Type::Bytes)
                }
                Type::Str => Err(self.error(span, "bytes() expects encoding when called with str")),
                Type::Unknown => Ok(Type::Bytes),
                _ => Err(self.error(span, "bytes() expects int or iterable of ints")),
            }?;
            return Ok(Type::Bytes);
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
            if args.is_empty() || args.len() > 2 {
                return Err(self.error(span, "enumerate() expects one or two arguments"));
            }
            let iter_ty = self.check_expr(&mut args[0], None)?;
            if args.len() == 2 {
                let start_ty = self.check_expr(&mut args[1], Some(&Type::Int))?;
                self.ensure_assignable(&start_ty, &Type::Int, span)?;
            }
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
        if name == "sorted" {
            if args.len() != 1 {
                return Err(self.error(span, "sorted() expects one positional argument"));
            }
            let mut seen_key = false;
            let mut seen_reverse = false;
            let iter_ty = self.check_expr(&mut args[0], None)?;
            let item_ty = self.iter_item_type(&iter_ty, span)?;
            for kw in keywords.iter_mut() {
                let Some(kw_name) = kw.name.as_deref() else {
                    return Err(self.error(
                        span,
                        "Call-site **kwargs unpacking is not supported for sorted()",
                    ));
                };
                match kw_name {
                    "key" => {
                        if seen_key {
                            return Err(
                                self.error(span, "Multiple values for keyword argument `key`")
                            );
                        }
                        seen_key = true;
                        let _ = self.infer_callable_return(&mut kw.value, &item_ty, span)?;
                    }
                    "reverse" => {
                        if seen_reverse {
                            return Err(
                                self.error(span, "Multiple values for keyword argument `reverse`")
                            );
                        }
                        seen_reverse = true;
                        let reverse_ty = self.check_expr(&mut kw.value, Some(&Type::Bool))?;
                        self.ensure_assignable(&reverse_ty, &Type::Bool, span)?;
                    }
                    _ => {
                        return Err(self.error(
                            span,
                            format!("Unknown keyword argument `{kw_name}` for sorted()"),
                        ));
                    }
                }
            }
            return Ok(Type::List(Box::new(item_ty)));
        }
        if name == "max" || name == "min" {
            if args.is_empty() {
                return Err(self.error(span, "max()/min() expect at least one argument"));
            }
            let mut key_arg: Option<&mut Expr> = None;
            for kw in keywords.iter_mut() {
                let Some(kw_name) = kw.name.as_deref() else {
                    return Err(self.error(
                        span,
                        "Call-site **kwargs unpacking is not supported for max()/min()",
                    ));
                };
                if kw_name != "key" {
                    return Err(self.error(
                        span,
                        format!("Unknown keyword argument `{kw_name}` for max()/min()"),
                    ));
                }
                if key_arg.is_some() {
                    return Err(self.error(span, "Multiple values for keyword argument `key`"));
                }
                key_arg = Some(&mut kw.value);
            }
            if args.len() == 1 {
                let iter_ty = self.check_expr(&mut args[0], None)?;
                let item_ty = self.iter_item_type(&iter_ty, span)?;
                if let Some(key_expr) = key_arg {
                    let _ = self.infer_callable_return(key_expr, &item_ty, span)?;
                }
                if matches!(item_ty, Type::Unknown) {
                    if let Some(expected_ty) = expected {
                        if !matches!(expected_ty, Type::Unknown) {
                            return Ok(expected_ty.clone());
                        }
                    }
                }
                return Ok(item_ty);
            }
            if key_arg.is_some() {
                return Err(self.error(
                    span,
                    "max()/min() with key= currently supports only iterable form",
                ));
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
                        return Err(self.error(span, "max()/min() expect numeric arguments"));
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
        if name == "ascii" {
            if args.len() != 1 {
                return Err(self.error(span, "ascii() expects one argument"));
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
            let numeric =
                |ty: &Type| matches!(ty, Type::Int | Type::Float | Type::Bool | Type::Unknown);
            if !numeric(&left_ty) || !numeric(&right_ty) {
                return Err(self.error(span, "divmod() expects numeric arguments"));
            }
            let use_float = matches!(left_ty, Type::Float) || matches!(right_ty, Type::Float);
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
        if name == "iter" {
            if args.len() != 1 {
                return Err(self.error(span, "iter() expects one argument"));
            }
            let iter_ty = self.check_expr(&mut args[0], None)?;
            let item_ty = self.iter_item_type(&iter_ty, span)?;
            return Ok(Type::Iterator(Box::new(item_ty)));
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
        if name == "super" {
            if !args.is_empty() {
                return Err(self.error(span, "super() takes no arguments"));
            }
            let class_name = self
                .current_class
                .as_ref()
                .ok_or_else(|| self.error(span, "super() outside of class"))?;
            let base = self
                .ctx
                .classes
                .get(class_name)
                .and_then(|info| info.base.clone())
                .ok_or_else(|| self.error(span, "super() has no base class"))?;
            return Ok(Type::Custom(base));
        }
        if let Some(class_info) = self.ctx.classes.get(name) {
            if let Some(init_sig) = class_info.init.clone() {
                self.check_call_args(&init_sig, args, keywords, span, true)?;
            } else {
                if !class_info.fields.is_empty() {
                    return Err(self.error(span, format!("Class {name} is missing __init__")));
                }
                if !args.is_empty() || !keywords.is_empty() {
                    return Err(self.error(span, format!("Class {name} takes no arguments")));
                }
            }
            if let Some(Type::Union(union_name)) = expected {
                let variants = self
                    .ctx
                    .unions
                    .get(union_name)
                    .ok_or_else(|| self.error(span, format!("Unknown union: {union_name}")))?
                    .variants
                    .clone();
                if variants.contains(name) {
                    return Ok(Type::Union(union_name.clone()));
                }
            }
            return Ok(Type::Custom(name.clone()));
        }
        if let Some(sig) = self.ctx.functions.get(name).cloned() {
            self.check_call_args(&sig, args, keywords, span, false)?;
            return Ok(sig.ret.clone());
        }
        if let Some(var_ty) = self.lookup_var(name) {
            if let Type::StdlibFunction { module, method } = &var_ty {
                func.ty = Some(Type::StdlibFunction {
                    module: module.clone(),
                    method: method.clone(),
                });
                let module_id = resolve_module(module.as_str()).ok_or_else(|| {
                    self.error(
                        span,
                        format!("module '{module}' is not registered in stdlib registry"),
                    )
                })?;
                let spec = find_stdlib_method(module_id, method.as_str()).ok_or_else(|| {
                    self.error(span, format!("{module} has no supported member '{method}'"))
                })?;
                return self.check_stdlib_call(spec, args, keywords, span);
            }
            if let Type::Lambda { params, ret } = var_ty {
                if params.len() != args.len() && !params.is_empty() {
                    return Err(self.error(span, "Argument count mismatch"));
                }
                let mut refined_params = params.clone();
                for (idx, (arg, param_ty)) in args.iter_mut().zip(params.iter()).enumerate() {
                    if !matches!(param_ty, Type::Unknown) {
                        let arg_ty = self.check_expr(arg, Some(param_ty))?;
                        self.ensure_assignable(&arg_ty, param_ty, span)?;
                    } else {
                        let arg_ty = self.check_expr(arg, None)?;
                        if idx < refined_params.len() {
                            refined_params[idx] = arg_ty;
                        }
                    }
                }
                let mut refined_ret = *ret.clone();
                if matches!(refined_ret, Type::Unknown) {
                    if let Some(expected_ty) = expected {
                        if !matches!(expected_ty, Type::Unknown) {
                            // Assignment/return context can constrain callable return type.
                            refined_ret = expected_ty.clone();
                        }
                    }
                }
                if matches!(refined_ret, Type::Unknown) {
                    if let Some(lambda_expr) = self.lambda_defs.get(name).cloned() {
                        let expected = Type::Lambda {
                            params: refined_params.clone(),
                            ret: Box::new(Type::Unknown),
                        };
                        let mut expr_clone = lambda_expr;
                        let inferred = self.check_expr(&mut expr_clone, Some(&expected))?;
                        if let Type::Lambda { params, ret } = inferred {
                            refined_params = params;
                            refined_ret = *ret;
                        }
                    }
                }
                self.set_var_type(
                    name,
                    Type::Lambda {
                        params: refined_params,
                        ret: Box::new(refined_ret.clone()),
                    },
                );
                return Ok(refined_ret);
            }
            if matches!(var_ty, Type::Unknown) {
                let mut param_tys = Vec::new();
                for arg in args.iter_mut() {
                    let arg_ty = self.check_expr(arg, None)?;
                    param_tys.push(arg_ty);
                }
                let inferred_ret = expected
                    .filter(|ty| !matches!(ty, Type::Unknown))
                    .cloned()
                    .unwrap_or(Type::Unknown);
                let lambda = Type::Lambda {
                    params: param_tys,
                    ret: Box::new(inferred_ret),
                };
                self.set_var_type(name, lambda);
                return Ok(Type::Unknown);
            }
        }
        Err(self.error(span, "Unknown call target"))
    }
}
