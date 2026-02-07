// Attribute method call lowering.

mod class_union;
mod dict;
mod list;
mod set;
mod string_file;

use super::super::*;
use crate::stdlib::registry::{resolve_method, resolve_module};

impl<'a> Codegen<'a> {
    /// Lower attribute-based method calls with special cases for collections and format().
    pub(super) fn gen_attr_call(
        &mut self,
        value: &Expr,
        attr: &str,
        args: &[Expr],
        keywords: &[KeywordArg],
    ) -> Result<String, CompileError> {
        if let Some(Type::Module(module_name)) = value.ty.as_ref() {
            let module_id = resolve_module(module_name.as_str()).ok_or_else(|| {
                self.error(
                    value.span,
                    format!("module '{module_name}' is not registered in stdlib registry"),
                )
            })?;
            let spec = resolve_method(module_id, attr).ok_or_else(|| {
                self.error(
                    value.span,
                    format!("{module_name} has no supported member '{attr}'"),
                )
            })?;
            return self.gen_stdlib_call(value.span, spec, args, keywords);
        }

        if matches!(value.ty.as_ref(), Some(Type::Str))
            && matches!(
                attr,
                "upper"
                    | "lower"
                    | "strip"
                    | "startswith"
                    | "endswith"
                    | "find"
                    | "replace"
                    | "split"
                    | "join"
                    | "count"
                    | "title"
                    | "capitalize"
                    | "swapcase"
                    | "lstrip"
                    | "rstrip"
                    | "center"
                    | "ljust"
                    | "rjust"
                    | "zfill"
                    | "isdigit"
                    | "isalpha"
                    | "isalnum"
                    | "isspace"
                    | "isupper"
                    | "islower"
            )
        {
            return self.gen_str_attr_call(value, attr, args, keywords);
        }

        if matches!(value.ty.as_ref(), Some(Type::Custom(name)) if name == "__py2rust_file")
            && matches!(attr, "read" | "readline" | "readlines" | "write" | "close")
        {
            return self.gen_file_attr_call(value, attr, args, keywords);
        }

        if matches!(value.ty.as_ref(), Some(Type::List(_)))
            && matches!(
                attr,
                "append"
                    | "extend"
                    | "pop"
                    | "insert"
                    | "clear"
                    | "copy"
                    | "reverse"
                    | "index"
                    | "sort"
                    | "count"
            )
        {
            return self.gen_list_attr_call(value, attr, args, keywords);
        }

        if matches!(value.ty.as_ref(), Some(Type::Dict(_, _)))
            && matches!(attr, "get" | "pop" | "clear" | "copy" | "update")
        {
            return self.gen_dict_attr_call(value, attr, args, keywords);
        }

        if matches!(value.ty.as_ref(), Some(Type::Set(_)))
            && matches!(attr, "add" | "remove" | "discard" | "clear" | "copy")
        {
            return self.gen_set_attr_call(value, attr, args, keywords);
        }

        if let Some(Type::Iterator(inner)) = value.ty.as_ref() {
            if attr == "send" {
                self.uses.py_iter = true;
                if !keywords.is_empty() {
                    return Err(self.error(
                        value.span,
                        "Keyword arguments are not supported for this method call",
                    ));
                }
                if args.len() != 1 {
                    return Err(self.error(value.span, "iterator.send() expects one argument"));
                }
                let recv = self.gen_expr(value)?;
                let sent = self.gen_expr_with_expected(&args[0], Some(inner.as_ref()))?;
                return Ok(format!("{}.send({})", recv, sent));
            }
            if attr == "close" {
                self.uses.py_iter = true;
                if !keywords.is_empty() {
                    return Err(self.error(
                        value.span,
                        "Keyword arguments are not supported for this method call",
                    ));
                }
                if !args.is_empty() {
                    return Err(self.error(value.span, "iterator.close() expects no arguments"));
                }
                return Ok(format!("{}.close()", self.gen_expr(value)?));
            }
        }

        if attr == "format" && matches!(value.kind, ExprKind::Literal(Literal::Str(_))) {
            return self.gen_format_attr_call(value, attr, args, keywords);
        }

        if let Some(call) = self.gen_class_attr_call(value, attr, args, keywords)? {
            return Ok(call);
        }
        if let Some(call) = self.gen_union_attr_call(value, attr, args, keywords)? {
            return Ok(call);
        }

        if !keywords.is_empty() {
            return Err(self.error(
                value.span,
                "Keyword arguments are not supported for this method call",
            ));
        }
        Ok(format!(
            "{}.{}({})",
            self.gen_expr(value)?,
            attr,
            self.gen_args(args)?
        ))
    }
}
