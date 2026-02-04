// Helper function emission for generated Rust files.

use super::super::*;

impl<'a> Codegen<'a> {
    /// Emit all helper functions needed by the generated code.
    ///
    /// Helpers are only emitted if their corresponding `uses.*` flag is set.
    /// This is determined by scanning the HIR before code generation.
    ///
    /// Why inject helpers instead of using a runtime crate?
    /// 1. Self-contained output: one .rs file can be compiled standalone
    /// 2. No version dependencies: no need to manage crate versions
    /// 3. Easier distribution: users can just copy the .rs file
    /// 4. Transparency: users can see exactly what code is running
    ///
    /// Drawbacks:
    /// - Larger generated files if many helpers are used
    /// - Code duplication if transpiling multiple Python files
    ///
    /// We accept these tradeoffs because py2rust targets single-file transpilation.
    pub(crate) fn emit_helpers(&mut self) {
        // PyError enum is needed for exception handling.
        if self.needs_py_error() {
            self.emit_py_error_enum();
        }

        // PyIter wrapper makes iterators clonable (Python's for loops clone iterators).
        if self.uses.py_iter {
            self.push_line("#[derive(Clone)]");
            self.push_line("struct PyIter<T> {");
            self.indent += 1;
            self.push_line("inner: Arc<Mutex<Box<dyn Iterator<Item = T> + Send>>>,");
            self.indent -= 1;
            self.push_line("}");
            self.push_line("impl<T> PyIter<T> {");
            self.indent += 1;
            self.push_line("fn new<I: Iterator<Item = T> + Send + 'static>(iter: I) -> Self {");
            self.indent += 1;
            self.push_line("Self { inner: Arc::new(Mutex::new(Box::new(iter))) }");
            self.indent -= 1;
            self.push_line("}");
            self.indent -= 1;
            self.push_line("}");
            self.push_line("impl<T> Iterator for PyIter<T> {");
            self.indent += 1;
            self.push_line("type Item = T;");
            self.push_line("fn next(&mut self) -> Option<Self::Item> {");
            self.indent += 1;
            // Poisoned mutex means iterator state is invalid; panic with context.
            self.push_line("self.inner.lock().expect(\"PyIter mutex poisoned\").next()");
            self.indent -= 1;
            self.push_line("}");
            self.indent -= 1;
            self.push_line("}");
            self.push_line("fn py_iter<T, I>(iter: I) -> PyIter<T>");
            self.indent += 1;
            self.push_line("where");
            self.indent += 1;
            self.push_line("I: Iterator<Item = T> + Send + 'static,");
            self.indent -= 1;
            self.push_line("{");
            self.indent += 1;
            self.push_line("PyIter::new(iter)");
            self.indent -= 1;
            self.push_line("}");
        }

        if self.uses.print {
            self.push_line("fn py_print<T: std::fmt::Display>(v: T) {");
            self.indent += 1;
            self.push_line("println!(\"{v}\");");
            self.indent -= 1;
            self.push_line("}");
        }
        if self.uses.py_repr {
            // PyRepr wraps preformatted strings so list Debug output matches Python repr style.
            self.push_line("#[derive(Clone, PartialEq, PartialOrd)]");
            self.push_line("struct PyRepr(String);");
            self.push_line("impl std::fmt::Debug for PyRepr {");
            self.indent += 1;
            self.push_line("fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {");
            self.indent += 1;
            self.push_line("write!(f, \"{}\", self.0)");
            self.indent -= 1;
            self.push_line("}");
            self.indent -= 1;
            self.push_line("}");
        }
        if self.uses.len {
            self.push_line("trait PyLen {");
            self.indent += 1;
            self.push_line("fn py_len(&self) -> i64;");
            self.indent -= 1;
            self.push_line("}");
            // Tuples have fixed arity, so provide PyLen impls for common tuple sizes.
            self.push_line("impl<T> PyLen for Vec<T> {");
            self.indent += 1;
            self.push_line("fn py_len(&self) -> i64 { self.len() as i64 }");
            self.indent -= 1;
            self.push_line("}");
            self.push_line("impl PyLen for String {");
            self.indent += 1;
            self.push_line("fn py_len(&self) -> i64 { self.chars().count() as i64 }");
            self.indent -= 1;
            self.push_line("}");
            self.push_line("impl PyLen for &str {");
            self.indent += 1;
            self.push_line("fn py_len(&self) -> i64 { self.chars().count() as i64 }");
            self.indent -= 1;
            self.push_line("}");
            self.push_line("impl<K, V> PyLen for std::collections::HashMap<K, V> {");
            self.indent += 1;
            self.push_line("fn py_len(&self) -> i64 { self.len() as i64 }");
            self.indent -= 1;
            self.push_line("}");
            self.push_line("impl<T> PyLen for std::collections::HashSet<T> {");
            self.indent += 1;
            self.push_line("fn py_len(&self) -> i64 { self.len() as i64 }");
            self.indent -= 1;
            self.push_line("}");
            self.push_line("impl PyLen for () {");
            self.indent += 1;
            self.push_line("fn py_len(&self) -> i64 { 0 }");
            self.indent -= 1;
            self.push_line("}");
            self.push_line("impl<T1> PyLen for (T1,) {");
            self.indent += 1;
            self.push_line("fn py_len(&self) -> i64 { 1 }");
            self.indent -= 1;
            self.push_line("}");
            self.push_line("impl<T1, T2> PyLen for (T1, T2) {");
            self.indent += 1;
            self.push_line("fn py_len(&self) -> i64 { 2 }");
            self.indent -= 1;
            self.push_line("}");
            self.push_line("impl<T1, T2, T3> PyLen for (T1, T2, T3) {");
            self.indent += 1;
            self.push_line("fn py_len(&self) -> i64 { 3 }");
            self.indent -= 1;
            self.push_line("}");
            self.push_line("impl<T1, T2, T3, T4> PyLen for (T1, T2, T3, T4) {");
            self.indent += 1;
            self.push_line("fn py_len(&self) -> i64 { 4 }");
            self.indent -= 1;
            self.push_line("}");
            self.push_line("impl<T1, T2, T3, T4, T5> PyLen for (T1, T2, T3, T4, T5) {");
            self.indent += 1;
            self.push_line("fn py_len(&self) -> i64 { 5 }");
            self.indent -= 1;
            self.push_line("}");
            self.push_line("impl<T1, T2, T3, T4, T5, T6> PyLen for (T1, T2, T3, T4, T5, T6) {");
            self.indent += 1;
            self.push_line("fn py_len(&self) -> i64 { 6 }");
            self.indent -= 1;
            self.push_line("}");
            self.push_line(
                "impl<T1, T2, T3, T4, T5, T6, T7> PyLen for (T1, T2, T3, T4, T5, T6, T7) {",
            );
            self.indent += 1;
            self.push_line("fn py_len(&self) -> i64 { 7 }");
            self.indent -= 1;
            self.push_line("}");
            self.push_line(
                "impl<T1, T2, T3, T4, T5, T6, T7, T8> PyLen for (T1, T2, T3, T4, T5, T6, T7, T8) {",
            );
            self.indent += 1;
            self.push_line("fn py_len(&self) -> i64 { 8 }");
            self.indent -= 1;
            self.push_line("}");
            self.push_line("fn py_len<T: PyLen>(v: &T) -> i64 { v.py_len() }");
        }
        if self.uses.range {
            self.push_line("fn py_range(end: i64) -> std::ops::Range<i64> { 0..end }");
        }
        if self.uses.range2 {
            self.push_line(
                "fn py_range2(start: i64, end: i64) -> std::ops::Range<i64> { start..end }",
            );
        }
        if self.uses.range3 {
            // range(start, end, step) with arbitrary step values.
            // Positive step: count up from start to end.
            // Negative step: count down from start to end.
            // Step of 0 is an error (would create infinite loop).
            self.push_line(
                "fn py_range3(start: i64, end: i64, step: i64) -> Result<Box<dyn Iterator<Item = i64>>, PyError> {",
            );
            self.indent += 1;
            self.push_line(
                "if step == 0 { return Err(PyError::ValueError(String::from(\"range() arg 3 must not be zero\"))); }",
            );
            self.push_line("if step > 0 {");
            self.indent += 1;
            self.push_line("Ok(Box::new((start..end).step_by(step as usize)))");
            self.indent -= 1;
            self.push_line("} else {");
            self.indent += 1;
            self.push_line("let step = (-step) as usize;");
            self.push_line("if start <= end {");
            self.indent += 1;
            self.push_line("Ok(Box::new(std::iter::empty::<i64>()))");
            self.indent -= 1;
            self.push_line("} else {");
            self.indent += 1;
            self.push_line("Ok(Box::new(((end + 1)..=start).rev().step_by(step)))");
            self.indent -= 1;
            self.push_line("}");
            self.indent -= 1;
            self.push_line("}");
            self.indent -= 1;
            self.push_line("}");
        }
        if self.uses.round {
            // Python's round() uses "round half to even" (banker's rounding).
            // Unlike Rust's f64::round() which uses "round half away from zero".
            // Example: round(0.5) = 0, round(1.5) = 2, round(2.5) = 2
            // This minimizes bias in repeated rounding operations.
            self.push_line("fn py_round_ties_even(value: f64) -> f64 {");
            self.indent += 1;
            self.push_line("let rounded = value.round();");
            self.push_line("let diff = (value - rounded).abs();");
            // Check if we're exactly at 0.5 (within floating point epsilon).
            self.push_line("if (diff - 0.5).abs() < 1e-12 {");
            self.indent += 1;
            self.push_line("let floor = value.floor();");
            // Round to nearest even number.
            self.push_line("if (floor as i64) % 2 == 0 { floor } else { floor + 1.0 }");
            self.indent -= 1;
            self.push_line("} else {");
            self.indent += 1;
            self.push_line("rounded");
            self.indent -= 1;
            self.push_line("}");
            self.indent -= 1;
            self.push_line("}");
            self.push_line("fn py_round(value: f64, ndigits: i64) -> f64 {");
            self.indent += 1;
            self.push_line("let factor = 10f64.powi(ndigits as i32);");
            self.push_line("py_round_ties_even(value * factor) / factor");
            self.indent -= 1;
            self.push_line("}");
        }
        if self.uses.type_name {
            self.push_line("fn py_type_name<T: ?Sized>(value: &T) -> String {");
            self.indent += 1;
            self.push_line("std::any::type_name_of_val(value).to_string()");
            self.indent -= 1;
            self.push_line("}");
        }
        if self.uses.py_max {
            // Use PartialOrd to support floats and fall back to equality on NaN comparisons.
            self.push_line(
                "fn py_max<T: PartialOrd, I: IntoIterator<Item = T>>(iter: I) -> Result<T, PyError> {",
            );
            self.indent += 1;
            self.push_line("let mut iter = iter.into_iter();");
            self.push_line(
                "let mut best = iter.next().ok_or_else(|| PyError::ValueError(String::from(\"max() arg is an empty sequence\")))?;",
            );
            self.push_line("for item in iter {");
            self.indent += 1;
            self.push_line(
                "if item.partial_cmp(&best).unwrap_or(std::cmp::Ordering::Equal) == std::cmp::Ordering::Greater {",
            );
            self.indent += 1;
            self.push_line("best = item;");
            self.indent -= 1;
            self.push_line("}");
            self.indent -= 1;
            self.push_line("}");
            self.push_line("Ok(best)");
            self.indent -= 1;
            self.push_line("}");
        }
        if self.uses.py_min {
            // Use PartialOrd to support floats and fall back to equality on NaN comparisons.
            self.push_line(
                "fn py_min<T: PartialOrd, I: IntoIterator<Item = T>>(iter: I) -> Result<T, PyError> {",
            );
            self.indent += 1;
            self.push_line("let mut iter = iter.into_iter();");
            self.push_line(
                "let mut best = iter.next().ok_or_else(|| PyError::ValueError(String::from(\"min() arg is an empty sequence\")))?;",
            );
            self.push_line("for item in iter {");
            self.indent += 1;
            self.push_line(
                "if item.partial_cmp(&best).unwrap_or(std::cmp::Ordering::Equal) == std::cmp::Ordering::Less {",
            );
            self.indent += 1;
            self.push_line("best = item;");
            self.indent -= 1;
            self.push_line("}");
            self.indent -= 1;
            self.push_line("}");
            self.push_line("Ok(best)");
            self.indent -= 1;
            self.push_line("}");
        }
        if self.uses.py_parse_int {
            self.push_line("fn py_parse_int(s: &str) -> Result<i64, PyError> {");
            self.indent += 1;
            self.push_line("let trimmed = s.trim();");
            self.push_line(
                "trimmed.parse().map_err(|_| PyError::ValueError(format!(\"invalid literal for int() with base 10: '{}'\", trimmed)))",
            );
            self.indent -= 1;
            self.push_line("}");
        }
        if self.uses.py_parse_float {
            self.push_line("fn py_parse_float(s: &str) -> Result<f64, PyError> {");
            self.indent += 1;
            self.push_line("let trimmed = s.trim();");
            self.push_line(
                "trimmed.parse().map_err(|_| PyError::ValueError(format!(\"could not convert string to float: '{}'\", trimmed)))",
            );
            self.indent -= 1;
            self.push_line("}");
        }
        if self.uses.py_bytes_from_len {
            // Match Python bytes(n): negative sizes raise ValueError.
            self.push_line("fn py_bytes_from_len(len: i64) -> Result<Vec<i64>, PyError> {");
            self.indent += 1;
            self.push_line("if len < 0 {");
            self.indent += 1;
            self.push_line("return Err(PyError::ValueError(String::from(\"negative count\")));");
            self.indent -= 1;
            self.push_line("}");
            self.push_line("Ok(vec![0i64; len as usize])");
            self.indent -= 1;
            self.push_line("}");
        }
        if self.uses.py_bytes_from_str {
            // Only UTF-8 encoding is supported for bytes(str, encoding).
            self.push_line(
                "fn py_bytes_from_str(s: &str, encoding: &str) -> Result<Vec<i64>, PyError> {",
            );
            self.indent += 1;
            self.push_line("if encoding != \"utf-8\" {");
            self.indent += 1;
            self.push_line(
                "return Err(PyError::ValueError(String::from(\"unsupported encoding\")));",
            );
            self.indent -= 1;
            self.push_line("}");
            self.push_line("Ok(s.as_bytes().iter().map(|b| *b as i64).collect())");
            self.indent -= 1;
            self.push_line("}");
        }
        if self.uses.py_chr {
            self.push_line("fn py_chr(value: i64) -> Result<String, PyError> {");
            self.indent += 1;
            self.push_line("if value < 0 || value > 0x10FFFF {");
            self.indent += 1;
            self.push_line(
                "return Err(PyError::ValueError(String::from(\"chr() arg not in range(0x110000)\")));",
            );
            self.indent -= 1;
            self.push_line("}");
            self.push_line("match std::char::from_u32(value as u32) {");
            self.indent += 1;
            self.push_line("Some(c) => Ok(c.to_string()),");
            self.push_line(
                "None => Err(PyError::ValueError(String::from(\"chr() arg not in range(0x110000)\"))),",
            );
            self.indent -= 1;
            self.push_line("}");
            self.indent -= 1;
            self.push_line("}");
        }
        if self.uses.py_ord {
            self.push_line("fn py_ord(s: &str) -> Result<i64, PyError> {");
            self.indent += 1;
            self.push_line("let mut chars = s.chars();");
            self.push_line("match (chars.next(), chars.next()) {");
            self.indent += 1;
            self.push_line("(Some(c), None) => Ok(c as i64),");
            self.push_line(
                "_ => Err(PyError::TypeError(String::from(\"ord() expects a character\"))),",
            );
            self.indent -= 1;
            self.push_line("}");
            self.indent -= 1;
            self.push_line("}");
        }
        if self.uses.py_next {
            self.push_line("fn py_next<T>(value: Option<T>) -> Result<T, PyError> {");
            self.indent += 1;
            self.push_line("value.ok_or_else(|| PyError::StopIteration(String::new()))");
            self.indent -= 1;
            self.push_line("}");
        }
        if self.uses.py_dict_get {
            self.push_line(
                "fn py_dict_get<K: Eq + std::hash::Hash, V: Clone>(map: &HashMap<K, V>, key: &K) -> Result<V, PyError> {",
            );
            self.indent += 1;
            self.push_line(
                "map.get(key).cloned().ok_or_else(|| PyError::KeyError(String::from(\"KeyError\")))",
            );
            self.indent -= 1;
            self.push_line("}");
        }
        if self.uses.py_list_get {
            self.push_line(
                "fn py_list_get<T: Clone>(items: &[T], idx: i64) -> Result<T, PyError> {",
            );
            self.indent += 1;
            self.push_line("let len = items.len() as i64;");
            self.push_line("let adj = if idx < 0 { len + idx } else { idx };");
            self.push_line("if adj < 0 || adj >= len {");
            self.indent += 1;
            self.push_line("Err(PyError::IndexError(String::from(\"IndexError\")))");
            self.indent -= 1;
            self.push_line("} else {");
            self.indent += 1;
            self.push_line("Ok(items[adj as usize].clone())");
            self.indent -= 1;
            self.push_line("}");
            self.indent -= 1;
            self.push_line("}");
        }
        if self.uses.py_list_index {
            self.push_line(
                "fn py_list_index<T: PartialEq>(items: &[T], needle: &T) -> Result<i64, PyError> {",
            );
            self.indent += 1;
            self.push_line("for (idx, item) in items.iter().enumerate() {");
            self.indent += 1;
            self.push_line("if item == needle { return Ok(idx as i64); }");
            self.indent -= 1;
            self.push_line("}");
            self.push_line("Err(PyError::ValueError(String::from(\"ValueError\")))");
            self.indent -= 1;
            self.push_line("}");
        }
        if self.uses.py_list_count {
            self.push_line("fn py_list_count<T: PartialEq>(items: &[T], needle: &T) -> i64 {");
            self.indent += 1;
            self.push_line("let mut count = 0i64;");
            self.push_line("for item in items.iter() {");
            self.indent += 1;
            self.push_line("if item == needle { count += 1; }");
            self.indent -= 1;
            self.push_line("}");
            self.push_line("count");
            self.indent -= 1;
            self.push_line("}");
        }
        if self.uses.py_index {
            // Python supports negative indices: list[-1] is the last element.
            // Formula: negative index i becomes len + i.
            // Example: for list of length 5, index -1 becomes 5 + (-1) = 4.
            self.push_line("fn py_index(idx: i64, len: usize) -> Result<usize, PyError> {");
            self.indent += 1;
            self.push_line("let len_i = len as i64;");
            self.push_line("let adj = if idx < 0 { len_i + idx } else { idx };");
            self.push_line("if adj < 0 || adj >= len_i {");
            self.indent += 1;
            self.push_line("Err(PyError::IndexError(String::from(\"IndexError\")))");
            self.indent -= 1;
            self.push_line("} else {");
            self.indent += 1;
            self.push_line("Ok(adj as usize)");
            self.indent -= 1;
            self.push_line("}");
            self.indent -= 1;
            self.push_line("}");
        }
        if self.uses.py_insert_index {
            // Python list.insert clamps indices: <0 inserts at start, >len inserts at end.
            self.push_line("fn py_insert_index(idx: i64, len: usize) -> usize {");
            self.indent += 1;
            self.push_line("let len_i = len as i64;");
            self.push_line("let mut adj = if idx < 0 { len_i + idx } else { idx };");
            self.push_line("if adj < 0 { adj = 0; }");
            self.push_line("if adj > len_i { adj = len_i; }");
            self.push_line("adj as usize");
            self.indent -= 1;
            self.push_line("}");
        }
        if self.uses.py_str_slice {
            self.push_line(
                "fn py_str_slice(s: &str, start: Option<i64>, end: Option<i64>) -> String {",
            );
            self.indent += 1;
            self.push_line("let chars: Vec<char> = s.chars().collect();");
            self.push_line("let len = chars.len() as i64;");
            self.push_line("let start = start.map(|i| if i < 0 { (len + i).max(0) } else { i.min(len) }).unwrap_or(0) as usize;");
            self.push_line("let end = end.map(|i| if i < 0 { (len + i).max(0) } else { i.min(len) }).unwrap_or(len as i64) as usize;");
            self.push_line("chars[start..end.max(start)].iter().collect()");
            self.indent -= 1;
            self.push_line("}");
        }
        if self.uses.py_list_slice_step {
            self.push_line(
                "fn py_list_slice_step<T: Clone>(items: &[T], start: Option<i64>, end: Option<i64>, step: i64) -> Result<Vec<T>, PyError> {",
            );
            self.indent += 1;
            self.push_line(
                "if step == 0 { return Err(PyError::ValueError(String::from(\"slice step cannot be zero\"))); }",
            );
            self.push_line("let len = items.len() as i64;");
            self.push_line("let mut out = Vec::new();");
            self.push_line("if step > 0 {");
            self.indent += 1;
            self.push_line("let mut i = match start {");
            self.indent += 1;
            self.push_line(
                "Some(s) => { let s = if s < 0 { len + s } else { s }; s.max(0).min(len) },",
            );
            self.push_line("None => 0,");
            self.indent -= 1;
            self.push_line("};");
            self.push_line("let end = match end {");
            self.indent += 1;
            self.push_line(
                "Some(e) => { let e = if e < 0 { len + e } else { e }; e.max(0).min(len) },",
            );
            self.push_line("None => len,");
            self.indent -= 1;
            self.push_line("};");
            self.push_line("while i < end {");
            self.indent += 1;
            self.push_line("out.push(items[i as usize].clone());");
            self.push_line("i += step;");
            self.indent -= 1;
            self.push_line("}");
            self.indent -= 1;
            self.push_line("} else {");
            self.indent += 1;
            self.push_line("let mut i = match start {");
            self.indent += 1;
            self.push_line("Some(s) => { let s = if s < 0 { len + s } else { s }; if s < 0 { -1 } else if s >= len { len - 1 } else { s } },");
            self.push_line("None => len - 1,");
            self.indent -= 1;
            self.push_line("};");
            self.push_line("let end = match end {");
            self.indent += 1;
            self.push_line("Some(e) => { let e = if e < 0 { len + e } else { e }; if e < 0 { -1 } else if e >= len { len - 1 } else { e } },");
            self.push_line("None => -1,");
            self.indent -= 1;
            self.push_line("};");
            self.push_line("while i > end {");
            self.indent += 1;
            self.push_line("if i >= 0 && i < len { out.push(items[i as usize].clone()); }");
            self.push_line("i += step;");
            self.indent -= 1;
            self.push_line("}");
            self.indent -= 1;
            self.push_line("}");
            self.push_line("Ok(out)");
            self.indent -= 1;
            self.push_line("}");
        }
        if self.uses.py_str_slice_step {
            self.push_line(
                "fn py_str_slice_step(s: &str, start: Option<i64>, end: Option<i64>, step: i64) -> Result<String, PyError> {",
            );
            self.indent += 1;
            self.push_line("let chars: Vec<char> = s.chars().collect();");
            self.push_line("let sliced = py_list_slice_step(&chars, start, end, step)?;");
            self.push_line("Ok(sliced.into_iter().collect())");
            self.indent -= 1;
            self.push_line("}");
        }
        if self.uses.print
            || self.uses.len
            || self.uses.range
            || self.uses.range2
            || self.uses.range3
            || self.uses.round
            || self.uses.type_name
            || self.uses.py_max
            || self.uses.py_min
            || self.uses.py_parse_int
            || self.uses.py_parse_float
            || self.uses.py_chr
            || self.uses.py_ord
            || self.uses.py_next
            || self.uses.py_dict_get
            || self.uses.py_list_get
            || self.uses.py_index
            || self.uses.py_str_slice
            || self.uses.py_list_slice_step
            || self.uses.py_str_slice_step
        {
            self.push_line("");
        }
    }

