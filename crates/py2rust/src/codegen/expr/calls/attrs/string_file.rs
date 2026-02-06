// String/file/format attribute call lowering.

use super::super::super::*;

impl<'a> Codegen<'a> {
    /// Lower string method calls on `str` values.
    pub(super) fn gen_str_attr_call(
        &mut self,
        value: &Expr,
        attr: &str,
        args: &[Expr],
        keywords: &[KeywordArg],
    ) -> Result<String, CompileError> {
        if let Some(Type::Str) = value.ty.as_ref() {
            if attr == "upper" {
                if !keywords.is_empty() {
                    return Err(self.error(value.span, "Keyword arguments are not supported"));
                }
                if !args.is_empty() {
                    return Err(self.error(value.span, "str.upper() expects no arguments"));
                }
                return Ok(format!("{}.to_uppercase()", self.gen_expr(value)?));
            }
            if attr == "lower" {
                if !keywords.is_empty() {
                    return Err(self.error(value.span, "Keyword arguments are not supported"));
                }
                if !args.is_empty() {
                    return Err(self.error(value.span, "str.lower() expects no arguments"));
                }
                return Ok(format!("{}.to_lowercase()", self.gen_expr(value)?));
            }
            if attr == "strip" {
                if !keywords.is_empty() {
                    return Err(self.error(value.span, "Keyword arguments are not supported"));
                }
                if args.len() > 1 {
                    return Err(self.error(value.span, "str.strip() expects zero or one argument"));
                }
                if args.is_empty() {
                    return Ok(format!("{}.trim().to_string()", self.gen_expr(value)?));
                }
                let source_expr = self.gen_expr(value)?;
                let chars_expr = self.gen_expr(&args[0])?;
                let source_tmp = self.new_tmp();
                let chars_tmp = self.new_tmp();
                return Ok(format!(
                    "{{ let {source_tmp} = {source_expr}; let {chars_tmp} = {chars_expr}; {source_tmp}.trim_matches(|ch| {chars_tmp}.contains(ch)).to_string() }}",
                    source_tmp = source_tmp,
                    source_expr = source_expr,
                    chars_tmp = chars_tmp,
                    chars_expr = chars_expr
                ));
            }
            if attr == "startswith" || attr == "endswith" {
                if !keywords.is_empty() {
                    return Err(self.error(value.span, "Keyword arguments are not supported"));
                }
                if args.len() != 1 {
                    return Err(
                        self.error(value.span, format!("str.{attr}() expects one argument"))
                    );
                }
                let method = if attr == "startswith" {
                    "starts_with"
                } else {
                    "ends_with"
                };
                return Ok(format!(
                    "{}.{}(&{})",
                    self.gen_expr(value)?,
                    method,
                    self.gen_expr(&args[0])?
                ));
            }
            if attr == "find" {
                if !keywords.is_empty() {
                    return Err(self.error(value.span, "Keyword arguments are not supported"));
                }
                if args.len() != 1 {
                    return Err(self.error(value.span, "str.find() expects one argument"));
                }
                return Ok(format!(
                    "{}.find(&{}).map(|i| i as i64).unwrap_or(-1)",
                    self.gen_expr(value)?,
                    self.gen_expr(&args[0])?
                ));
            }
            if attr == "replace" {
                if !keywords.is_empty() {
                    return Err(self.error(value.span, "Keyword arguments are not supported"));
                }
                if args.len() != 2 {
                    return Err(self.error(value.span, "str.replace() expects two arguments"));
                }
                return Ok(format!(
                    "{}.replace(&{}, &{})",
                    self.gen_expr(value)?,
                    self.gen_expr(&args[0])?,
                    self.gen_expr(&args[1])?
                ));
            }
            if attr == "split" {
                if !keywords.is_empty() {
                    return Err(self.error(value.span, "Keyword arguments are not supported"));
                }
                if args.len() > 2 {
                    return Err(self.error(value.span, "str.split() expects up to two arguments"));
                }
                self.uses.py_string_methods = true;
                let source_expr = self.gen_expr(value)?;
                let split_expr = if args.is_empty() {
                    format!("py_str_split_whitespace(&{}, None)", source_expr)
                } else if args.len() == 1 {
                    format!(
                        "py_str_split_sep(&{}, &{}, None)",
                        source_expr,
                        self.gen_expr(&args[0])?
                    )
                } else {
                    format!(
                        "py_str_split_sep(&{}, &{}, Some({}))",
                        source_expr,
                        self.gen_expr(&args[0])?,
                        self.gen_expr(&args[1])?
                    )
                };
                return Ok(format!("Arc::new(Mutex::new({}))", split_expr));
            }
            if attr == "join" {
                if !keywords.is_empty() {
                    return Err(self.error(value.span, "Keyword arguments are not supported"));
                }
                if args.len() != 1 {
                    return Err(self.error(value.span, "str.join() expects one argument"));
                }
                let sep_expr = self.gen_expr(value)?;
                let iter_src = self.gen_iter_source(&args[0])?;
                let body = format!(
                    "({}).collect::<Vec<String>>().join(&{})",
                    iter_src.expr, sep_expr
                );
                return Ok(iter_src.wrap(body));
            }
            if attr == "count" {
                if !keywords.is_empty() {
                    return Err(self.error(value.span, "Keyword arguments are not supported"));
                }
                if args.len() != 1 {
                    return Err(self.error(value.span, "str.count() expects one argument"));
                }
                self.uses.py_string_methods = true;
                return Ok(format!(
                    "py_str_count(&{}, &{})",
                    self.gen_expr(value)?,
                    self.gen_expr(&args[0])?
                ));
            }
            if attr == "title" || attr == "capitalize" || attr == "swapcase" {
                if !keywords.is_empty() {
                    return Err(self.error(value.span, "Keyword arguments are not supported"));
                }
                if !args.is_empty() {
                    return Err(
                        self.error(value.span, format!("str.{attr}() expects no arguments"))
                    );
                }
                self.uses.py_string_methods = true;
                let helper = if attr == "title" {
                    "py_str_title"
                } else if attr == "capitalize" {
                    "py_str_capitalize"
                } else {
                    "py_str_swapcase"
                };
                return Ok(format!("{}(&{})", helper, self.gen_expr(value)?));
            }
            if attr == "lstrip" || attr == "rstrip" {
                if !keywords.is_empty() {
                    return Err(self.error(value.span, "Keyword arguments are not supported"));
                }
                if args.len() > 1 {
                    return Err(self.error(
                        value.span,
                        format!("str.{attr}() expects zero or one argument"),
                    ));
                }
                if args.is_empty() {
                    let method = if attr == "lstrip" {
                        "trim_start"
                    } else {
                        "trim_end"
                    };
                    return Ok(format!(
                        "{}.{}().to_string()",
                        self.gen_expr(value)?,
                        method
                    ));
                }
                self.uses.py_string_methods = true;
                let helper = if attr == "lstrip" {
                    "py_str_lstrip_chars"
                } else {
                    "py_str_rstrip_chars"
                };
                return Ok(format!(
                    "{}(&{}, &{})",
                    helper,
                    self.gen_expr(value)?,
                    self.gen_expr(&args[0])?
                ));
            }
            if attr == "center" || attr == "ljust" || attr == "rjust" {
                if !keywords.is_empty() {
                    return Err(self.error(value.span, "Keyword arguments are not supported"));
                }
                if args.is_empty() || args.len() > 2 {
                    return Err(self.error(
                        value.span,
                        format!("str.{attr}() expects one or two arguments"),
                    ));
                }
                self.uses.py_string_methods = true;
                let helper = if attr == "center" {
                    "py_str_center"
                } else if attr == "ljust" {
                    "py_str_ljust"
                } else {
                    "py_str_rjust"
                };
                let fill_expr = if args.len() == 2 {
                    format!("py_fill_char(&{})", self.gen_expr(&args[1])?)
                } else {
                    "' '".to_string()
                };
                return Ok(format!(
                    "{}(&{}, {}, {})",
                    helper,
                    self.gen_expr(value)?,
                    self.gen_expr(&args[0])?,
                    fill_expr
                ));
            }
            if attr == "zfill" {
                if !keywords.is_empty() {
                    return Err(self.error(value.span, "Keyword arguments are not supported"));
                }
                if args.len() != 1 {
                    return Err(self.error(value.span, "str.zfill() expects one argument"));
                }
                self.uses.py_string_methods = true;
                return Ok(format!(
                    "py_str_zfill(&{}, {})",
                    self.gen_expr(value)?,
                    self.gen_expr(&args[0])?
                ));
            }
            if attr == "isdigit"
                || attr == "isalpha"
                || attr == "isalnum"
                || attr == "isspace"
                || attr == "isupper"
                || attr == "islower"
            {
                if !keywords.is_empty() {
                    return Err(self.error(value.span, "Keyword arguments are not supported"));
                }
                if !args.is_empty() {
                    return Err(
                        self.error(value.span, format!("str.{attr}() expects no arguments"))
                    );
                }
                self.uses.py_string_methods = true;
                return Ok(format!("py_str_{}(&{})", attr, self.gen_expr(value)?));
            }
        }
        Err(self.error(
            value.span,
            format!("Internal error: unsupported str method `{attr}`"),
        ))
    }

