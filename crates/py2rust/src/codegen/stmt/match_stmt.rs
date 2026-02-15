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

        // Build field binding map (field_name -> binding_name)
        let mut field_bindings: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        if let Some(binding_fields) = &case.binding_fields {
            for (field, binding) in binding_fields.iter().zip(case.bindings.iter()) {
                field_bindings.insert(field.clone(), binding.clone());
            }
        } else {
            // Positional patterns use __match_args__ when present, otherwise declaration order.
            let fields_to_bind: Vec<String> = if let Some(ref match_args) = class_info.match_args {
                match_args.clone()
            } else {
                class_info.fields.keys().cloned().collect()
            };
            // Validate that all referenced fields actually exist on the class.
            for field in &fields_to_bind {
                if !class_info.fields.contains_key(field) {
                    return Err(self.error(
                        case.span,
                        format!(
                            "__match_args__ references non-existent field '{}' on class '{}'",
                            field, case.variant
                        ),
                    ));
                }
            }
            for (field, binding) in fields_to_bind.iter().zip(case.bindings.iter()) {
                field_bindings.insert(field.clone(), binding.clone());
            }
        }

        // Generate pattern for ALL fields (required by Rust), using bindings or _ for each
        let mut bindings = Vec::new();
        for (field, _) in class_info.fields.iter() {
            if let Some(binding) = field_bindings.get(field) {
                if field == binding {
                    bindings.push(field.clone());
                } else {
                    bindings.push(format!("{}: {}", field, binding));
                }
            } else {
                // Field not in __match_args__, ignore it with _
                bindings.push(format!("{}: _", field));
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
