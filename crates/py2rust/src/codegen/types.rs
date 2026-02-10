use super::*;

/// Type mapping from Python types to Rust types.
///
/// This module handles the core type translation that makes transpilation work.
///
/// Key mapping decisions:
/// - int -> i64 (not i32, to handle large numbers)
/// - float -> f64 (standard floating point)
/// - str -> String (owned, not &str, to avoid lifetime complexity)
/// - list -> Rc<RefCell<Vec<T>>> (non-global shared path)
/// - dict -> Rc<RefCell<IndexMap<K, V>>> (non-global shared path)
/// - set -> HashSet<T>
/// - None -> () (unit type)
/// - Optional[T] -> Option<T>
///
/// Special cases:
/// - Lambdas: `impl Fn(...) -> ... + 'static` at top level, boxed in nested callable positions
/// - Iterators: `impl Iterator<Item = T>` (no boxing)
/// - Globals: Need thread-safe wrappers (Arc, Mutex)
/// - References: &str instead of &String for ergonomics
impl<'a> Codegen<'a> {
    /// Convert a Python Type to its Rust representation.
    ///
    /// This is used for local variable declarations, function parameters,
    /// and return types. The generated types are optimized for local use
    /// (e.g., using `impl Trait` for lambdas and iterators).
    pub(crate) fn rust_type(&mut self, ty: &Type) -> String {
        self.rust_type_with_lambda_depth(ty, 0)
    }

    /// Render a type for closure parameters where `impl Trait` is not allowed.
    pub(crate) fn rust_type_for_closure_param(&mut self, ty: &Type) -> String {
        match ty {
            Type::Lambda { .. } | Type::Iterator(_) => self.rust_type_with_lambda_depth(ty, 1),
            _ => self.rust_type(ty),
        }
    }

    /// Render a type while tracking callable nesting depth.
    ///
    /// Once we're inside a callable signature, nested callables must be rendered
    /// as trait objects because Rust forbids nested `impl Trait` in `Fn` bounds.
    fn rust_type_with_lambda_depth(&mut self, ty: &Type, lambda_depth: usize) -> String {
        match ty {
            Type::Int => "i64".to_string(),
            Type::Float => "f64".to_string(),
            Type::Bool => "bool".to_string(),
            Type::Str => "String".to_string(),
            Type::Bytes => "Vec<i64>".to_string(),
            Type::None => "()".to_string(),
            Type::List(inner) => {
                if lambda_depth > 0 && matches!(inner.as_ref(), Type::Unknown) {
                    self.uses.py_repr = true;
                    "Rc<RefCell<Vec<PyRepr>>>".to_string()
                } else {
                    format!(
                        "Rc<RefCell<Vec<{}>>>",
                        self.rust_type_with_lambda_depth(inner, lambda_depth)
                    )
                }
            }
            Type::Dict(k, v) => {
                self.uses.index_map = true;
                let key_ty = if lambda_depth > 0 && matches!(k.as_ref(), Type::Unknown) {
                    self.uses.py_repr = true;
                    "PyRepr".to_string()
                } else {
                    self.rust_type_with_lambda_depth(k, lambda_depth)
                };
                let val_ty = if lambda_depth > 0 && matches!(v.as_ref(), Type::Unknown) {
                    self.uses.py_repr = true;
                    "PyRepr".to_string()
                } else {
                    self.rust_type_with_lambda_depth(v, lambda_depth)
                };
                format!("Rc<RefCell<IndexMap<{}, {}>>>", key_ty, val_ty)
            }
            Type::Tuple(items) => {
                let parts: Vec<String> = items
                    .iter()
                    .map(|t| self.rust_type_with_lambda_depth(t, lambda_depth))
                    .collect();
                if items.len() == 1 {
                    format!("({},)", parts[0])
                } else {
                    format!("({})", parts.join(", "))
                }
            }
            Type::Set(inner) => {
                self.uses.hash_set = true;
                if lambda_depth > 0 && matches!(inner.as_ref(), Type::Unknown) {
                    self.uses.py_repr = true;
                    "HashSet<PyRepr>".to_string()
                } else {
                    format!(
                        "HashSet<{}>",
                        self.rust_type_with_lambda_depth(inner, lambda_depth)
                    )
                }
            }
            Type::Option(inner) => format!(
                "Option<{}>",
                self.rust_type_with_lambda_depth(inner, lambda_depth)
            ),
            // Import bindings are compile-time only and are never emitted as values.
            Type::Module(_) | Type::StdlibFunction { .. } => "()".to_string(),
            Type::Custom(name) => {
                if name == "__py_file" {
                    "std::fs::File".to_string()
                } else {
                    name.clone()
                }
            }
            Type::Union(name) => name.clone(),
            Type::Iterator(inner) => {
                if lambda_depth == 0 {
                    format!(
                        "impl Iterator<Item = {}>",
                        self.rust_type_with_lambda_depth(inner, lambda_depth)
                    )
                } else {
                    self.uses.py_iter = true;
                    format!(
                        "PyIter<{}>",
                        self.rust_type_with_lambda_depth(inner, lambda_depth)
                    )
                }
            }
            Type::Lambda { params, ret, .. } => {
                let args: Vec<String> = params
                    .iter()
                    .map(|t| {
                        if matches!(t, Type::Unknown) {
                            "()".to_string()
                        } else {
                            self.rust_type_with_lambda_depth(t, lambda_depth + 1)
                        }
                    })
                    .collect();
                let ret_ty = if matches!(ret.as_ref(), Type::Unknown) {
                    "()".to_string()
                } else {
                    self.rust_type_with_lambda_depth(ret, lambda_depth + 1)
                };
                if lambda_depth == 0 {
                    format!("impl Fn({}) -> {} + 'static", args.join(", "), ret_ty)
                } else {
                    format!("Box<dyn Fn({}) -> {} + 'static>", args.join(", "), ret_ty)
                }
            }
            Type::Ref(inner) => {
                if matches!(inner.as_ref(), Type::Str) {
                    "&str".to_string()
                } else {
                    format!("&{}", self.rust_type_with_lambda_depth(inner, lambda_depth))
                }
            }
            Type::MutRef(inner) => format!(
                "&mut {}",
                self.rust_type_with_lambda_depth(inner, lambda_depth)
            ),
            Type::Slice(inner) => format!(
                "&[{}]",
                self.rust_type_with_lambda_depth(inner, lambda_depth)
            ),
            Type::Result(ok, err) => format!(
                "Result<{}, {}>",
                self.rust_type_with_lambda_depth(ok, lambda_depth),
                self.rust_type_with_lambda_depth(err, lambda_depth)
            ),
            Type::Exception(name) => name.clone(),
            Type::Unknown => "_".to_string(),
        }
    }

