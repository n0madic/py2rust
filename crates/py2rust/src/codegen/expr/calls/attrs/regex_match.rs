// Lightweight `re.Match` attribute call lowering.

use super::super::super::*;

impl<'a> Codegen<'a> {
    /// Lower `re.Match` methods supported by the lightweight runtime.
    pub(super) fn gen_re_match_attr_call(
        &mut self,
        value: &Expr,
        attr: &str,
        args: &[Expr],
        keywords: &[KeywordArg],
    ) -> Result<String, CompileError> {
        if !keywords.is_empty() {
            return Err(self.error(
                value.span,
                "Keyword arguments are not supported for re.Match methods",
            ));
        }
        if attr == "group" {
            if args.len() != 1 {
                return Err(self.error(value.span, "re.Match.group() expects one argument"));
            }
            self.uses.py_re_group = true;
            return Ok(format!(
                "py_re_group(&{}, {})",
                self.gen_expr(value)?,
                self.gen_expr(&args[0])?
            ));
        }
        if attr == "span" {
            if !args.is_empty() {
                return Err(self.error(value.span, "re.Match.span() expects no arguments"));
            }
            self.uses.py_re_span = true;
            return Ok(format!("py_re_span(&{})", self.gen_expr(value)?));
        }
        Err(self.error(value.span, "Unsupported re.Match method"))
    }
}
