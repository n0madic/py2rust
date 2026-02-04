// Emission for unions, classes, and iterator adapters.

use super::super::util::collect_assign_counts;
use super::super::*;

impl<'a> Codegen<'a> {
    /// Emit a union (enum) definition.
    ///
    /// Python unions (created via `Status = Success | Failure`) map to Rust enums.
    /// Each variant wraps the corresponding class type.
    ///
    /// Example:
    /// Python: `Result = Ok | Err`
    /// Rust:   `enum Result { Ok(Ok), Err(Err) }`
    ///
    /// The variant names are the same as the class names they wrap.
    pub(crate) fn emit_union(&mut self, def: &UnionDef) -> Result<(), CompileError> {
        self.push_line("#[derive(Debug, Clone)]");
        self.push_line(&format!("pub enum {} {{", def.name));
        self.indent += 1;
        for variant in &def.variants {
            self.push_line(&format!("{}({}),", variant, variant));
        }
        self.indent -= 1;
        self.push_line("}");
        self.push_line("");
        Ok(())
    }

    /// Emit a class definition, its methods, and iterator impls when needed.
    pub(crate) fn emit_class(&mut self, class_def: &ClassDef) -> Result<(), CompileError> {
        let class_info = self.ctx.classes.get(&class_def.name).ok_or_else(|| {
            self.error(class_def.span, format!("Unknown class: {}", class_def.name))
        })?;

        self.push_line("#[derive(Debug, Clone)]");
        self.push_line(&format!("pub struct {} {{", class_def.name));
        self.indent += 1;
        for (field, ty) in &class_info.fields {
            let ty_str = self.rust_type(ty);
            self.push_line(&format!("pub {}: {},", field, ty_str));
        }
        self.indent -= 1;
        self.push_line("}");
        self.push_line("");

        self.push_line(&format!("impl {} {{", class_def.name));
        self.indent += 1;
        if let Some(init) = class_def.methods.iter().find(|m| m.name == "__init__") {
            self.emit_constructor(class_def, init)?;
        } else {
            self.push_line("// no __init__ defined");
        }

        for method in &class_def.methods {
            if method.name == "__init__" {
                continue;
            }
            if method.name == "next" && class_info.next_item.is_some() {
                continue;
            }
            self.emit_function(method, Some(class_def))?;
        }
        self.indent -= 1;
        self.push_line("}");
        self.push_line("");

        if let Some(item_ty) = &class_info.next_item {
            let item_ty = self.rust_type(item_ty);
            self.push_line(&format!("impl Iterator for {} {{", class_def.name));
            self.indent += 1;
            self.push_line(&format!("type Item = {};", item_ty));
            let next_method = class_def
                .methods
                .iter()
                .find(|m| m.name == "next")
                .ok_or_else(|| self.error(class_def.span, "Iterator class missing next method"))?;
            let ret_ty = self.resolve_type_ref(&next_method.ret, next_method.span)?;
            let ret_str = self.rust_type(&ret_ty);
            self.push_line(&format!("fn next(&mut self) -> {} {{", ret_str));
            self.indent += 1;
            let mut_counts = collect_assign_counts(&next_method.body);
            for stmt in &next_method.body {
                self.emit_stmt(stmt, &mut_counts)?;
            }
            self.indent -= 1;
            self.push_line("}");
            self.indent -= 1;
            self.push_line("}");
            self.push_line("");
        }

        if class_info.iter_return.is_some() || class_info.iter_item.is_some() {
            self.emit_into_iter(class_def, class_info)?;
        }

        Ok(())
    }

