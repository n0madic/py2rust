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

/// The Lowerer transforms RustPython's AST into our HIR.
///
/// Why we need a separate HIR instead of using RustPython's AST directly:
/// 1. RustPython AST is comprehensive (supports all Python features), we only need a subset
/// 2. The AST contains many Python-specific details we don't need for Rust codegen
/// 3. Having our own HIR lets us normalize constructs and add Rust-specific information
/// 4. It provides stable anchor points for type information (ty fields in Expr)
/// 5. Error reporting is simpler when working with a smaller, focused representation
///
/// The lowering process validates that the Python code uses only supported features
/// and rejects unsupported constructs with clear error messages.
pub struct Lowerer<'a> {
    /// Source code (needed for error reporting)
    source: &'a str,
    /// Filename (needed for error reporting)
    filename: &'a str,
}

impl<'a> Lowerer<'a> {
    pub fn new(source: &'a str, filename: &'a str) -> Self {
        Self { source, filename }
    }

    /// Lower a Python module (suite of statements) to HIR Program.
    ///
    /// Top-level items are categorized into:
    /// - Functions (def statements)
    /// - Classes (class statements)
    /// - Unions (type alias assignments like `Result = Ok | Err`)
    /// - Statements (everything else - will go into generated `fn main()`)
    ///
    /// Special handling:
    /// - Decorated functions are expanded into multiple items (impl function + wrapper)
    /// - Type aliases using `|` operator are detected as union definitions
    /// - Regular assignments become top-level statements
    pub fn lower(&self, suite: &ast::Suite) -> Result<Program, CompileError> {
        let mut items = Vec::new();
        let mut known_classes = std::collections::HashSet::new();
        for stmt in suite {
            match stmt {
                ast::Stmt::FunctionDef(def) => {
                    if def.decorator_list.is_empty() {
                        items.push(Item::Function(self.lower_function(def)?));
                    } else {
                        // Decorators are expanded: `@dec def f(): ...` becomes
                        // two functions: f_impl() and f() which calls dec(f_impl)()
                        let mut decorated = self.lower_decorated_function(def)?;
                        items.append(&mut decorated);
                    }
                }
                ast::Stmt::ClassDef(def) => {
                    items.push(Item::Class(self.lower_class(def)?));
                    known_classes.insert(self.ident(def.name.as_str()));
                }
                ast::Stmt::Assign(def) => {
                    // Check if this is a union type alias (e.g., Status = Success | Failure)
                    if let Some(union_item) = self.lower_union_alias(def, &known_classes)? {
                        items.push(Item::Union(union_item));
                    } else {
                        // Regular assignment - becomes a top-level statement
                        items.push(Item::Stmt(Box::new(self.lower_stmt(stmt)?)));
                    }
                }
                _ => items.push(Item::Stmt(Box::new(self.lower_stmt(stmt)?))),
            }
        }
        Ok(Program { items })
    }
}