    /// Decide whether the PyError enum and trait impls are needed.
    fn needs_py_error(&self) -> bool {
        self.top_level_can_throw
            || self.ctx.functions.values().any(|sig| sig.can_throw)
            || self.uses.py_parse_int
            || self.uses.py_parse_float
            || self.uses.py_index
            || self.uses.py_list_get
            || self.uses.py_dict_get
            || self.uses.py_chr
            || self.uses.py_ord
            || self.uses.py_next
            || self.uses.py_max
            || self.uses.py_min
            || self.uses.py_list_slice_step
            || self.uses.py_str_slice_step
            || self.uses.range3
    }

    /// Emit the PyError enum plus Display/Error implementations.
    fn emit_py_error_enum(&mut self) {
        self.push_line("#[derive(Debug, Clone)]");
        self.push_line("pub enum PyError {");
        self.indent += 1;

        // Built-in exceptions.
        self.push_line("ValueError(String),");
        self.push_line("TypeError(String),");
        self.push_line("RuntimeError(String),");
        self.push_line("KeyError(String),");
        self.push_line("IndexError(String),");
        self.push_line("AttributeError(String),");
        self.push_line("ZeroDivisionError(String),");
        self.push_line("NameError(String),");
        self.push_line("AssertionError(String),");
        self.push_line("StopIteration(String),");
        self.push_line("NotImplementedError(String),");
        self.push_line("IOError(String),");
        self.push_line("OverflowError(String),");

        self.indent -= 1;
        self.push_line("}");
        self.push_line("");

        // Implement Display.
        self.push_line("impl std::fmt::Display for PyError {");
        self.indent += 1;
        self.push_line("fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {");
        self.indent += 1;
        self.push_line("match self {");
        self.indent += 1;
        self.push_line("PyError::ValueError(msg) => write!(f, \"ValueError: {}\", msg),");
        self.push_line("PyError::TypeError(msg) => write!(f, \"TypeError: {}\", msg),");
        self.push_line("PyError::RuntimeError(msg) => write!(f, \"RuntimeError: {}\", msg),");
        self.push_line("PyError::KeyError(msg) => write!(f, \"KeyError: {}\", msg),");
        self.push_line("PyError::IndexError(msg) => write!(f, \"IndexError: {}\", msg),");
        self.push_line("PyError::AttributeError(msg) => write!(f, \"AttributeError: {}\", msg),");
        self.push_line(
            "PyError::ZeroDivisionError(msg) => write!(f, \"ZeroDivisionError: {}\", msg),",
        );
        self.push_line("PyError::NameError(msg) => write!(f, \"NameError: {}\", msg),");
        self.push_line("PyError::AssertionError(msg) => write!(f, \"AssertionError: {}\", msg),");
        self.push_line("PyError::StopIteration(msg) => write!(f, \"StopIteration: {}\", msg),");
        self.push_line(
            "PyError::NotImplementedError(msg) => write!(f, \"NotImplementedError: {}\", msg),",
        );
        self.push_line("PyError::IOError(msg) => write!(f, \"IOError: {}\", msg),");
        self.push_line("PyError::OverflowError(msg) => write!(f, \"OverflowError: {}\", msg),");
        self.indent -= 1;
        self.push_line("}");
        self.indent -= 1;
        self.push_line("}");
        self.indent -= 1;
        self.push_line("}");
        self.push_line("");

        // Implement std::error::Error.
        self.push_line("impl std::error::Error for PyError {}");
        self.push_line("");
    }
}