    fn emit_constructor(
        &mut self,
        class_def: &ClassDef,
        init: &Function,
    ) -> Result<(), CompileError> {
        let mut params = Vec::new();
        for param in init.params.iter().skip(1) {
            let ty = self.resolve_type_ref(&param.ann, param.span)?;
            let ty_str = self.rust_type(&ty);
            params.push(format!("{}: {}", param.name, ty_str));
        }
        let sig = format!("({}) -> {}", params.join(", "), class_def.name);
        self.push_line(&format!("pub fn new{} {{", sig));
        self.indent += 1;
        let mut field_inits: HashMap<String, String> = HashMap::new();
        for stmt in &init.body {
            match &stmt.kind {
                StmtKind::Assign { target, value } => {
                    if let AssignTarget::Attr { value: obj, attr } = target {
                        if matches!(&obj.kind, ExprKind::Name(n) if n == "self") {
                            let expr = self.gen_expr(value)?;
                            field_inits.insert(attr.clone(), expr);
                        } else {
                            return Err(
                                self.error(stmt.span, "__init__ may only assign to self fields")
                            );
                        }
                    } else {
                        return Err(
                            self.error(stmt.span, "__init__ may only assign to self fields")
                        );
                    }
                }
                StmtKind::Expr(expr) => {
                    if matches!(expr.kind, ExprKind::Literal(Literal::None)) {
                        continue;
                    }
                    return Err(
                        self.error(stmt.span, "__init__ may only contain field assignments")
                    );
                }
                _ => {
                    return Err(self.error(stmt.span, "__init__ may only contain field assignments"))
                }
            }
        }
        let class_info = self
            .ctx
            .classes
            .get(&class_def.name)
            .ok_or_else(|| self.error(class_def.span, "Unknown class"))?;
        for field in class_info.fields.keys() {
            if !field_inits.contains_key(field) {
                return Err(self.error(
                    init.span,
                    format!("Field {field} not initialized in __init__"),
                ));
            }
        }
        self.push_line(&format!("{} {{", class_def.name));
        self.indent += 1;
        for (field, _) in &class_info.fields {
            // Safe: missing fields are rejected above during __init__ validation.
            let expr = field_inits
                .get(field)
                .expect("field init missing after __init__ validation");
            self.push_line(&format!("{}: {},", field, expr));
        }
        self.indent -= 1;
        self.push_line("}");
        self.indent -= 1;
        self.push_line("}");
        Ok(())
    }

    fn emit_into_iter(
        &mut self,
        class_def: &ClassDef,
        info: &ClassInfo,
    ) -> Result<(), CompileError> {
        if let Some(iter_return) = &info.iter_return {
            let iter_info = self.ctx.classes.get(iter_return).ok_or_else(|| {
                self.error(
                    class_def.span,
                    format!("Unknown iterator class: {iter_return}"),
                )
            })?;
            let item_ty = iter_info
                .next_item
                .as_ref()
                .ok_or_else(|| self.error(class_def.span, "Iterator class missing next()"))?;
            let item_ty = self.rust_type(item_ty);
            self.push_line(&format!("impl IntoIterator for {} {{", class_def.name));
            self.indent += 1;
            self.push_line(&format!("type Item = {};", item_ty));
            self.push_line(&format!("type IntoIter = {};", iter_return));
            self.push_line("fn into_iter(self) -> Self::IntoIter {");
            self.indent += 1;
            self.push_line("self.__iter__()");
            self.indent -= 1;
            self.push_line("}");
            self.indent -= 1;
            self.push_line("}");
            self.push_line("");
        } else if let Some(item_ty) = &info.iter_item {
            let item_ty = self.rust_type(item_ty);
            self.push_line(&format!("impl IntoIterator for {} {{", class_def.name));
            self.indent += 1;
            self.push_line(&format!("type Item = {};", item_ty));
            self.push_line("type IntoIter = Box<dyn Iterator<Item = Self::Item>>;");
            self.push_line("fn into_iter(self) -> Self::IntoIter {");
            self.indent += 1;
            self.push_line("Box::new(self.__iter__())");
            self.indent -= 1;
            self.push_line("}");
            self.indent -= 1;
            self.push_line("}");
            self.push_line("");
        }
        Ok(())
    }
}
