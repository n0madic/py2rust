// Attribute method call lowering.

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
        if attr == "append" {
            if let Some(Type::List(_)) = value.ty.as_ref() {
                if !keywords.is_empty() {
                    return Err(self.error(value.span, "Keyword arguments are not supported"));
                }
                let target = if let ExprKind::Name(name) = &value.kind {
                    if self.is_global(name) {
                        format!(
                            "{}.lock().expect(\"list mutex poisoned\")",
                            self.global_lock_expr(name)
                        )
                    } else if self.is_local_list_name(name) {
                        return Ok(format!("{}.push({})", name, self.gen_args(args)?));
                    } else {
                        format!(
                            "{}.lock().expect(\"list mutex poisoned\")",
                            self.gen_expr(value)?
                        )
                    }
                } else {
                    format!(
                        "{}.lock().expect(\"list mutex poisoned\")",
                        self.gen_expr(value)?
                    )
                };
                return Ok(format!("{}.push({})", target, self.gen_args(args)?));
            }
        }
        if attr == "extend" {
            if let Some(Type::List(_)) = value.ty.as_ref() {
                if !keywords.is_empty() {
                    return Err(self.error(value.span, "Keyword arguments are not supported"));
                }
                let mut target = None;
                if let ExprKind::Name(name) = &value.kind {
                    if self.is_global(name) {
                        target = Some(format!(
                            "{}.lock().expect(\"list mutex poisoned\")",
                            self.global_lock_expr(name)
                        ));
                    } else if self.is_local_list_name(name) {
                        target = Some(name.clone());
                    }
                }
                let target = match target {
                    Some(expr) => expr,
                    None => format!(
                        "{}.lock().expect(\"list mutex poisoned\")",
                        self.gen_expr(value)?
                    ),
                };
                if args.is_empty() {
                    return Ok(format!("{{ {}.extend(std::iter::empty()); }}", target));
                }
                let arg = &args[0];
                // Avoid moving the source list/tuple by iterating and cloning elements.
                if matches!(arg.ty.as_ref(), Some(Type::Tuple(_))) {
                    let tuple_tmp = self.new_tmp();
                    let arg_expr = self.gen_expr(arg)?;
                    let mut elems = Vec::new();
                    if let Some(Type::Tuple(items)) = arg.ty.as_ref() {
                        for idx in 0..items.len() {
                            elems.push(format!("{}.{}.clone()", tuple_tmp, idx));
                        }
                    }
                    return Ok(format!(
                        "{{ let {} = {}; {}.extend(vec![{}]); }}",
                        tuple_tmp,
                        arg_expr,
                        target,
                        elems.join(", ")
                    ));
                }
                let arg_expr = self.gen_expr(arg)?;
                if matches!(arg.ty.as_ref(), Some(Type::List(_))) {
                    if matches!(self.list_storage_for_expr(arg), ListStorage::Local) {
                        return Ok(format!("{}.extend({}.iter().cloned())", target, arg_expr));
                    }
                    return Ok(format!(
                        "{}.extend({}.lock().expect(\"list mutex poisoned\").iter().cloned())",
                        target, arg_expr
                    ));
                }
                return Ok(format!("{}.extend({}.into_iter())", target, arg_expr));
            }
        }
        if attr == "pop" {
            if let Some(Type::List(_)) = value.ty.as_ref() {
                if !keywords.is_empty() {
                    return Err(self.error(value.span, "Keyword arguments are not supported"));
                }
                if args.len() > 1 {
                    return Err(self.error(value.span, "list.pop() expects zero or one argument"));
                }
                let idx_arg = args.first();
                if let ExprKind::Name(name) = &value.kind {
                    if !self.is_global(name) && self.is_local_list_name(name) {
                        if let Some(arg) = idx_arg {
                            let idx_raw = self.gen_expr(arg)?;
                            self.uses.py_index = true;
                            let len_tmp = self.new_tmp();
                            let idx_tmp = self.new_tmp();
                            return Ok(format!(
                                "{{ let {len_tmp} = {target}.len(); let {idx_tmp} = {idx_expr}; {target}.remove({idx_tmp}) }}",
                                len_tmp = len_tmp,
                                idx_tmp = idx_tmp,
                                idx_expr = self.wrap_result(format!(
                                    "py_index({}, {})",
                                    idx_raw, len_tmp
                                )),
                                target = name
                            ));
                        }
                        let pop_expr = format!(
                            "{}.pop().ok_or_else(|| PyError::IndexError(\"IndexError\".to_string()))",
                            name
                        );
                        return Ok(self.wrap_result(pop_expr));
                    }
                }
                if let ExprKind::Name(name) = &value.kind {
                    if self.is_global(name) {
                        let outer = self.new_tmp();
                        let guard = self.new_tmp();
                        if let Some(arg) = idx_arg {
                            let idx_raw = self.gen_expr(arg)?;
                            self.uses.py_index = true;
                            let len_tmp = self.new_tmp();
                            let idx_tmp = self.new_tmp();
                            return Ok(format!(
                                "{{ let {outer} = {lock}; let mut {guard} = {outer}.lock().expect(\"list mutex poisoned\"); let {len_tmp} = {guard}.len(); let {idx_tmp} = {idx_expr}; {guard}.remove({idx_tmp}) }}",
                                outer = outer,
                                guard = guard,
                                lock = self.global_lock_expr(name),
                                len_tmp = len_tmp,
                                idx_tmp = idx_tmp,
                                idx_expr = self.wrap_result(format!("py_index({}, {})", idx_raw, len_tmp)),
                            ));
                        }
                        let pop_expr = format!(
                            "{}.pop().ok_or_else(|| PyError::IndexError(\"IndexError\".to_string()))",
                            guard
                        );
                        return Ok(format!(
                            "{{ let {outer} = {lock}; let mut {guard} = {outer}.lock().expect(\"list mutex poisoned\"); {pop} }}",
                            outer = outer,
                            guard = guard,
                            lock = self.global_lock_expr(name),
                            pop = self.wrap_result(pop_expr),
                        ));
                    }
                }

                let target_expr = self.gen_expr(value)?;
                // For non-name targets, evaluate once into a mutable temporary.
                if !matches!(value.kind, ExprKind::Name(_)) {
                    let tmp = self.new_tmp();
                    let guard = self.new_tmp();
                    if let Some(arg) = idx_arg {
                        let idx_raw = self.gen_expr(arg)?;
                        self.uses.py_index = true;
                        let len_tmp = self.new_tmp();
                        let idx_tmp = self.new_tmp();
                        return Ok(format!(
                            "{{ let {tmp} = {target}; let mut {guard} = {tmp}.lock().expect(\"list mutex poisoned\"); let {len_tmp} = {guard}.len(); let {idx_tmp} = {idx_expr}; {guard}.remove({idx_tmp}) }}",
                            tmp = tmp,
                            guard = guard,
                            target = target_expr,
                            len_tmp = len_tmp,
                            idx_tmp = idx_tmp,
                            idx_expr = self.wrap_result(format!("py_index({}, {})", idx_raw, len_tmp)),
                        ));
                    }
                    let pop_expr = format!(
                        "{}.pop().ok_or_else(|| PyError::IndexError(\"IndexError\".to_string()))",
                        guard
                    );
                    return Ok(format!(
                        "{{ let {tmp} = {target}; let mut {guard} = {tmp}.lock().expect(\"list mutex poisoned\"); {pop} }}",
                        tmp = tmp,
                        guard = guard,
                        target = target_expr,
                        pop = self.wrap_result(pop_expr),
                    ));
                }

                // Simple local name: emit direct mutation.
                if let Some(arg) = idx_arg {
                    let idx_raw = self.gen_expr(arg)?;
                    self.uses.py_index = true;
                    let len_tmp = self.new_tmp();
                    let idx_tmp = self.new_tmp();
                    let guard = self.new_tmp();
                    return Ok(format!(
                        "{{ let mut {guard} = {target}.lock().expect(\"list mutex poisoned\"); let {len_tmp} = {guard}.len(); let {idx_tmp} = {idx_expr}; {guard}.remove({idx_tmp}) }}",
                        len_tmp = len_tmp,
                        target = target_expr,
                        idx_tmp = idx_tmp,
                        idx_expr = self.wrap_result(format!("py_index({}, {})", idx_raw, len_tmp)),
                        guard = guard,
                    ));
                }
                let pop_expr = format!(
                    "{}.pop().ok_or_else(|| PyError::IndexError(\"IndexError\".to_string()))",
                    "guard"
                );
                return Ok(self.wrap_result(format!(
                    "{{ let mut guard = {}.lock().expect(\"list mutex poisoned\"); {} }}",
                    target_expr, pop_expr
                )));
            }
        }
        if attr == "insert" {
            if let Some(Type::List(inner)) = value.ty.as_ref() {
                if args.len() != 2 {
                    return Err(self.error(value.span, "list.insert() expects two arguments"));
                }
                let idx_raw = self.gen_expr(&args[0])?;
                let val_expr = self.gen_expr_with_expected(&args[1], Some(inner.as_ref()))?;
                self.uses.py_insert_index = true;
                if let ExprKind::Name(name) = &value.kind {
                    if !self.is_global(name) && self.is_local_list_name(name) {
                        let len_tmp = self.new_tmp();
                        let idx_tmp = self.new_tmp();
                        return Ok(format!(
                            "{{ let {len_tmp} = {target}.len(); let {idx_tmp} = py_insert_index({idx_raw}, {len_tmp}); {target}.insert({idx_tmp}, {val}); }}",
                            len_tmp = len_tmp,
                            idx_tmp = idx_tmp,
                            idx_raw = idx_raw,
                            target = name,
                            val = val_expr
                        ));
                    }
                }
                if let ExprKind::Name(name) = &value.kind {
                    if self.is_global(name) {
                        let outer = self.new_tmp();
                        let guard = self.new_tmp();
                        let len_tmp = self.new_tmp();
                        let idx_tmp = self.new_tmp();
                        return Ok(format!(
                            "{{ let {outer} = {lock}; let mut {guard} = {outer}.lock().expect(\"list mutex poisoned\"); let {len_tmp} = {guard}.len(); let {idx_tmp} = py_insert_index({idx_raw}, {len_tmp}); {guard}.insert({idx_tmp}, {val}); }}",
                            outer = outer,
                            guard = guard,
                            lock = self.global_lock_expr(name),
                            len_tmp = len_tmp,
                            idx_tmp = idx_tmp,
                            idx_raw = idx_raw,
                            val = val_expr
                        ));
                    }
                }
                let target_expr = self.gen_expr(value)?;
                if !matches!(value.kind, ExprKind::Name(_)) {
                    let tmp = self.new_tmp();
                    let guard = self.new_tmp();
                    let len_tmp = self.new_tmp();
                    let idx_tmp = self.new_tmp();
                    return Ok(format!(
                        "{{ let {tmp} = {target}; let mut {guard} = {tmp}.lock().expect(\"list mutex poisoned\"); let {len_tmp} = {guard}.len(); let {idx_tmp} = py_insert_index({idx_raw}, {len_tmp}); {guard}.insert({idx_tmp}, {val}); }}",
                        tmp = tmp,
                        guard = guard,
                        target = target_expr,
                        len_tmp = len_tmp,
                        idx_tmp = idx_tmp,
                        idx_raw = idx_raw,
                        val = val_expr
                    ));
                }
                let len_tmp = self.new_tmp();
                let idx_tmp = self.new_tmp();
                return Ok(format!(
                    "{{ let mut guard = {target}.lock().expect(\"list mutex poisoned\"); let {len_tmp} = guard.len(); let {idx_tmp} = py_insert_index({idx_raw}, {len_tmp}); guard.insert({idx_tmp}, {val}); }}",
                    len_tmp = len_tmp,
                    idx_tmp = idx_tmp,
                    idx_raw = idx_raw,
                    target = target_expr,
                    val = val_expr
                ));
            }
        }
        if attr == "clear" {
            if let Some(Type::List(_)) = value.ty.as_ref() {
                if !args.is_empty() {
                    return Err(self.error(value.span, "list.clear() expects no arguments"));
                }
                if let ExprKind::Name(name) = &value.kind {
                    if !self.is_global(name) && self.is_local_list_name(name) {
                        return Ok(format!("{{ {}.clear(); }}", name));
                    }
                }
                if let ExprKind::Name(name) = &value.kind {
                    if self.is_global(name) {
                        let outer = self.new_tmp();
                        let guard = self.new_tmp();
                        return Ok(format!(
                            "{{ let {outer} = {lock}; let mut {guard} = {outer}.lock().expect(\"list mutex poisoned\"); {guard}.clear(); }}",
                            outer = outer,
                            guard = guard,
                            lock = self.global_lock_expr(name)
                        ));
                    }
                }
                let target_expr = self.gen_expr(value)?;
                if !matches!(value.kind, ExprKind::Name(_)) {
                    let tmp = self.new_tmp();
                    let guard = self.new_tmp();
                    return Ok(format!(
                        "{{ let {tmp} = {target}; let mut {guard} = {tmp}.lock().expect(\"list mutex poisoned\"); {guard}.clear(); }}",
                        tmp = tmp,
                        guard = guard,
                        target = target_expr
                    ));
                }
                return Ok(format!(
                    "{{ let mut guard = {}.lock().expect(\"list mutex poisoned\"); guard.clear(); }}",
                    target_expr
                ));
            }
        }
        if attr == "copy" {
            if let Some(Type::List(_)) = value.ty.as_ref() {
                if !args.is_empty() {
                    return Err(self.error(value.span, "list.copy() expects no arguments"));
                }
                if let ExprKind::Name(name) = &value.kind {
                    if self.is_global(name) {
                        return Ok(format!(
                            "Arc::new(Mutex::new({}.lock().expect(\"list mutex poisoned\").clone()))",
                            self.global_lock_expr(name)
                        ));
                    }
                    if self.is_local_list_name(name) {
                        return Ok(format!("Arc::new(Mutex::new({}.clone()))", name));
                    }
                }
                let target_expr = self.gen_expr(value)?;
                return Ok(format!(
                    "Arc::new(Mutex::new({}.lock().expect(\"list mutex poisoned\").clone()))",
                    target_expr
                ));
            }
        }
        if attr == "reverse" {
            if let Some(Type::List(_)) = value.ty.as_ref() {
                if !args.is_empty() {
                    return Err(self.error(value.span, "list.reverse() expects no arguments"));
                }
                if let ExprKind::Name(name) = &value.kind {
                    if !self.is_global(name) && self.is_local_list_name(name) {
                        return Ok(format!("{{ {}.reverse(); }}", name));
                    }
                }
                if let ExprKind::Name(name) = &value.kind {
                    if self.is_global(name) {
                        let outer = self.new_tmp();
                        let guard = self.new_tmp();
                        return Ok(format!(
                            "{{ let {outer} = {lock}; let mut {guard} = {outer}.lock().expect(\"list mutex poisoned\"); {guard}.reverse(); }}",
                            outer = outer,
                            guard = guard,
                            lock = self.global_lock_expr(name)
                        ));
                    }
                }
                let target_expr = self.gen_expr(value)?;
                if !matches!(value.kind, ExprKind::Name(_)) {
                    let tmp = self.new_tmp();
                    let guard = self.new_tmp();
                    return Ok(format!(
                        "{{ let {tmp} = {target}; let mut {guard} = {tmp}.lock().expect(\"list mutex poisoned\"); {guard}.reverse(); }}",
                        tmp = tmp,
                        guard = guard,
                        target = target_expr
                    ));
                }
                return Ok(format!(
                    "{{ let mut guard = {}.lock().expect(\"list mutex poisoned\"); guard.reverse(); }}",
                    target_expr
                ));
            }
        }
        if attr == "index" {
            if let Some(Type::List(_)) = value.ty.as_ref() {
                if args.len() != 1 {
                    return Err(self.error(value.span, "list.index() expects one argument"));
                }
                self.uses.py_list_index = true;
                let needle_expr = self.gen_expr(&args[0])?;
                if let ExprKind::Name(name) = &value.kind {
                    if !self.is_global(name) && self.is_local_list_name(name) {
                        let call = format!(
                            "py_list_index(&{target}, &{needle})",
                            target = name,
                            needle = needle_expr
                        );
                        return Ok(self.wrap_result(call));
                    }
                }
                if let ExprKind::Name(name) = &value.kind {
                    if self.is_global(name) {
                        let outer = self.new_tmp();
                        let guard = self.new_tmp();
                        let call = format!(
                            "py_list_index(&{guard}, &{needle})",
                            guard = guard,
                            needle = needle_expr
                        );
                        return Ok(format!(
                            "{{ let {outer} = {lock}; let {guard} = {outer}.lock().expect(\"list mutex poisoned\"); {result} }}",
                            outer = outer,
                            lock = self.global_lock_expr(name),
                            guard = guard,
                            result = self.wrap_result(call)
                        ));
                    }
                }
                let target_expr = self.gen_expr(value)?;
                let call = format!(
                    "py_list_index(&{}.lock().expect(\"list mutex poisoned\"), &{})",
                    target_expr, needle_expr
                );
                return Ok(self.wrap_result(call));
            }
        }
        if attr == "sort" {
            if let Some(Type::List(inner)) = value.ty.as_ref() {
                if !args.is_empty() {
                    return Err(self.error(value.span, "list.sort() expects no arguments"));
                }
                let sort_call = if matches!(inner.as_ref(), Type::Float) {
                    // `total_cmp` avoids panics for NaN while still providing deterministic order.
                    "sort_by(|a, b| a.total_cmp(b))"
                } else {
                    "sort()"
                };
                if let ExprKind::Name(name) = &value.kind {
                    if !self.is_global(name) && self.is_local_list_name(name) {
                        return Ok(format!("{{ {}.{}; }}", name, sort_call));
                    }
                }
                if let ExprKind::Name(name) = &value.kind {
                    if self.is_global(name) {
                        let outer = self.new_tmp();
                        let guard = self.new_tmp();
                        return Ok(format!(
                            "{{ let {outer} = {lock}; let mut {guard} = {outer}.lock().expect(\"list mutex poisoned\"); {guard}.{sort_call}; }}",
                            outer = outer,
                            guard = guard,
                            lock = self.global_lock_expr(name),
                            sort_call = sort_call
                        ));
                    }
                }
                let target_expr = self.gen_expr(value)?;
                if !matches!(value.kind, ExprKind::Name(_)) {
                    let tmp = self.new_tmp();
                    let guard = self.new_tmp();
                    return Ok(format!(
                        "{{ let {tmp} = {target}; let mut {guard} = {tmp}.lock().expect(\"list mutex poisoned\"); {guard}.{sort_call}; }}",
                        tmp = tmp,
                        guard = guard,
                        target = target_expr,
                        sort_call = sort_call
                    ));
                }
                return Ok(format!(
                    "{{ let mut guard = {}.lock().expect(\"list mutex poisoned\"); guard.{}; }}",
                    target_expr, sort_call
                ));
            }
        }
        if attr == "count" {
            if let Some(Type::List(_)) = value.ty.as_ref() {
                if args.len() != 1 {
                    return Err(self.error(value.span, "list.count() expects one argument"));
                }
                self.uses.py_list_count = true;
                let needle_expr = self.gen_expr(&args[0])?;
                if let ExprKind::Name(name) = &value.kind {
                    if !self.is_global(name) && self.is_local_list_name(name) {
                        return Ok(format!("py_list_count(&{}, &{})", name, needle_expr));
                    }
                }
                if let ExprKind::Name(name) = &value.kind {
                    if self.is_global(name) {
                        let outer = self.new_tmp();
                        let guard = self.new_tmp();
                        return Ok(format!(
                            "{{ let {outer} = {lock}; let {guard} = {outer}.lock().expect(\"list mutex poisoned\"); py_list_count(&{guard}, &{needle}) }}",
                            outer = outer,
                            lock = self.global_lock_expr(name),
                            guard = guard,
                            needle = needle_expr
                        ));
                    }
                }
                let target_expr = self.gen_expr(value)?;
                return Ok(format!(
                    "py_list_count(&{}.lock().expect(\"list mutex poisoned\"), &{})",
                    target_expr, needle_expr
                ));
            }
        }
        if attr == "get" {
            if let Some(Type::Dict(_, _)) = value.ty.as_ref() {
                if args.is_empty() || args.len() > 2 {
                    return Err(self.error(value.span, "dict.get() expects one or two arguments"));
                }
                self.uses.hash_map = true;
                let key_expr = self.gen_expr(&args[0])?;
                let default_expr = if args.len() == 2 {
                    Some(self.gen_expr(&args[1])?)
                } else {
                    None
                };
                if matches!(self.dict_storage_for_expr(value), DictStorage::Local) {
                    let target_expr = self.gen_expr(value)?;
                    if let Some(default_expr) = default_expr {
                        return Ok(format!(
                            "{target}.get(&{key}).cloned().unwrap_or({default})",
                            target = target_expr,
                            key = key_expr,
                            default = default_expr
                        ));
                    }
                    self.uses.py_dict_get = true;
                    return Ok(
                        self.wrap_result(format!("py_dict_get(&{}, &{})", target_expr, key_expr))
                    );
                }
                if let ExprKind::Name(name) = &value.kind {
                    if self.is_global(name) {
                        // Global dicts store an Arc<Mutex<...>> inside a global lock.
                        let outer = self.new_tmp();
                        let guard = self.new_tmp();
                        if let Some(default_expr) = default_expr {
                            return Ok(format!(
                                "{{ let {outer} = {lock}; let {guard} = {outer}.lock().expect(\"dict mutex poisoned\"); {guard}.get(&{key}).cloned().unwrap_or({default}) }}",
                                outer = outer,
                                guard = guard,
                                lock = self.global_lock_expr(name),
                                key = key_expr,
                                default = default_expr
                            ));
                        }
                        self.uses.py_dict_get = true;
                        return Ok(self.wrap_result(format!(
                            "{{ let {outer} = {lock}; let {guard} = {outer}.lock().expect(\"dict mutex poisoned\"); py_dict_get(&{guard}, &{key}) }}",
                            outer = outer,
                            guard = guard,
                            lock = self.global_lock_expr(name),
                            key = key_expr
                        )));
                    }
                }
                let target_expr = self.gen_expr(value)?;
                if let Some(default_expr) = default_expr {
                    let guard = self.new_tmp();
                    if !matches!(value.kind, ExprKind::Name(_)) {
                        let tmp = self.new_tmp();
                        return Ok(format!(
                            "{{ let {tmp} = {target}; let {guard} = {tmp}.lock().expect(\"dict mutex poisoned\"); {guard}.get(&{key}).cloned().unwrap_or({default}) }}",
                            tmp = tmp,
                            target = target_expr,
                            guard = guard,
                            key = key_expr,
                            default = default_expr
                        ));
                    }
                    return Ok(format!(
                        "{{ let {guard} = {target}.lock().expect(\"dict mutex poisoned\"); {guard}.get(&{key}).cloned().unwrap_or({default}) }}",
                        guard = guard,
                        target = target_expr,
                        key = key_expr,
                        default = default_expr
                    ));
                }
                self.uses.py_dict_get = true;
                let guard = self.new_tmp();
                if !matches!(value.kind, ExprKind::Name(_)) {
                    let tmp = self.new_tmp();
                    return Ok(self.wrap_result(format!(
                        "{{ let {tmp} = {target}; let {guard} = {tmp}.lock().expect(\"dict mutex poisoned\"); py_dict_get(&{guard}, &{key}) }}",
                        tmp = tmp,
                        target = target_expr,
                        guard = guard,
                        key = key_expr
                    )));
                }
                return Ok(self.wrap_result(format!(
                    "{{ let {guard} = {target}.lock().expect(\"dict mutex poisoned\"); py_dict_get(&{guard}, &{key}) }}",
                    guard = guard,
                    target = target_expr,
                    key = key_expr
                )));
            }
        }
        if attr == "pop" {
            if let Some(Type::Dict(_, _)) = value.ty.as_ref() {
                if args.is_empty() || args.len() > 2 {
                    return Err(self.error(value.span, "dict.pop() expects one or two arguments"));
                }
                self.uses.hash_map = true;
                let key_expr = self.gen_expr(&args[0])?;
                let default_expr = if args.len() == 2 {
                    Some(self.gen_expr(&args[1])?)
                } else {
                    None
                };
                if matches!(self.dict_storage_for_expr(value), DictStorage::Local) {
                    let target_expr = self.gen_expr(value)?;
                    if let Some(default_expr) = default_expr {
                        return Ok(format!(
                            "{target}.remove(&{key}).unwrap_or({default})",
                            target = target_expr,
                            key = key_expr,
                            default = default_expr
                        ));
                    }
                    let pop_expr = format!(
                        "{target}.remove(&{key}).ok_or_else(|| PyError::KeyError(\"KeyError\".to_string()))",
                        target = target_expr,
                        key = key_expr
                    );
                    return Ok(self.wrap_result(pop_expr));
                }
                if let ExprKind::Name(name) = &value.kind {
                    if self.is_global(name) {
                        // Lock the inner dict before mutating.
                        let outer = self.new_tmp();
                        let guard = self.new_tmp();
                        if let Some(default_expr) = default_expr {
                            return Ok(format!(
                                "{{ let {outer} = {lock}; let mut {guard} = {outer}.lock().expect(\"dict mutex poisoned\"); {guard}.remove(&{key}).unwrap_or({default}) }}",
                                outer = outer,
                                guard = guard,
                                lock = self.global_lock_expr(name),
                                key = key_expr,
                                default = default_expr
                            ));
                        }
                        let pop_expr = format!(
                            "{guard}.remove(&{key}).ok_or_else(|| PyError::KeyError(\"KeyError\".to_string()))",
                            guard = guard,
                            key = key_expr
                        );
                        return Ok(self.wrap_result(format!(
                            "{{ let {outer} = {lock}; let mut {guard} = {outer}.lock().expect(\"dict mutex poisoned\"); {pop} }}",
                            outer = outer,
                            guard = guard,
                            lock = self.global_lock_expr(name),
                            pop = pop_expr
                        )));
                    }
                }
                let target_expr = self.gen_expr(value)?;
                if let Some(default_expr) = default_expr {
                    let guard = self.new_tmp();
                    if !matches!(value.kind, ExprKind::Name(_)) {
                        let tmp = self.new_tmp();
                        return Ok(format!(
                            "{{ let {tmp} = {target}; let mut {guard} = {tmp}.lock().expect(\"dict mutex poisoned\"); {guard}.remove(&{key}).unwrap_or({default}) }}",
                            tmp = tmp,
                            target = target_expr,
                            guard = guard,
                            key = key_expr,
                            default = default_expr
                        ));
                    }
                    return Ok(format!(
                        "{{ let mut {guard} = {target}.lock().expect(\"dict mutex poisoned\"); {guard}.remove(&{key}).unwrap_or({default}) }}",
                        guard = guard,
                        target = target_expr,
                        key = key_expr,
                        default = default_expr
                    ));
                }
                let pop_expr = format!(
                    "{guard}.remove(&{key}).ok_or_else(|| PyError::KeyError(\"KeyError\".to_string()))",
                    guard = "{guard}",
                    key = key_expr
                );
                let guard = self.new_tmp();
                if !matches!(value.kind, ExprKind::Name(_)) {
                    let tmp = self.new_tmp();
                    return Ok(self.wrap_result(format!(
                        "{{ let {tmp} = {target}; let mut {guard} = {tmp}.lock().expect(\"dict mutex poisoned\"); {pop} }}",
                        tmp = tmp,
                        target = target_expr,
                        guard = guard,
                        pop = pop_expr.replace("{guard}", &guard)
                    )));
                }
                return Ok(self.wrap_result(format!(
                    "{{ let mut {guard} = {target}.lock().expect(\"dict mutex poisoned\"); {pop} }}",
                    guard = guard,
                    target = target_expr,
                    pop = pop_expr.replace("{guard}", &guard)
                )));
            }
        }
        if attr == "clear" {
            if let Some(Type::Dict(_, _)) = value.ty.as_ref() {
                if !args.is_empty() {
                    return Err(self.error(value.span, "dict.clear() expects no arguments"));
                }
                if matches!(self.dict_storage_for_expr(value), DictStorage::Local) {
                    let target_expr = self.gen_expr(value)?;
                    return Ok(format!("{}.clear()", target_expr));
                }
                if let ExprKind::Name(name) = &value.kind {
                    if self.is_global(name) {
                        // Clear through the inner dict lock for globals.
                        let outer = self.new_tmp();
                        let guard = self.new_tmp();
                        return Ok(format!(
                            "{{ let {outer} = {lock}; let mut {guard} = {outer}.lock().expect(\"dict mutex poisoned\"); {guard}.clear(); }}",
                            outer = outer,
                            guard = guard,
                            lock = self.global_lock_expr(name)
                        ));
                    }
                }
                let target_expr = self.gen_expr(value)?;
                let guard = self.new_tmp();
                if !matches!(value.kind, ExprKind::Name(_)) {
                    let tmp = self.new_tmp();
                    return Ok(format!(
                        "{{ let {tmp} = {target}; let mut {guard} = {tmp}.lock().expect(\"dict mutex poisoned\"); {guard}.clear(); }}",
                        tmp = tmp,
                        target = target_expr,
                        guard = guard
                    ));
                }
                return Ok(format!(
                    "{{ let mut {guard} = {target}.lock().expect(\"dict mutex poisoned\"); {guard}.clear(); }}",
                    guard = guard,
                    target = target_expr
                ));
            }
        }
        if attr == "copy" {
            if let Some(Type::Dict(_, _)) = value.ty.as_ref() {
                if !args.is_empty() {
                    return Err(self.error(value.span, "dict.copy() expects no arguments"));
                }
                if matches!(self.dict_storage_for_expr(value), DictStorage::Local) {
                    let target_expr = self.gen_expr(value)?;
                    // HashMap::clone creates a new dict object.
                    return Ok(format!("{}.clone()", target_expr));
                }
                if let ExprKind::Name(name) = &value.kind {
                    if self.is_global(name) {
                        // Copy the underlying HashMap so the result is a new dict object.
                        let outer = self.new_tmp();
                        let guard = self.new_tmp();
                        return Ok(format!(
                            "{{ let {outer} = {lock}; let {guard} = {outer}.lock().expect(\"dict mutex poisoned\"); Arc::new(Mutex::new({guard}.clone())) }}",
                            outer = outer,
                            guard = guard,
                            lock = self.global_lock_expr(name)
                        ));
                    }
                }
                let target_expr = self.gen_expr(value)?;
                let guard = self.new_tmp();
                if !matches!(value.kind, ExprKind::Name(_)) {
                    let tmp = self.new_tmp();
                    return Ok(format!(
                        "{{ let {tmp} = {target}; let {guard} = {tmp}.lock().expect(\"dict mutex poisoned\"); Arc::new(Mutex::new({guard}.clone())) }}",
                        tmp = tmp,
                        target = target_expr,
                        guard = guard
                    ));
                }
                return Ok(format!(
                    "{{ let {guard} = {target}.lock().expect(\"dict mutex poisoned\"); Arc::new(Mutex::new({guard}.clone())) }}",
                    guard = guard,
                    target = target_expr
                ));
            }
        }
        if attr == "update" {
            if let Some(Type::Dict(_, _)) = value.ty.as_ref() {
                if args.len() != 1 {
                    return Err(self.error(value.span, "dict.update() expects one argument"));
                }
                self.uses.hash_map = true;
                let arg_expr = self.gen_expr(&args[0])?;
                // Snapshot key/value pairs to avoid holding two dict borrows/locks at once.
                let pairs_tmp = self.new_tmp();
                let pairs_expr = if matches!(
                    self.dict_storage_for_expr(&args[0]),
                    DictStorage::Local
                ) {
                    format!(
                        "{arg}.iter().map(|(k, v)| (k.clone(), v.clone())).collect::<Vec<_>>()",
                        arg = arg_expr
                    )
                } else {
                    let arg_tmp = self.new_tmp();
                    let arg_guard = self.new_tmp();
                    let arg_init = if matches!(args[0].kind, ExprKind::Name(_)) {
                        format!("{}.clone()", arg_expr)
                    } else {
                        arg_expr
                    };
                    format!(
                        "{{ let {arg_tmp} = {arg_init}; let {arg_guard} = {arg_tmp}.lock().expect(\"dict mutex poisoned\"); {arg_guard}.iter().map(|(k, v)| (k.clone(), v.clone())).collect::<Vec<_>>() }}",
                        arg_tmp = arg_tmp,
                        arg_init = arg_init,
                        arg_guard = arg_guard
                    )
                };
                if matches!(self.dict_storage_for_expr(value), DictStorage::Local) {
                    let target_expr = self.gen_expr(value)?;
                    return Ok(format!(
                        "{{ let {pairs} = {pairs_expr}; {target}.extend({pairs}); }}",
                        pairs = pairs_tmp,
                        pairs_expr = pairs_expr,
                        target = target_expr
                    ));
                }
                if let ExprKind::Name(name) = &value.kind {
                    if self.is_global(name) {
                        let outer = self.new_tmp();
                        let guard = self.new_tmp();
                        return Ok(format!(
                            "{{ let {pairs} = {pairs_expr}; let {outer} = {lock}; let mut {guard} = {outer}.lock().expect(\"dict mutex poisoned\"); {guard}.extend({pairs}); }}",
                            pairs = pairs_tmp,
                            pairs_expr = pairs_expr,
                            outer = outer,
                            guard = guard,
                            lock = self.global_lock_expr(name)
                        ));
                    }
                }
                let target_expr = self.gen_expr(value)?;
                let guard = self.new_tmp();
                if !matches!(value.kind, ExprKind::Name(_)) {
                    let tmp = self.new_tmp();
                    return Ok(format!(
                        "{{ let {pairs} = {pairs_expr}; let {tmp} = {target}; let mut {guard} = {tmp}.lock().expect(\"dict mutex poisoned\"); {guard}.extend({pairs}); }}",
                        pairs = pairs_tmp,
                        pairs_expr = pairs_expr,
                        tmp = tmp,
                        target = target_expr,
                        guard = guard
                    ));
                }
                return Ok(format!(
                    "{{ let {pairs} = {pairs_expr}; let mut {guard} = {target}.lock().expect(\"dict mutex poisoned\"); {guard}.extend({pairs}); }}",
                    pairs = pairs_tmp,
                    pairs_expr = pairs_expr,
                    guard = guard,
                    target = target_expr
                ));
            }
        }
        if attr == "add" {
            if let Some(Type::Set(_)) = value.ty.as_ref() {
                self.uses.hash_set = true;
                let target = if let ExprKind::Name(name) = &value.kind {
                    if self.is_global(name) {
                        self.global_lock_expr(name)
                    } else {
                        self.gen_expr(value)?
                    }
                } else {
                    self.gen_expr(value)?
                };
                return Ok(format!("{}.insert({})", target, self.gen_args(args)?));
            }
        }
        if attr == "remove" {
            if let Some(Type::Set(_)) = value.ty.as_ref() {
                self.uses.hash_set = true;
                let target = if let ExprKind::Name(name) = &value.kind {
                    if self.is_global(name) {
                        self.global_lock_expr(name)
                    } else {
                        self.gen_expr(value)?
                    }
                } else {
                    self.gen_expr(value)?
                };
                return Ok(format!("{}.remove(&{})", target, self.gen_args(args)?));
            }
        }
        if attr == "discard" {
            if let Some(Type::Set(_)) = value.ty.as_ref() {
                self.uses.hash_set = true;
                let target = if let ExprKind::Name(name) = &value.kind {
                    if self.is_global(name) {
                        self.global_lock_expr(name)
                    } else {
                        self.gen_expr(value)?
                    }
                } else {
                    self.gen_expr(value)?
                };
                return Ok(format!(
                    "{{ {}.remove(&{}); }}",
                    target,
                    self.gen_args(args)?
                ));
            }
        }
        if attr == "clear" {
            if let Some(Type::Set(_)) = value.ty.as_ref() {
                self.uses.hash_set = true;
                if !args.is_empty() {
                    return Err(self.error(value.span, "set.clear() expects no arguments"));
                }
                if let ExprKind::Name(name) = &value.kind {
                    if self.is_global(name) {
                        let guard = self.new_tmp();
                        return Ok(format!(
                            "{{ let mut {guard} = {lock}; {guard}.clear(); }}",
                            guard = guard,
                            lock = self.global_lock_expr(name)
                        ));
                    }
                }
                let target_expr = self.gen_expr(value)?;
                if !matches!(value.kind, ExprKind::Name(_)) {
                    let tmp = self.new_tmp();
                    return Ok(format!(
                        "{{ let mut {tmp} = {target}; {tmp}.clear(); }}",
                        tmp = tmp,
                        target = target_expr
                    ));
                }
                return Ok(format!("{}.clear()", target_expr));
            }
        }
        if attr == "copy" {
            if let Some(Type::Set(_)) = value.ty.as_ref() {
                self.uses.hash_set = true;
                if !args.is_empty() {
                    return Err(self.error(value.span, "set.copy() expects no arguments"));
                }
                if let ExprKind::Name(name) = &value.kind {
                    if self.is_global(name) {
                        return Ok(format!("{}.clone()", self.global_lock_expr(name)));
                    }
                }
                let target_expr = self.gen_expr(value)?;
                return Ok(format!("{}.clone()", target_expr));
            }
        }
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
        if let Some((class_name, is_class_value)) = match &value.kind {
            ExprKind::Name(name) if self.ctx.classes.contains_key(name) => {
                Some((name.as_str(), true))
            }
            _ => value.ty.as_ref().and_then(|ty| match ty {
                Type::Custom(name) => Some((name.as_str(), false)),
                _ => None,
            }),
        } {
            if let Some(class_info) = self.ctx.classes.get(class_name) {
                if let Some(sig) = class_info.methods.get(attr) {
                    let kind = class_info
                        .method_kinds
                        .get(attr)
                        .copied()
                        .unwrap_or(MethodKind::Instance);
                    let method_def =
                        self.method_def(class_name, attr).cloned().ok_or_else(|| {
                            self.error(value.span, format!("Unknown method {class_name}.{attr}"))
                        })?;
                    let mut call = match kind {
                        MethodKind::Instance => {
                            if is_class_value {
                                return Err(
                                    self.error(value.span, "Instance methods require an instance")
                                );
                            }
                            let param_types: Vec<Type> = sig
                                .params
                                .iter()
                                .skip(1)
                                .map(|t| self.to_borrowed_param_type(t))
                                .collect();
                            let full_args = self.resolve_call_args(
                                args,
                                keywords,
                                &method_def.params[1..],
                                &param_types,
                                (Some(class_name), attr),
                                false,
                            )?;
                            let call_args = self.gen_call_args_for_sig(&param_types, &full_args)?;
                            if self.method_is_mutating(&method_def) {
                                if let ExprKind::Name(name) = &value.kind {
                                    if self.is_global(name) {
                                        let guard = self.new_tmp();
                                        return Ok(format!(
                                            "{{ let mut {guard} = {lock}; {guard}.{attr}({args}) }}",
                                            guard = guard,
                                            lock = self.global_lock_expr(name),
                                            attr = attr,
                                            args = call_args
                                        ));
                                    }
                                }
                            }
                            format!("{}.{}({})", self.gen_expr(value)?, attr, call_args)
                        }
                        MethodKind::Static => {
                            let param_types: Vec<Type> = sig
                                .params
                                .iter()
                                .map(|t| self.to_borrowed_param_type(t))
                                .collect();
                            let full_args = self.resolve_call_args(
                                args,
                                keywords,
                                &method_def.params,
                                &param_types,
                                (Some(class_name), attr),
                                false,
                            )?;
                            let call_args = self.gen_call_args_for_sig(&param_types, &full_args)?;
                            format!("{}::{}({})", class_name, attr, call_args)
                        }
                        MethodKind::Class => {
                            let param_types: Vec<Type> = sig
                                .params
                                .iter()
                                .map(|t| self.to_borrowed_param_type(t))
                                .collect();
                            let full_args = self.resolve_call_args(
                                args,
                                keywords,
                                &method_def.params,
                                &param_types,
                                (Some(class_name), attr),
                                true,
                            )?;
                            let call_args = self.gen_call_args_for_sig(&param_types, &full_args)?;
                            format!("{}::{}({})", class_name, attr, call_args)
                        }
                    };
                    if sig.can_throw {
                        call = format!("({}?)", call);
                    }
                    return Ok(call);
                }
            }
        }
        // Handle method calls on Union types by generating match expression.
        if let Some(Type::Union(union_name)) = value.ty.as_ref() {
            if let Some(union_info) = self.ctx.unions.get(union_name) {
                if !keywords.is_empty() {
                    return Err(self.error(
                        value.span,
                        "Keyword arguments are not supported for union method calls",
                    ));
                }
                // Get method signature from first variant to check if it can throw.
                let can_throw = union_info.variants.first().and_then(|v| {
                    self.ctx
                        .classes
                        .get(v)
                        .and_then(|c| c.methods.get(attr))
                        .map(|sig| sig.can_throw)
                });
                let value_expr = self.gen_expr(value)?;
                let args_str = self.gen_args(args)?;
                let mut arms = Vec::new();
                for variant in &union_info.variants {
                    arms.push(format!(
                        "{}::{}(ref _x) => _x.{}({})",
                        union_name, variant, attr, args_str
                    ));
                }
                let mut call = format!("match {} {{ {} }}", value_expr, arms.join(", "));
                if can_throw == Some(true) {
                    call = format!("({}?)", call);
                }
                return Ok(call);
            }
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
