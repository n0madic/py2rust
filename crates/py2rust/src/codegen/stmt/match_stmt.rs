// Pattern matching statement emission.

use super::super::util::collect_assign_counts;
use super::super::*;

impl<'a> Codegen<'a> {
    /// Emit a single match/case arm for a union variant.
    pub(super) fn emit_match_case(&mut self, case: &MatchCase) -> Result<(), CompileError> {
        let class_info = self.ctx.classes.get(&case.variant).ok_or_else(|| {
            self.error(
                case.span,
                format!("Unknown variant class: {}", case.variant),
            )
        })?;
        let mut bindings = Vec::new();
        for ((field, _), binding) in class_info.fields.iter().zip(case.bindings.iter()) {
            if field == binding {
                bindings.push(field.clone());
            } else {
                bindings.push(format!("{}: {}", field, binding));
            }
        }
        let union = self
            .find_union_for_variant(&case.variant)
            .ok_or_else(|| self.error(case.span, "Unable to locate union for variant"))?;
        let fields = if bindings.is_empty() {
            String::new()
        } else {
            bindings.join(", ")
        };
        self.push_line(&format!(
            "{}::{}({} {{ {} }}) => {{",
            union, case.variant, case.variant, fields
        ));
        self.indent += 1;
        let mut_counts = collect_assign_counts(&case.body);
        for stmt in &case.body {
            self.emit_stmt(stmt, &mut_counts)?;
        }
        self.indent -= 1;
        self.push_line("}");
        Ok(())
    }

    /// Find which union contains the given variant name.
    fn find_union_for_variant(&self, variant: &str) -> Option<String> {
        for (name, info) in &self.ctx.unions {
            if info.variants.contains(&variant.to_string()) {
                return Some(name.clone());
            }
        }
        None
    }
}
