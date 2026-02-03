use crate::diagnostic::{CompileError, Warning};
use crate::hir::*;
use crate::span::Span;
use crate::types::{Type, TypeRef};
use indexmap::IndexMap;
use std::collections::{HashMap, HashSet};

mod call;
mod check_class;
mod check_function;
mod context;
mod diag;
mod expr;
mod resolve;
mod scope;
mod signatures;
mod stmt;
mod type_ops;

pub use context::{ClassInfo, FunctionSig, TypeContext, UnionInfo};

#[derive(Debug, Default, Clone)]
struct GlobalScope {
    declared: HashSet<String>,
    used_before_decl: HashMap<String, Span>,
}

pub struct TypeChecker<'a> {
    source: &'a str,
    filename: &'a str,
    ctx: TypeContext,
    scopes: Vec<HashMap<String, Type>>,
    global_scopes: Vec<GlobalScope>,
    warnings: Vec<Warning>,
}

impl<'a> TypeChecker<'a> {
    pub fn new(
        program: &Program,
        source: &'a str,
        filename: &'a str,
    ) -> Result<Self, CompileError> {
        let mut classes = HashMap::new();
        let mut unions = HashMap::new();
        let functions = HashMap::new();
        let globals = HashMap::new();

        for item in &program.items {
            if let Item::Union(def) = item {
                unions.insert(
                    def.name.clone(),
                    UnionInfo {
                        name: def.name.clone(),
                        variants: def.variants.clone(),
                    },
                );
            }
        }

        for item in &program.items {
            if let Item::Class(class_def) = item {
                classes.insert(
                    class_def.name.clone(),
                    ClassInfo {
                        name: class_def.name.clone(),
                        fields: IndexMap::new(),
                        methods: HashMap::new(),
                        init: None,
                        iter_return: None,
                        iter_item: None,
                        next_item: None,
                    },
                );
            }
        }

        let mut checker = Self {
            source,
            filename,
            ctx: TypeContext {
                classes,
                unions,
                functions,
                globals,
            },
            scopes: Vec::new(),
            global_scopes: Vec::new(),
            warnings: Vec::new(),
        };

        checker.collect_signatures(program)?;

        Ok(checker)
    }

    pub fn check_program(&mut self, program: &mut Program) -> Result<TypeContext, CompileError> {
        self.scopes.push(HashMap::new());
        self.insert_var("__name__", Type::Str, Span::new(0, 0))?;
        for item in &mut program.items {
            if let Item::Stmt(stmt) = item {
                self.check_stmt(stmt.as_mut(), None)?;
            }
        }
        if let Some(scope) = self.scopes.last() {
            self.ctx.globals = scope
                .iter()
                .filter(|(name, _)| name.as_str() != "__name__")
                .map(|(name, ty)| (name.clone(), ty.clone()))
                .collect();
        }
        for item in &mut program.items {
            match item {
                Item::Function(func) => self.check_function(func, None)?,
                Item::Class(class) => self.check_class(class)?,
                Item::Stmt(_) => {}
                Item::Union(_) => {}
            }
        }
        self.scopes.pop();
        Ok(self.ctx.clone())
    }
}
