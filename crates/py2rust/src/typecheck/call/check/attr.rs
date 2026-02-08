// Attribute-based call target type checking.

use super::super::super::*;
use crate::container::registry::{find_container_method, ContainerId};
use crate::stdlib::registry::{find_stdlib_method, resolve_module};

impl<'a> TypeChecker<'a> {
    pub(super) fn check_call_attr(
        &mut self,
        value: &mut Expr,
        attr: &String,
        args: &mut [Expr],
        keywords: &mut [KeywordArg],
        span: Span,
    ) -> Result<Type, CompileError> {
        // Catch unresolved stdlib module roots in chained calls such as
        // `urllib.parse.quote(...)` before generic attribute diagnostics.
        let mut root = &value.kind;
        loop {
            match root {
                ExprKind::Name(module_name) => {
                    if resolve_module(module_name.as_str()).is_some()
                        && self.lookup_var(module_name).is_none()
                    {
                        return Err(
                            self.error(span, format!("module '{module_name}' used without import"))
                        );
                    }
                    break;
                }
                ExprKind::Attr { value, .. } => {
                    root = &value.kind;
                }
                _ => break,
            }
        }
        let obj_ty = self.check_expr(value, None)?;
        if let Type::Module(module_name) = &obj_ty {
            let module_id = resolve_module(module_name.as_str()).ok_or_else(|| {
                self.error(
                    span,
                    format!("module '{module_name}' is not registered in stdlib registry"),
                )
            })?;
            let method = find_stdlib_method(module_id, attr.as_str()).ok_or_else(|| {
                self.error(
                    span,
                    format!("{module_name} has no supported member '{attr}'"),
                )
            })?;
            return self.check_stdlib_call(method, args, keywords, span);
        }
        if let Type::List(inner) = &obj_ty {
            if let Some(spec) = find_container_method(ContainerId::List, attr.as_str()) {
                let keyword_names: Vec<Option<&str>> =
                    keywords.iter().map(|kw| kw.name.as_deref()).collect();
                if let Err(shape_err) = spec.validate(args.len(), &keyword_names) {
                    return Err(self.error(span, shape_err.message()));
                }
            }
            if attr == "append" {
                if args.len() != 1 {
                    return Err(self.error(span, "list.append() expects one argument"));
                }
                let arg_ty = self.check_expr(&mut args[0], Some(inner))?;
                if !matches!(arg_ty, Type::Unknown) && !matches!(inner.as_ref(), Type::Unknown) {
                    self.ensure_assignable(&arg_ty, inner, span)?;
                }
                if matches!(inner.as_ref(), Type::Unknown) && !matches!(arg_ty, Type::Unknown) {
                    if let ExprKind::Name(name) = &value.kind {
                        self.set_var_type(name, Type::List(Box::new(arg_ty.clone())));
                    }
                }
                return Ok(Type::None);
            }
            if attr == "extend" {
                if args.len() != 1 {
                    return Err(self.error(span, "list.extend() expects one argument"));
                }
                let arg_ty = self.check_expr(&mut args[0], None)?;
                match arg_ty {
                    Type::List(arg_inner) => {
                        if !matches!(inner.as_ref(), Type::Unknown)
                            && !matches!(arg_inner.as_ref(), Type::Unknown)
                        {
                            self.ensure_assignable(&arg_inner, inner, span)?;
                        }
                        if matches!(inner.as_ref(), Type::Unknown)
                            && !matches!(arg_inner.as_ref(), Type::Unknown)
                        {
                            // Infer list element type from the source list.
                            if let ExprKind::Name(name) = &value.kind {
                                self.set_var_type(name, Type::List(Box::new((*arg_inner).clone())));
                            }
                        }
                        return Ok(Type::None);
                    }
                    Type::Tuple(items) => {
                        // Require homogeneous tuple elements to extend a list.
                        let mut candidate: Option<Type> = None;
                        for item in items.iter() {
                            if matches!(item, Type::Unknown) {
                                continue;
                            }
                            if let Some(existing) = candidate.as_ref() {
                                if existing.is_numeric() && item.is_numeric() {
                                    if matches!(existing, Type::Float)
                                        || matches!(item, Type::Float)
                                    {
                                        candidate = Some(Type::Float);
                                    }
                                } else if existing != item {
                                    return Err(self.error(
                                        span,
                                        "list.extend() requires homogeneous tuple elements",
                                    ));
                                }
                            } else {
                                candidate = Some(item.clone());
                            }
                        }
                        if let Some(elem_ty) = candidate.as_ref() {
                            if !matches!(inner.as_ref(), Type::Unknown) {
                                self.ensure_assignable(elem_ty, inner, span)?;
                            } else if let ExprKind::Name(name) = &value.kind {
                                self.set_var_type(name, Type::List(Box::new(elem_ty.clone())));
                            }
                        } else if !matches!(inner.as_ref(), Type::Unknown) {
                            // All tuple elements are unknown: keep list element type as-is.
                        }
                        return Ok(Type::None);
                    }
                    Type::Unknown => return Ok(Type::None),
                    _ => {
                        return Err(
                            self.error(span, "list.extend() expects a list or tuple argument")
                        )
                    }
                }
            }
            if attr == "pop" {
                if args.len() > 1 {
                    return Err(self.error(span, "list.pop() expects zero or one argument"));
                }
                if args.len() == 1 {
                    let arg_ty = self.check_expr(&mut args[0], Some(&Type::Int))?;
                    self.ensure_assignable(&arg_ty, &Type::Int, span)?;
                }
                return Ok((*inner.as_ref()).clone());
            }
            if attr == "insert" {
                if args.len() != 2 {
                    return Err(self.error(span, "list.insert() expects two arguments"));
                }
                let idx_ty = self.check_expr(&mut args[0], Some(&Type::Int))?;
                self.ensure_assignable(&idx_ty, &Type::Int, span)?;
                let val_ty = self.check_expr(&mut args[1], Some(inner))?;
                if !matches!(val_ty, Type::Unknown) && !matches!(inner.as_ref(), Type::Unknown) {
                    self.ensure_assignable(&val_ty, inner, span)?;
                }
                if matches!(inner.as_ref(), Type::Unknown) && !matches!(val_ty, Type::Unknown) {
                    if let ExprKind::Name(name) = &value.kind {
                        self.set_var_type(name, Type::List(Box::new(val_ty.clone())));
                    }
                }
                return Ok(Type::None);
            }
            if attr == "clear" {
                if !args.is_empty() {
                    return Err(self.error(span, "list.clear() expects no arguments"));
                }
                return Ok(Type::None);
            }
            if attr == "copy" {
                if !args.is_empty() {
                    return Err(self.error(span, "list.copy() expects no arguments"));
                }
                return Ok(Type::List(Box::new((*inner.as_ref()).clone())));
            }
            if attr == "reverse" {
                if !args.is_empty() {
                    return Err(self.error(span, "list.reverse() expects no arguments"));
                }
                return Ok(Type::None);
            }
            if attr == "index" {
                if args.len() != 1 {
                    return Err(self.error(span, "list.index() expects one argument"));
                }
                let arg_ty = self.check_expr(&mut args[0], Some(inner))?;
                if !matches!(arg_ty, Type::Unknown) && !matches!(inner.as_ref(), Type::Unknown) {
                    self.ensure_assignable(&arg_ty, inner, span)?;
                }
                if matches!(inner.as_ref(), Type::Unknown) && !matches!(arg_ty, Type::Unknown) {
                    if let ExprKind::Name(name) = &value.kind {
                        self.set_var_type(name, Type::List(Box::new(arg_ty.clone())));
                    }
                }
                return Ok(Type::Int);
            }
            if attr == "count" {
                if args.len() != 1 {
                    return Err(self.error(span, "list.count() expects one argument"));
                }
                let arg_ty = self.check_expr(&mut args[0], Some(inner))?;
                if !matches!(arg_ty, Type::Unknown) && !matches!(inner.as_ref(), Type::Unknown) {
                    self.ensure_assignable(&arg_ty, inner, span)?;
                }
                if matches!(inner.as_ref(), Type::Unknown) && !matches!(arg_ty, Type::Unknown) {
                    if let ExprKind::Name(name) = &value.kind {
                        self.set_var_type(name, Type::List(Box::new(arg_ty.clone())));
                    }
                }
                return Ok(Type::Int);
            }
            if attr == "remove" {
                if args.len() != 1 {
                    return Err(self.error(span, "list.remove() expects one argument"));
                }
                let arg_ty = self.check_expr(&mut args[0], Some(inner))?;
                if !matches!(arg_ty, Type::Unknown) && !matches!(inner.as_ref(), Type::Unknown) {
                    self.ensure_assignable(&arg_ty, inner, span)?;
                }
                if matches!(inner.as_ref(), Type::Unknown) && !matches!(arg_ty, Type::Unknown) {
                    if let ExprKind::Name(name) = &value.kind {
                        self.set_var_type(name, Type::List(Box::new(arg_ty.clone())));
                    }
                }
                return Ok(Type::None);
            }
            if attr == "sort" {
                if !args.is_empty() {
                    return Err(self.error(span, "list.sort() expects no arguments"));
                }
                match inner.as_ref() {
                    Type::Int | Type::Float | Type::Str | Type::Unknown => Ok(Type::None),
                    _ => Err(self.error(span, "list.sort() requires int, float, or str elements")),
                }?;
                return Ok(Type::None);
            }
        }
        if let Type::Dict(key_ty, val_ty) = &obj_ty {
            if let Some(spec) = find_container_method(ContainerId::Dict, attr.as_str()) {
                let keyword_names: Vec<Option<&str>> =
                    keywords.iter().map(|kw| kw.name.as_deref()).collect();
                if let Err(shape_err) = spec.validate(args.len(), &keyword_names) {
                    return Err(self.error(span, shape_err.message()));
                }
            }
            if attr == "get" {
                if args.is_empty() || args.len() > 2 {
                    return Err(self.error(span, "dict.get() expects one or two arguments"));
                }
                let arg_key = self.check_expr(&mut args[0], Some(key_ty))?;
                self.ensure_assignable(&arg_key, key_ty, span)?;
                if args.len() == 2 {
                    let default_ty = self.check_expr(&mut args[1], Some(val_ty))?;
                    if !matches!(default_ty, Type::Unknown)
                        && !matches!(val_ty.as_ref(), Type::Unknown)
                    {
                        self.ensure_assignable(&default_ty, val_ty, span)?;
                    }
                }
                return Ok(*val_ty.clone());
            }
            if attr == "clear" {
                if !args.is_empty() {
                    return Err(self.error(span, "dict.clear() expects no arguments"));
                }
                return Ok(Type::None);
            }
            if attr == "copy" {
                if !args.is_empty() {
                    return Err(self.error(span, "dict.copy() expects no arguments"));
                }
                return Ok(Type::Dict(key_ty.clone(), val_ty.clone()));
            }
            if attr == "pop" {
                if args.is_empty() || args.len() > 2 {
                    return Err(self.error(span, "dict.pop() expects one or two arguments"));
                }
                let arg_key = self.check_expr(&mut args[0], Some(key_ty))?;
                self.ensure_assignable(&arg_key, key_ty, span)?;
                if args.len() == 2 {
                    let default_ty = self.check_expr(&mut args[1], Some(val_ty))?;
                    if !matches!(default_ty, Type::Unknown)
                        && !matches!(val_ty.as_ref(), Type::Unknown)
                    {
                        self.ensure_assignable(&default_ty, val_ty, span)?;
                    }
                }
                return Ok(*val_ty.clone());
            }
            if attr == "update" {
                if args.len() != 1 {
                    return Err(self.error(span, "dict.update() expects one argument"));
                }
                let arg_ty = self.check_expr(&mut args[0], None)?;
                if let Type::Dict(k2, v2) = arg_ty {
                    if !matches!(key_ty.as_ref(), Type::Unknown)
                        && !matches!(k2.as_ref(), Type::Unknown)
                    {
                        self.ensure_assignable(&k2, key_ty, span)?;
                    }
                    if !matches!(val_ty.as_ref(), Type::Unknown)
                        && !matches!(v2.as_ref(), Type::Unknown)
                    {
                        self.ensure_assignable(&v2, val_ty, span)?;
                    }
                    return Ok(Type::None);
                }
                return Err(self.error(span, "dict.update() expects a dict argument"));
            }
            if attr == "keys" {
                if !args.is_empty() {
                    return Err(self.error(span, "dict.keys() expects no arguments"));
                }
                return Ok(Type::Iterator(Box::new(*key_ty.clone())));
            }
            if attr == "values" {
                if !args.is_empty() {
                    return Err(self.error(span, "dict.values() expects no arguments"));
                }
                return Ok(Type::Iterator(Box::new(*val_ty.clone())));
            }
            if attr == "setdefault" {
                if args.is_empty() || args.len() > 2 {
                    return Err(self.error(span, "dict.setdefault() expects one or two arguments"));
                }
                let arg_key = self.check_expr(&mut args[0], Some(key_ty))?;
                self.ensure_assignable(&arg_key, key_ty, span)?;
                if args.len() == 1 {
                    if !matches!(
                        val_ty.as_ref(),
                        Type::Option(_) | Type::None | Type::Unknown
                    ) {
                        return Err(self.error(
                            span,
                            "dict.setdefault() without default requires dict values that can be None",
                        ));
                    }
                    return Ok(*val_ty.clone());
                }

                let default_ty = self.check_expr(&mut args[1], Some(val_ty))?;
                if !matches!(default_ty, Type::Unknown) && !matches!(val_ty.as_ref(), Type::Unknown)
                {
                    self.ensure_assignable(&default_ty, val_ty, span)?;
                }

                if matches!(val_ty.as_ref(), Type::Unknown) && !matches!(default_ty, Type::Unknown)
                {
                    if let ExprKind::Name(name) = &value.kind {
                        self.set_var_type(
                            name,
                            Type::Dict(key_ty.clone(), Box::new(default_ty.clone())),
                        );
                    }
                    return Ok(default_ty);
                }
                return Ok(*val_ty.clone());
            }
        }
        if let Type::Set(inner) = &obj_ty {
            if let Some(spec) = find_container_method(ContainerId::Set, attr.as_str()) {
                let keyword_names: Vec<Option<&str>> =
                    keywords.iter().map(|kw| kw.name.as_deref()).collect();
                if let Err(shape_err) = spec.validate(args.len(), &keyword_names) {
                    return Err(self.error(span, shape_err.message()));
                }
            }
            if attr == "add" {
                let arg_ty = self.check_expr(&mut args[0], Some(inner))?;
                if !matches!(arg_ty, Type::Unknown) && !matches!(inner.as_ref(), Type::Unknown) {
                    self.ensure_assignable(&arg_ty, inner, span)?;
                }
                if matches!(inner.as_ref(), Type::Unknown) && !matches!(arg_ty, Type::Unknown) {
                    if let ExprKind::Name(name) = &value.kind {
                        self.set_var_type(name, Type::Set(Box::new(arg_ty.clone())));
                    }
                }
                return Ok(Type::None);
            }
            if attr == "remove" {
                let arg_ty = self.check_expr(&mut args[0], Some(inner))?;
                if !matches!(arg_ty, Type::Unknown) && !matches!(inner.as_ref(), Type::Unknown) {
                    self.ensure_assignable(&arg_ty, inner, span)?;
                }
                return Ok(Type::None);
            }
            if attr == "discard" {
                let arg_ty = self.check_expr(&mut args[0], Some(inner))?;
                if !matches!(arg_ty, Type::Unknown) && !matches!(inner.as_ref(), Type::Unknown) {
                    self.ensure_assignable(&arg_ty, inner, span)?;
                }
                return Ok(Type::None);
            }
            if attr == "clear" {
                return Ok(Type::None);
            }
            if attr == "copy" {
                return Ok(Type::Set(Box::new((*inner.as_ref()).clone())));
            }
            if attr == "extend" {
                let iter_ty = self.check_expr(&mut args[0], None)?;
                let item_ty = self.iter_item_type(&iter_ty, span)?;
                if !matches!(item_ty, Type::Unknown) && !matches!(inner.as_ref(), Type::Unknown) {
                    self.ensure_assignable(&item_ty, inner, span)?;
                }
                if matches!(inner.as_ref(), Type::Unknown) && !matches!(item_ty, Type::Unknown) {
                    if let ExprKind::Name(name) = &value.kind {
                        self.set_var_type(name, Type::Set(Box::new(item_ty.clone())));
                    }
                }
                return Ok(Type::None);
            }
            if attr == "pop" {
                return Ok((*inner.as_ref()).clone());
            }
        }
        if let Type::Custom(class_name) = &obj_ty {
            if class_name == "__py_re_match" {
                if !keywords.is_empty() {
                    return Err(self.error(span, "Keyword arguments are not supported"));
                }
                if attr == "group" {
                    if args.len() != 1 {
                        return Err(self.error(span, "re.Match.group() expects one argument"));
                    }
                    let index_ty = self.check_expr(&mut args[0], Some(&Type::Int))?;
                    self.ensure_assignable(&index_ty, &Type::Int, span)?;
                    return Ok(Type::Str);
                }
                if attr == "span" {
                    if !args.is_empty() {
                        return Err(self.error(span, "re.Match.span() expects no arguments"));
                    }
                    return Ok(Type::Tuple(vec![Type::Int, Type::Int]));
                }
            }
            if class_name == "__py_urllib_parse_result" {
                if !keywords.is_empty() {
                    return Err(self.error(
                        span,
                        "Keyword arguments are not supported for urllib.parse.ParseResult methods",
                    ));
                }
                if attr == "geturl" {
                    if !args.is_empty() {
                        return Err(self.error(
                            span,
                            "urllib.parse.ParseResult.geturl() expects no arguments",
                        ));
                    }
                    return Ok(Type::Str);
                }
            }
            if class_name == "__py_urllib_response" {
                if !keywords.is_empty() {
                    return Err(self.error(
                        span,
                        "Keyword arguments are not supported for urllib.request response methods",
                    ));
                }
                if attr == "read" {
                    if !args.is_empty() {
                        return Err(
                            self.error(span, "urllib.request response read() expects no arguments")
                        );
                    }
                    return Ok(Type::Str);
                }
                if attr == "getcode" {
                    if !args.is_empty() {
                        return Err(self.error(
                            span,
                            "urllib.request response getcode() expects no arguments",
                        ));
                    }
                    return Ok(Type::Int);
                }
                if attr == "geturl" {
                    if !args.is_empty() {
                        return Err(self.error(
                            span,
                            "urllib.request response geturl() expects no arguments",
                        ));
                    }
                    return Ok(Type::Str);
                }
            }
            if class_name == "__py_file" {
                if attr == "read" {
                    if args.len() > 1 {
                        return Err(self.error(span, "file.read() expects zero or one argument"));
                    }
                    if args.len() == 1 {
                        let arg_ty = self.check_expr(&mut args[0], Some(&Type::Int))?;
                        self.ensure_assignable(&arg_ty, &Type::Int, span)?;
                    }
                    return Ok(Type::Str);
                }
                if attr == "readline" {
                    if !args.is_empty() {
                        return Err(self.error(span, "file.readline() expects no arguments"));
                    }
                    return Ok(Type::Str);
                }
                if attr == "readlines" {
                    if !args.is_empty() {
                        return Err(self.error(span, "file.readlines() expects no arguments"));
                    }
                    return Ok(Type::List(Box::new(Type::Str)));
                }
                if attr == "write" {
                    if args.len() != 1 {
                        return Err(self.error(span, "file.write() expects one argument"));
                    }
                    let arg_ty = self.check_expr(&mut args[0], Some(&Type::Str))?;
                    self.ensure_assignable(&arg_ty, &Type::Str, span)?;
                    return Ok(Type::Int);
                }
                if attr == "close" {
                    if !args.is_empty() {
                        return Err(self.error(span, "file.close() expects no arguments"));
                    }
                    return Ok(Type::None);
                }
            }
        }
        // String method support.
        if matches!(obj_ty, Type::Str) {
            if attr == "upper"
                || attr == "lower"
                || attr == "title"
                || attr == "capitalize"
                || attr == "swapcase"
            {
                if !keywords.is_empty() {
                    return Err(self.error(span, "Keyword arguments are not supported"));
                }
                if !args.is_empty() {
                    return Err(self.error(span, format!("str.{attr}() expects no arguments")));
                }
                return Ok(Type::Str);
            }
            if attr == "isdigit"
                || attr == "isalpha"
                || attr == "isalnum"
                || attr == "isspace"
                || attr == "isupper"
                || attr == "islower"
            {
                if !keywords.is_empty() {
                    return Err(self.error(span, "Keyword arguments are not supported"));
                }
                if !args.is_empty() {
                    return Err(self.error(span, format!("str.{attr}() expects no arguments")));
                }
                return Ok(Type::Bool);
            }
            if attr == "startswith" || attr == "endswith" || attr == "find" || attr == "count" {
                if !keywords.is_empty() {
                    return Err(self.error(span, "Keyword arguments are not supported"));
                }
                if args.len() != 1 {
                    return Err(self.error(span, format!("str.{attr}() expects one argument")));
                }
                let arg_ty = self.check_expr(&mut args[0], Some(&Type::Str))?;
                self.ensure_assignable(&arg_ty, &Type::Str, span)?;
                return Ok(if attr == "find" || attr == "count" {
                    Type::Int
                } else {
                    Type::Bool
                });
            }
            if attr == "replace" {
                if !keywords.is_empty() {
                    return Err(self.error(span, "Keyword arguments are not supported"));
                }
                if args.len() != 2 {
                    return Err(self.error(span, "str.replace() expects two arguments"));
                }
                let old_ty = self.check_expr(&mut args[0], Some(&Type::Str))?;
                let new_ty = self.check_expr(&mut args[1], Some(&Type::Str))?;
                self.ensure_assignable(&old_ty, &Type::Str, span)?;
                self.ensure_assignable(&new_ty, &Type::Str, span)?;
                return Ok(Type::Str);
            }
            if attr == "strip" || attr == "lstrip" || attr == "rstrip" {
                if !keywords.is_empty() {
                    return Err(self.error(span, "Keyword arguments are not supported"));
                }
                if args.len() > 1 {
                    return Err(
                        self.error(span, format!("str.{attr}() expects zero or one argument"))
                    );
                }
                if args.len() == 1 {
                    let chars_ty = self.check_expr(&mut args[0], Some(&Type::Str))?;
                    self.ensure_assignable(&chars_ty, &Type::Str, span)?;
                }
                return Ok(Type::Str);
            }
            if attr == "split" {
                if !keywords.is_empty() {
                    return Err(self.error(span, "Keyword arguments are not supported"));
                }
                if args.len() > 2 {
                    return Err(self.error(span, "str.split() expects up to two arguments"));
                }
                if !args.is_empty() {
                    let sep_ty = self.check_expr(&mut args[0], Some(&Type::Str))?;
                    self.ensure_assignable(&sep_ty, &Type::Str, span)?;
                }
                if args.len() == 2 {
                    let max_ty = self.check_expr(&mut args[1], Some(&Type::Int))?;
                    self.ensure_assignable(&max_ty, &Type::Int, span)?;
                }
                return Ok(Type::List(Box::new(Type::Str)));
            }
            if attr == "join" {
                if !keywords.is_empty() {
                    return Err(self.error(span, "Keyword arguments are not supported"));
                }
                if args.len() != 1 {
                    return Err(self.error(span, "str.join() expects one argument"));
                }
                let iter_ty = self.check_expr(&mut args[0], None)?;
                match iter_ty {
                    Type::Str | Type::Unknown => {}
                    Type::List(inner) | Type::Set(inner) | Type::Iterator(inner) => {
                        if !matches!(inner.as_ref(), Type::Unknown) {
                            self.ensure_assignable(inner.as_ref(), &Type::Str, span)?;
                        }
                    }
                    Type::Tuple(items) => {
                        for item_ty in items {
                            if !matches!(item_ty, Type::Unknown) {
                                self.ensure_assignable(&item_ty, &Type::Str, span)?;
                            }
                        }
                    }
                    _ => return Err(self.error(span, "str.join() expects an iterable of strings")),
                }
                return Ok(Type::Str);
            }
            if attr == "center" || attr == "ljust" || attr == "rjust" {
                if !keywords.is_empty() {
                    return Err(self.error(span, "Keyword arguments are not supported"));
                }
                if args.is_empty() || args.len() > 2 {
                    return Err(
                        self.error(span, format!("str.{attr}() expects one or two arguments"))
                    );
                }
                let width_ty = self.check_expr(&mut args[0], Some(&Type::Int))?;
                self.ensure_assignable(&width_ty, &Type::Int, span)?;
                if args.len() == 2 {
                    let fill_ty = self.check_expr(&mut args[1], Some(&Type::Str))?;
                    self.ensure_assignable(&fill_ty, &Type::Str, span)?;
                }
                return Ok(Type::Str);
            }
            if attr == "zfill" {
                if !keywords.is_empty() {
                    return Err(self.error(span, "Keyword arguments are not supported"));
                }
                if args.len() != 1 {
                    return Err(self.error(span, "str.zfill() expects one argument"));
                }
                let width_ty = self.check_expr(&mut args[0], Some(&Type::Int))?;
                self.ensure_assignable(&width_ty, &Type::Int, span)?;
                return Ok(Type::Str);
            }
            if attr == "format" {
                for arg in args.iter_mut() {
                    let _ = self.check_expr(arg, None)?;
                }
                for kw in keywords.iter_mut() {
                    if kw.name.is_none() {
                        return Err(
                            self.error(span, "Call-site **kwargs unpacking is not supported")
                        );
                    }
                    let _ = self.check_expr(&mut kw.value, None)?;
                }
                return Ok(Type::Str);
            }
        }
        if let Type::Custom(ref class_name) = obj_ty {
            let class_info = self
                .ctx
                .classes
                .get(class_name)
                .ok_or_else(|| self.error(span, format!("Unknown class: {class_name}")))?;
            let sig = class_info.methods.get(attr).cloned();
            if let Some(sig) = sig {
                let kind = class_info
                    .method_kinds
                    .get(attr)
                    .copied()
                    .unwrap_or(MethodKind::Instance);
                match kind {
                    MethodKind::Static => {
                        self.check_call_args(&sig, args, keywords, span, false)?;
                    }
                    MethodKind::Class => {
                        self.check_call_args(&sig, args, keywords, span, true)?;
                    }
                    MethodKind::Instance => {
                        if matches!(&value.kind, ExprKind::Name(n) if n == class_name) {
                            return Err(self.error(span, "Instance methods require an instance"));
                        }
                        self.check_call_args(&sig, args, keywords, span, true)?;
                    }
                }
                return Ok(sig.ret.clone());
            }
        }
        if let Type::Iterator(inner) = &obj_ty {
            if attr == "send" {
                if !keywords.is_empty() {
                    return Err(self.error(span, "Keyword arguments are not supported"));
                }
                if args.len() != 1 {
                    return Err(self.error(span, "iterator.send() expects one argument"));
                }
                let arg_ty = self.check_expr(&mut args[0], Some(inner.as_ref()))?;
                if !matches!(inner.as_ref(), Type::Unknown) && !matches!(arg_ty, Type::Unknown) {
                    self.ensure_assignable(&arg_ty, inner.as_ref(), span)?;
                }
                return Ok(*inner.clone());
            }
            if attr == "close" {
                if !keywords.is_empty() {
                    return Err(self.error(span, "Keyword arguments are not supported"));
                }
                if !args.is_empty() {
                    return Err(self.error(span, "iterator.close() expects no arguments"));
                }
                return Ok(Type::None);
            }
        }
        // Handle method calls on Union types by checking all variants have the method.
        if let Type::Union(ref union_name) = obj_ty {
            if let Some(union_info) = self.ctx.unions.get(union_name) {
                // Find the method signature from the first variant that has it.
                let mut found_sig: Option<FunctionSig> = None;
                let mut all_have_method = true;
                for variant in &union_info.variants {
                    if let Some(class_info) = self.ctx.classes.get(variant) {
                        if let Some(sig) = class_info.methods.get(attr) {
                            if found_sig.is_none() {
                                found_sig = Some(sig.clone());
                            }
                        } else {
                            all_have_method = false;
                            break;
                        }
                    } else {
                        all_have_method = false;
                        break;
                    }
                }
                if all_have_method {
                    if let Some(sig) = found_sig {
                        self.check_call_args(&sig, args, keywords, span, true)?;
                        return Ok(sig.ret.clone());
                    }
                }
                return Err(self.error(
                    span,
                    format!(
                        "Method '{}' not available on all variants of union '{}'",
                        attr, union_name
                    ),
                ));
            }
        }
        Err(self.error(span, "Unsupported method call"))
    }
}