    /// Convert a Python Type to a Rust type, using the requested list storage.
    pub(crate) fn rust_type_for_list_storage(&mut self, ty: &Type, storage: ListStorage) -> String {
        match (ty, storage) {
            (Type::List(inner), ListStorage::Local) => {
                format!("Vec<{}>", self.rust_type(inner))
            }
            (Type::List(inner), ListStorage::SharedCell) => {
                format!("Rc<RefCell<Vec<{}>>>", self.rust_type(inner))
            }
            (Type::List(inner), ListStorage::SharedSync) => {
                format!("Arc<Mutex<Vec<{}>>>", self.rust_type(inner))
            }
            _ => self.rust_type(ty),
        }
    }

    /// Convert a Python Type to a Rust type, using the requested dict storage.
    pub(crate) fn rust_type_for_dict_storage(&mut self, ty: &Type, storage: DictStorage) -> String {
        match (ty, storage) {
            (Type::Dict(k, v), DictStorage::Local) => {
                self.uses.index_map = true;
                format!("IndexMap<{}, {}>", self.rust_type(k), self.rust_type(v))
            }
            (Type::Dict(k, v), DictStorage::SharedCell) => {
                self.uses.index_map = true;
                format!(
                    "Rc<RefCell<IndexMap<{}, {}>>>",
                    self.rust_type(k),
                    self.rust_type(v)
                )
            }
            (Type::Dict(k, v), DictStorage::SharedSync) => {
                self.uses.index_map = true;
                format!(
                    "Arc<Mutex<IndexMap<{}, {}>>>",
                    self.rust_type(k),
                    self.rust_type(v)
                )
            }
            _ => self.rust_type(ty),
        }
    }