    /// Lower file-like helper methods for `__py2rust_file` values.
    pub(super) fn gen_file_attr_call(
        &mut self,
        value: &Expr,
        attr: &str,
        args: &[Expr],
        keywords: &[KeywordArg],
    ) -> Result<String, CompileError> {
        if let Some(Type::Custom(class_name)) = value.ty.as_ref() {
            if class_name == "__py2rust_file" {
                if !keywords.is_empty() {
                    return Err(self.error(value.span, "Keyword arguments are not supported"));
                }
                self.uses.py_file = true;
                let file_expr = self.gen_expr(value)?;
                if attr == "read" {
                    if args.len() > 1 {
                        return Err(
                            self.error(value.span, "file.read() expects zero or one argument")
                        );
                    }
                    let read_size = if args.len() == 1 {
                        format!("Some({})", self.gen_expr(&args[0])?)
                    } else {
                        "None".to_string()
                    };
                    return Ok(self
                        .wrap_result(format!("py_file_read(&mut {}, {})", file_expr, read_size)));
                }
                if attr == "readline" {
                    if !args.is_empty() {
                        return Err(self.error(value.span, "file.readline() expects no arguments"));
                    }
                    return Ok(self.wrap_result(format!("py_file_readline(&mut {})", file_expr)));
                }
                if attr == "readlines" {
                    if !args.is_empty() {
                        return Err(self.error(value.span, "file.readlines() expects no arguments"));
                    }
                    let lines_expr =
                        self.wrap_result(format!("py_file_readlines(&mut {})", file_expr));
                    return Ok(format!("Arc::new(Mutex::new({}))", lines_expr));
                }
                if attr == "write" {
                    if args.len() != 1 {
                        return Err(self.error(value.span, "file.write() expects one argument"));
                    }
                    let data_expr = self.gen_expr(&args[0])?;
                    return Ok(self.wrap_result(format!(
                        "py_file_write(&mut {}, &{})",
                        file_expr, data_expr
                    )));
                }
                if attr == "close" {
                    if !args.is_empty() {
                        return Err(self.error(value.span, "file.close() expects no arguments"));
                    }
                    return Ok(self.wrap_result(format!("py_file_close(&mut {})", file_expr)));
                }
            }
        }
        Err(self.error(
            value.span,
            format!("Internal error: unsupported file method `{attr}`"),
        ))
    }

