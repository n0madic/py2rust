use crate::diagnostic::CompileError;
use crate::hir::*;
use crate::span::Span;
use crate::types::{Type, TypeRef};
use rustpython_parser::ast;
use rustpython_parser::ast::Ranged;

mod expr;
mod format;
mod function;
mod stmt;
mod types;
mod util;

pub struct Lowerer<'a> {
    source: &'a str,
    filename: &'a str,
}

impl<'a> Lowerer<'a> {
    pub fn new(source: &'a str, filename: &'a str) -> Self {
        Self { source, filename }
    }

    pub fn lower(&self, suite: &ast::Suite) -> Result<Program, CompileError> {
        let mut items = Vec::new();
        for stmt in suite {
            match stmt {
                ast::Stmt::FunctionDef(def) => {
                    if def.decorator_list.is_empty() {
                        items.push(Item::Function(self.lower_function(def)?));
                    } else {
                        let mut decorated = self.lower_decorated_function(def)?;
                        items.append(&mut decorated);
                    }
                }
                ast::Stmt::ClassDef(def) => {
                    items.push(Item::Class(self.lower_class(def)?));
                }
                ast::Stmt::Assign(def) => {
                    if let Some(union_item) = self.lower_union_alias(def)? {
                        items.push(Item::Union(union_item));
                    } else {
                        items.push(Item::Stmt(Box::new(self.lower_stmt(stmt)?)));
                    }
                }
                _ => items.push(Item::Stmt(Box::new(self.lower_stmt(stmt)?))),
            }
        }
        Ok(Program { items })
    }
}
