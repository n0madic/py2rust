// Small, self-contained expression forms (literals, names, attrs, ifs).

use super::super::*;

impl<'a> Codegen<'a> {
    /// Lower a literal to its Rust expression form.
    pub(super) fn gen_literal_expr(
        &mut self,
        expr: &Expr,
        lit: &Literal,
    ) -> Result<String, CompileError> {
        match lit {
            // Always suffix numeric literals to avoid Rust type inference ambiguity.
            Literal::Int(v) => Ok(format!("{}i64", v)),
            Literal::Float(v) => Ok(format!("{}f64", v)),
            Literal::Bool(v) => Ok(format!("{}", v)),
            // Use String::from for string literals (more consistent than .to_string()).
            Literal::Str(s) => Ok(format!("String::from({s:?})")),
            // Bytes map to Vec<i64> for Python-style byte access (0-255 as ints).
            Literal::Bytes(bytes) => {
                if bytes.is_empty() {
                    return Ok("Vec::<i64>::new()".to_string());
                }
                let parts: Vec<String> = bytes.iter().map(|b| format!("{}i64", b)).collect();
                Ok(format!("vec![{}]", parts.join(", ")))
            }
            // None maps to different Rust types depending on context.
            Literal::None => {
                if let Some(Type::Option(_)) = expr.ty.as_ref() {
                    Ok("None".to_string())
                } else {
                    Ok("()".to_string())
                }
            }
        }
    }

    /// Lower a variable reference, including globals and __name__ handling.
    pub(super) fn gen_name_expr(&mut self, name: &str) -> Result<String, CompileError> {
        if name == "__name__" {
            return Ok("__NAME__.to_string()".to_string());
        }
        if self.is_global(name) {
            // Global reads go through OnceLock + Mutex with context-rich expects.
            if let Some(override_expr) = self.global_override(name) {
                return Ok(override_expr.to_string());
            }
            return Ok(format!("{}.clone()", self.global_lock_expr(name)));
        }
        Ok(name.to_string())
    }

    /// Lower attribute access, including type(x).__name__ special case.
    pub(super) fn gen_attr_expr(
        &mut self,
        value: &Expr,
        attr: &str,
    ) -> Result<String, CompileError> {
        if attr == "__name__" {
            if let ExprKind::Call { func, args } = &value.kind {
                if let ExprKind::Name(name) = &func.kind {
                    if name == "type" && args.len() == 1 {
                        if let Some(ty) = args[0].ty.as_ref() {
                            if let Some(name) = self.python_type_name(ty) {
                                return Ok(format!("String::from({:?})", name));
                            }
                        }
                        self.uses.type_name = true;
                        return Ok(format!("py_type_name(&{})", self.gen_expr(&args[0])?));
                    }
                }
            }
        }
        Ok(format!("{}.{}", self.gen_expr(value)?, attr))
    }

    /// Lower a conditional (ternary) expression.
    pub(super) fn gen_if_expr(
        &mut self,
        test: &Expr,
        body: &Expr,
        orelse: &Expr,
    ) -> Result<String, CompileError> {
        Ok(format!(
            "if {} {{ {} }} else {{ {} }}",
            self.gen_expr(test)?,
            self.gen_expr(body)?,
            self.gen_expr(orelse)?
        ))
    }

    /// Lower a union constructor expression.
    pub(super) fn gen_union_ctor_expr(
        &mut self,
        union: &str,
        variant: &str,
        inner: &Expr,
    ) -> Result<String, CompileError> {
        Ok(format!("{}::{}({})", union, variant, self.gen_expr(inner)?))
    }
}