    /// Convert a Python Type to its Rust representation for global variables.
    ///
    /// Global variables have special requirements in Rust:
    /// 1. Must be thread-safe (Send + Sync)
    /// 2. Lambdas and iterators can't use `impl Trait` (can't be stored in globals)
    /// 3. Need concrete types for the OnceLock wrapper
    ///
    /// Differences from rust_type():
    /// - Iterators: PyIter<T> (clonable wrapper) instead of impl Iterator
    /// - Lambdas: Arc<dyn Fn...> (boxed trait object) instead of impl Fn
    /// - Everything must be Send + Sync for global storage
    pub(crate) fn rust_type_for_global(&mut self, ty: &Type) -> String {
        match ty {
            Type::Iterator(inner) => {
                // PyIter wraps an iterator in Arc<Mutex<Box<dyn Iterator>>>
                // This makes it clonable and thread-safe for global storage
                self.uses.py_iter = true;
                format!("PyIter<{}>", self.rust_type_for_global(inner))
            }
            Type::Lambda { params, ret, .. } => {
                let args: Vec<String> = params
                    .iter()
                    .map(|t| {
                        if matches!(t, Type::Unknown) {
                            "()".to_string()
                        } else {
                            self.rust_type_for_global(t)
                        }
                    })
                    .collect();
                let ret_ty = if matches!(ret.as_ref(), Type::Unknown) {
                    "()".to_string()
                } else {
                    self.rust_type_for_global(ret)
                };
                format!(
                    "Arc<dyn Fn({}) -> {} + Send + Sync + 'static>",
                    args.join(", "),
                    ret_ty
                )
            }
            // Use a PyRepr-backed list for unknown element types to keep globals concrete.
            Type::List(inner) if matches!(inner.as_ref(), Type::Unknown) => {
                self.uses.py_repr = true;
                "Arc<Mutex<Vec<PyRepr>>>".to_string()
            }
            Type::List(inner) => format!("Arc<Mutex<Vec<{}>>>", self.rust_type_for_global(inner)),
            Type::Bytes => "Vec<i64>".to_string(),
            Type::Set(inner) => {
                self.uses.hash_set = true;
                format!("HashSet<{}>", self.rust_type_for_global(inner))
            }
            Type::Dict(k, v) => {
                // Match local dict semantics (shared Arc) even in globals.
                self.uses.index_map = true;
                format!(
                    "Arc<Mutex<IndexMap<{}, {}>>>",
                    self.rust_type_for_global(k),
                    self.rust_type_for_global(v)
                )
            }
            Type::Tuple(items) => {
                let parts: Vec<String> =
                    items.iter().map(|t| self.rust_type_for_global(t)).collect();
                if items.len() == 1 {
                    format!("({},)", parts[0])
                } else {
                    format!("({})", parts.join(", "))
                }
            }
            Type::Option(inner) => format!("Option<{}>", self.rust_type_for_global(inner)),
            Type::Custom(name) if name == "__py_file" => "std::fs::File".to_string(),
            Type::Ref(inner) => {
                if matches!(inner.as_ref(), Type::Str) {
                    "&str".to_string()
                } else {
                    format!("&{}", self.rust_type_for_global(inner))
                }
            }
            Type::MutRef(inner) => format!("&mut {}", self.rust_type_for_global(inner)),
            Type::Slice(inner) => format!("&[{}]", self.rust_type_for_global(inner)),
            Type::Result(ok, err) => format!(
                "Result<{}, {}>",
                self.rust_type_for_global(ok),
                self.rust_type_for_global(err)
            ),
            Type::Unknown => "()".to_string(),
            Type::Module(_) | Type::StdlibFunction { .. } => "()".to_string(),
            _ => self.rust_type(ty),
        }
    }

    pub(crate) fn resolve_type_ref(&self, ty: &TypeRef, span: Span) -> Result<Type, CompileError> {
        match ty {
            TypeRef::Name(name) => Ok(match name.as_str() {
                "int" => Type::Int,
                "float" => Type::Float,
                "bool" => Type::Bool,
                "str" => Type::Str,
                "bytes" => Type::Bytes,
                _ => {
                    if self.ctx.unions.contains_key(name) {
                        Type::Union(name.clone())
                    } else if self.ctx.classes.contains_key(name) {
                        Type::Custom(name.clone())
                    } else {
                        return Err(self.error(span, format!("Unknown type: {name}")));
                    }
                }
            }),
            TypeRef::None => Ok(Type::None),
            TypeRef::List(inner) => Ok(Type::List(Box::new(self.resolve_type_ref(inner, span)?))),
            TypeRef::Dict(k, v) => Ok(Type::Dict(
                Box::new(self.resolve_type_ref(k, span)?),
                Box::new(self.resolve_type_ref(v, span)?),
            )),
            TypeRef::Tuple(items) => {
                let mut out = Vec::new();
                for item in items {
                    out.push(self.resolve_type_ref(item, span)?);
                }
                Ok(Type::Tuple(out))
            }
            TypeRef::Set(inner) => Ok(Type::Set(Box::new(self.resolve_type_ref(inner, span)?))),
            TypeRef::Optional(inner) => {
                Ok(Type::Option(Box::new(self.resolve_type_ref(inner, span)?)))
            }
            TypeRef::Iterator(inner) => Ok(Type::Iterator(Box::new(
                self.resolve_type_ref(inner, span)?,
            ))),
            TypeRef::Lambda { params, ret } => {
                let mut out = Vec::new();
                for param in params {
                    out.push(self.resolve_type_ref(param, span)?);
                }
                let ret_ty = self.resolve_type_ref(ret, span)?;
                Ok(Type::Lambda {
                    param_names: Vec::new(),
                    params: out,
                    param_kinds: Vec::new(),
                    has_defaults: Vec::new(),
                    ret: Box::new(ret_ty),
                })
            }
            TypeRef::Result(ok, err) => Ok(Type::Result(
                Box::new(self.resolve_type_ref(ok, span)?),
                Box::new(self.resolve_type_ref(err, span)?),
            )),
            TypeRef::Exception(name) => Ok(Type::Exception(name.clone())),
            TypeRef::Unknown => Ok(Type::Unknown),
            TypeRef::Union(parts) => {
                // Mirror typecheck-side behavior so codegen can continue even when
                // annotations use wide inline unions.
                let mut has_none = false;
                let mut other = Vec::new();
                for part in parts {
                    let t = self.resolve_type_ref(part, span)?;
                    if matches!(t, Type::None) {
                        has_none = true;
                    } else {
                        other.push(t);
                    }
                }
                if has_none && other.len() == 1 {
                    Ok(Type::Option(Box::new(other.remove(0))))
                } else if !has_none && other.len() == 1 {
                    Ok(other.remove(0))
                } else {
                    Ok(Type::Unknown)
                }
            }
        }
    }
}
