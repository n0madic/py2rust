// Lightweight urllib runtime object method-call lowering.

use super::super::super::*;

impl<'a> Codegen<'a> {
    /// Lower `urllib.parse.ParseResult` helper methods.
    pub(super) fn gen_urllib_parse_result_attr_call(
        &mut self,
        value: &Expr,
        attr: &str,
        args: &[Expr],
        keywords: &[KeywordArg],
    ) -> Result<String, CompileError> {
        if !keywords.is_empty() {
            return Err(self.error(
                value.span,
                "Keyword arguments are not supported for urllib.parse.ParseResult methods",
            ));
        }
        if attr == "geturl" {
            if !args.is_empty() {
                return Err(self.error(
                    value.span,
                    "urllib.parse.ParseResult.geturl() expects no arguments",
                ));
            }
            self.uses.py_urllib_parse_geturl = true;
            return Ok(format!(
                "py_urllib_parse_geturl(&{})",
                self.gen_expr(value)?
            ));
        }
        Err(self.error(value.span, "Unsupported urllib.parse.ParseResult method"))
    }

    /// Lower lightweight `urllib.request` response helper methods.
    pub(super) fn gen_urllib_response_attr_call(
        &mut self,
        value: &Expr,
        attr: &str,
        args: &[Expr],
        keywords: &[KeywordArg],
    ) -> Result<String, CompileError> {
        if !keywords.is_empty() {
            return Err(self.error(
                value.span,
                "Keyword arguments are not supported for urllib.request response methods",
            ));
        }
        if attr == "read" {
            if !args.is_empty() {
                return Err(self.error(
                    value.span,
                    "urllib.request response read() expects no arguments",
                ));
            }
            self.uses.py_urllib_response_read = true;
            return Ok(format!(
                "py_urllib_response_read(&{})",
                self.gen_expr(value)?
            ));
        }
        if attr == "getcode" {
            if !args.is_empty() {
                return Err(self.error(
                    value.span,
                    "urllib.request response getcode() expects no arguments",
                ));
            }
            self.uses.py_urllib_response_getcode = true;
            return Ok(format!(
                "py_urllib_response_getcode(&{})",
                self.gen_expr(value)?
            ));
        }
        if attr == "geturl" {
            if !args.is_empty() {
                return Err(self.error(
                    value.span,
                    "urllib.request response geturl() expects no arguments",
                ));
            }
            self.uses.py_urllib_response_geturl = true;
            return Ok(format!(
                "py_urllib_response_geturl(&{})",
                self.gen_expr(value)?
            ));
        }
        Err(self.error(value.span, "Unsupported urllib.request response method"))
    }
}