    /// Lower `str.format(...)` calls on string literals.
    pub(super) fn gen_format_attr_call(
        &mut self,
        value: &Expr,
        attr: &str,
        args: &[Expr],
        keywords: &[KeywordArg],
    ) -> Result<String, CompileError> {
        if attr == "format" {
            if let ExprKind::Literal(Literal::Str(fmt)) = &value.kind {
                let (rust_fmt, positional_with_spec, named_with_spec) =
                    self.rewrite_python_format_literal(fmt, value.span)?;
                let fmt_lit = format!("{rust_fmt:?}");
                if args.is_empty() && keywords.is_empty() {
                    return Ok(format!("format!({})", fmt_lit));
                }
                let mut vals = Vec::new();
                for (idx, arg) in args.iter().enumerate() {
                    let has_spec = positional_with_spec.contains(&idx);
                    vals.push(self.gen_format_arg_expr(arg, has_spec)?);
                }
                for kw in keywords {
                    let Some(name) = kw.name.as_deref() else {
                        return Err(
                            self.error(value.span, "Call-site **kwargs unpacking is not supported")
                        );
                    };
                    let has_spec = named_with_spec.get(name).copied().unwrap_or(false);
                    let rendered = self.gen_format_arg_expr(&kw.value, has_spec)?;
                    vals.push(format!("{name} = {rendered}"));
                }
                return Ok(format!("format!({}, {})", fmt_lit, vals.join(", ")));
            }
        }
        Err(self.error(
            value.span,
            format!("Internal error: unsupported format call `{attr}`"),
        ))
    }
}
