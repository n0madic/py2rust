// Helper function emission for generated Rust files.

use super::super::*;

/// Static helper body for `print(...)` calls.
const HELPER_PY_PRINT: &str = r#"
fn py_print<T: std::fmt::Display + ?Sized>(v: &T) {
    println!("{v}");
}
"#;

/// Static helper body for normalizing integer operands that may be borrowed.
const HELPER_PY_INT: &str = r#"
fn py_int(value: impl std::borrow::Borrow<i64>) -> i64 {
    *value.borrow()
}
"#;

/// Static helper body for Python `type(...)` name extraction.
const HELPER_PY_TYPE_NAME: &str = r#"
fn py_type_name<T: ?Sized>(value: &T) -> String {
    std::any::type_name_of_val(value).to_string()
}
"#;

/// Static helper body for iterator `next(...)` behavior.
const HELPER_PY_NEXT: &str = r#"
fn py_next<T>(value: Option<T>) -> Result<T, PyError> {
    value.ok_or_else(|| PyError::StopIteration(String::new().into()))
}
"#;

/// Static helper body for Python-style string indexing with negative indices.
const HELPER_PY_STR_GET: &str = r#"
fn py_str_get(s: &str, idx: i64) -> Result<String, PyError> {
    if idx >= 0 {
        return s
            .chars()
            .nth(idx as usize)
            .map(|ch| ch.to_string())
            .ok_or_else(|| PyError::IndexError("IndexError".into()));
    }
    // Negative indexing is resolved from the end in one iterator pass.
    let from_end = idx
        .checked_neg()
        .and_then(|v| usize::try_from(v).ok())
        .ok_or_else(|| PyError::IndexError("IndexError".into()))?;
    s.chars()
        .rev()
        .nth(from_end.saturating_sub(1))
        .map(|ch| ch.to_string())
        .ok_or_else(|| PyError::IndexError("IndexError".into()))
}
"#;

/// Static helper body for `os.remove(path)`.
const HELPER_PY_OS_REMOVE: &str = r#"
fn py_os_remove(path: &str) -> Result<(), PyError> {
    std::fs::remove_file(path).map_err(|e| PyError::IOError(e.to_string().into()))
}
"#;

/// Static helper body for clonable iterator wrapper.
const HELPER_PY_ITER: &str = r#"
#[derive(Clone)]
struct PyIter<T> {
    inner: Arc<Mutex<Box<dyn Iterator<Item = T> + Send>>>,
}
impl<T> PyIter<T> {
    fn new<I: Iterator<Item = T> + Send + 'static>(iter: I) -> Self {
        Self { inner: Arc::new(Mutex::new(Box::new(iter))) }
    }
}
impl<T> Iterator for PyIter<T> {
    type Item = T;
    fn next(&mut self) -> Option<Self::Item> {
        self.inner.lock().expect("PyIter mutex poisoned").next()
    }
}
trait PyIteratorSendClose<T>: Iterator<Item = T> {
    fn send(&mut self, _value: T) -> T {
        self.next()
            .unwrap_or_else(|| panic!("generator is exhausted"))
    }
    fn close(&mut self) {}
}
impl<I, T> PyIteratorSendClose<T> for I where I: Iterator<Item = T> {}
fn py_iter<T, I>(iter: I) -> PyIter<T>
where
    I: Iterator<Item = T> + Send + 'static,
{
    PyIter::new(iter)
}
"#;

/// Static helper body for `len(...)` support across core collection types.
const HELPER_PY_LEN: &str = r#"
trait PyLen {
    fn py_len(&self) -> i64;
}
impl<T> PyLen for Vec<T> {
    fn py_len(&self) -> i64 { self.len() as i64 }
}
impl<T> PyLen for Arc<Mutex<Vec<T>>> {
    fn py_len(&self) -> i64 { self.lock().expect("list mutex poisoned").len() as i64 }
}
impl<T> PyLen for Rc<RefCell<Vec<T>>> {
    fn py_len(&self) -> i64 { self.borrow().len() as i64 }
}
impl PyLen for String {
    fn py_len(&self) -> i64 { self.chars().count() as i64 }
}
impl PyLen for &str {
    fn py_len(&self) -> i64 { self.chars().count() as i64 }
}
impl<K, V> PyLen for std::collections::HashMap<K, V> {
    fn py_len(&self) -> i64 { self.len() as i64 }
}
impl<K, V> PyLen for Arc<Mutex<std::collections::HashMap<K, V>>> {
    fn py_len(&self) -> i64 { self.lock().expect("dict mutex poisoned").len() as i64 }
}
impl<K, V> PyLen for Rc<RefCell<std::collections::HashMap<K, V>>> {
    fn py_len(&self) -> i64 { self.borrow().len() as i64 }
}
impl<T> PyLen for std::collections::HashSet<T> {
    fn py_len(&self) -> i64 { self.len() as i64 }
}
impl PyLen for () {
    fn py_len(&self) -> i64 { 0 }
}
impl<T1> PyLen for (T1,) {
    fn py_len(&self) -> i64 { 1 }
}
impl<T1, T2> PyLen for (T1, T2) {
    fn py_len(&self) -> i64 { 2 }
}
impl<T1, T2, T3> PyLen for (T1, T2, T3) {
    fn py_len(&self) -> i64 { 3 }
}
impl<T1, T2, T3, T4> PyLen for (T1, T2, T3, T4) {
    fn py_len(&self) -> i64 { 4 }
}
impl<T1, T2, T3, T4, T5> PyLen for (T1, T2, T3, T4, T5) {
    fn py_len(&self) -> i64 { 5 }
}
impl<T1, T2, T3, T4, T5, T6> PyLen for (T1, T2, T3, T4, T5, T6) {
    fn py_len(&self) -> i64 { 6 }
}
impl<T1, T2, T3, T4, T5, T6, T7> PyLen for (T1, T2, T3, T4, T5, T6, T7) {
    fn py_len(&self) -> i64 { 7 }
}
impl<T1, T2, T3, T4, T5, T6, T7, T8> PyLen for (T1, T2, T3, T4, T5, T6, T7, T8) {
    fn py_len(&self) -> i64 { 8 }
}
fn py_len<T: PyLen>(v: &T) -> i64 { v.py_len() }
"#;

/// Static helper body for list slicing with arbitrary step.
const HELPER_PY_LIST_SLICE_STEP: &str = r#"
fn py_list_slice_step<T: Clone>(items: &[T], start: Option<i64>, end: Option<i64>, step: i64) -> Result<Vec<T>, PyError> {
    if step == 0 { return Err(PyError::ValueError("slice step cannot be zero".into())); }
    let len = items.len() as i64;
    let mut out = Vec::new();
    if step > 0 {
        let mut i = match start {
            Some(s) => { let s = if s < 0 { len + s } else { s }; s.max(0).min(len) },
            None => 0,
        };
        let end = match end {
            Some(e) => { let e = if e < 0 { len + e } else { e }; e.max(0).min(len) },
            None => len,
        };
        while i < end {
            out.push(items[i as usize].clone());
            i += step;
        }
    } else {
        let mut i = match start {
            Some(s) => { let s = if s < 0 { len + s } else { s }; if s < 0 { -1 } else if s >= len { len - 1 } else { s } },
            None => len - 1,
        };
        let end = match end {
            Some(e) => { let e = if e < 0 { len + e } else { e }; if e < 0 { -1 } else if e >= len { len - 1 } else { e } },
            None => -1,
        };
        while i > end {
            if i >= 0 && i < len { out.push(items[i as usize].clone()); }
            i += step;
        }
    }
    Ok(out)
}
"#;

/// Static helper body for string slicing with arbitrary step.
const HELPER_PY_STR_SLICE_STEP: &str = r#"
fn py_str_slice_step(s: &str, start: Option<i64>, end: Option<i64>, step: i64) -> Result<String, PyError> {
    let chars: Vec<char> = s.chars().collect();
    let sliced = py_list_slice_step(&chars, start, end, step)?;
    Ok(sliced.into_iter().collect())
}
"#;

/// Static helper body for file open/read/write helpers.
const HELPER_PY_FILE: &str = r#"
fn py_open(path: &str, mode: &str) -> Result<std::fs::File, PyError> {
    match mode {
        "r" => std::fs::File::open(path).map_err(|e| PyError::IOError(e.to_string().into())),
        "w" => std::fs::OpenOptions::new().create(true).write(true).truncate(true).open(path).map_err(|e| PyError::IOError(e.to_string().into())),
        "a" => std::fs::OpenOptions::new().create(true).append(true).open(path).map_err(|e| PyError::IOError(e.to_string().into())),
        _ => Err(PyError::ValueError(format!("unsupported file mode: {}", mode).into())),
    }
}
fn py_file_read(file: &mut std::fs::File, n: Option<i64>) -> Result<String, PyError> {
    use std::io::Read;
    if let Some(limit) = n {
        if limit >= 0 {
            let mut buf = vec![0u8; limit as usize];
            let read = file.read(&mut buf).map_err(|e| PyError::IOError(e.to_string().into()))?;
            buf.truncate(read);
            return String::from_utf8(buf).map_err(|e| PyError::IOError(e.to_string().into()));
        }
    }
    let mut out = String::new();
    file.read_to_string(&mut out).map_err(|e| PyError::IOError(e.to_string().into()))?;
    Ok(out)
}
fn py_file_readline(file: &mut std::fs::File) -> Result<String, PyError> {
    use std::io::Read;
    let mut bytes = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        let read = file.read(&mut byte).map_err(|e| PyError::IOError(e.to_string().into()))?;
        if read == 0 { break; }
        bytes.push(byte[0]);
        if byte[0] == b'\n' { break; }
    }
    String::from_utf8(bytes).map_err(|e| PyError::IOError(e.to_string().into()))
}
fn py_file_readlines(file: &mut std::fs::File) -> Result<Vec<String>, PyError> {
    let mut lines = Vec::new();
    loop {
        let line = py_file_readline(file)?;
        if line.is_empty() { break; }
        lines.push(line);
    }
    Ok(lines)
}
fn py_file_write(file: &mut std::fs::File, data: &str) -> Result<i64, PyError> {
    use std::io::Write;
    file.write_all(data.as_bytes()).map_err(|e| PyError::IOError(e.to_string().into()))?;
    Ok(data.len() as i64)
}
fn py_file_close(file: &mut std::fs::File) -> Result<(), PyError> {
    use std::io::Write;
    file.flush().map_err(|e| PyError::IOError(e.to_string().into()))
}
"#;

/// Static helper body for Python-compatible float string formatting.
const HELPER_PY_FLOAT_STR: &str = r#"
fn py_float_str(v: f64) -> String {
    if v.is_nan() { return "nan".to_string(); }
    if v.is_infinite() {
        return if v.is_sign_negative() { "-inf".to_string() } else { "inf".to_string() };
    }
    let mut s = v.to_string();
    if !s.contains('.') && !s.contains('e') && !s.contains('E') {
        s.push_str(".0");
    }
    s
}
"#;

/// Static helper body for Python-style string repr escaping.
const HELPER_PY_STR_REPR: &str = r#"
fn py_str_repr(s: &str) -> String {
    let use_double = s.contains('\'') && !s.contains('"');
    let quote = if use_double { '"' } else { '\'' };
    let mut out = String::new();
    out.push(quote);
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\x08' => out.push_str("\\x08"),
            '\x0c' => out.push_str("\\x0c"),
            '\'' if quote == '\'' => out.push_str("\\'"),
            '"' if quote == '"' => out.push_str("\\\""),
            _ => out.push(ch),
        }
    }
    out.push(quote);
    out
}
"#;

/// Static helper body for Python `ascii(...)` escaping.
const HELPER_PY_ASCII: &str = r#"
fn py_ascii_escape(s: &str) -> String {
    let mut out = String::new();
    for ch in s.chars() {
        if ch.is_ascii() {
            out.push(ch);
            continue;
        }
        let code = ch as u32;
        if code <= 0xFF {
            out.push_str(&format!("\\x{:02x}", code));
        } else if code <= 0xFFFF {
            out.push_str(&format!("\\u{:04x}", code));
        } else {
            out.push_str(&format!("\\U{:08x}", code));
        }
    }
    out
}
"#;

/// Static helper body for Python string method compatibility.
const HELPER_PY_STRING_METHODS: &str = r#"
fn py_str_split_whitespace(s: &str, maxsplit: Option<i64>) -> Vec<String> {
    let maxsplit = maxsplit.unwrap_or(-1);
    if maxsplit < 0 {
        return s.split_whitespace().map(|part| part.to_string()).collect();
    }
    let limit = maxsplit as usize;
    if limit == 0 {
        let trimmed = s.trim_start();
        if trimmed.is_empty() {
            return Vec::new();
        }
        return vec![trimmed.to_string()];
    }
    let mut raw: Vec<String> = s.split_whitespace().map(|part| part.to_string()).collect();
    if raw.len() <= limit + 1 {
        return raw;
    }
    let mut out: Vec<String> = raw.drain(..limit).collect();
    out.push(raw.join(" "));
    out
}

fn py_str_split_sep(s: &str, sep: &str, maxsplit: Option<i64>) -> Vec<String> {
    if maxsplit.unwrap_or(-1) < 0 {
        return s.split(sep).map(|part| part.to_string()).collect();
    }
    let limit = maxsplit.unwrap_or(-1).max(0) as usize + 1;
    s.splitn(limit, sep).map(|part| part.to_string()).collect()
}

fn py_str_count(s: &str, needle: &str) -> i64 {
    if needle.is_empty() {
        return s.chars().count() as i64 + 1;
    }
    s.matches(needle).count() as i64
}

fn py_str_title(s: &str) -> String {
    let mut out = String::new();
    let mut at_word_start = true;
    for ch in s.chars() {
        if ch.is_alphabetic() {
            if at_word_start {
                for upper in ch.to_uppercase() {
                    out.push(upper);
                }
                at_word_start = false;
            } else {
                for lower in ch.to_lowercase() {
                    out.push(lower);
                }
            }
        } else {
            at_word_start = true;
            out.push(ch);
        }
    }
    out
}

fn py_str_capitalize(s: &str) -> String {
    let mut chars = s.chars();
    let mut out = String::new();
    if let Some(first) = chars.next() {
        for upper in first.to_uppercase() {
            out.push(upper);
        }
        for ch in chars {
            for lower in ch.to_lowercase() {
                out.push(lower);
            }
        }
    }
    out
}

fn py_str_swapcase(s: &str) -> String {
    let mut out = String::new();
    for ch in s.chars() {
        if ch.is_lowercase() {
            for upper in ch.to_uppercase() {
                out.push(upper);
            }
        } else if ch.is_uppercase() {
            for lower in ch.to_lowercase() {
                out.push(lower);
            }
        } else {
            out.push(ch);
        }
    }
    out
}

fn py_str_lstrip_chars(s: &str, chars: &str) -> String {
    if chars.is_empty() {
        return s.to_string();
    }
    let mut start = 0usize;
    for (idx, ch) in s.char_indices() {
        if chars.contains(ch) {
            start = idx + ch.len_utf8();
        } else {
            break;
        }
    }
    s[start..].to_string()
}

fn py_str_rstrip_chars(s: &str, chars: &str) -> String {
    if chars.is_empty() {
        return s.to_string();
    }
    let mut end = s.len();
    for (idx, ch) in s.char_indices().rev() {
        if chars.contains(ch) {
            end = idx;
        } else {
            break;
        }
    }
    s[..end].to_string()
}

fn py_fill_char(fill: &str) -> char {
    fill.chars().next().unwrap_or(' ')
}

fn py_str_center(s: &str, width: i64, fill: char) -> String {
    let len = s.chars().count() as i64;
    if width <= len {
        return s.to_string();
    }
    let pad = (width - len) as usize;
    let left = pad / 2;
    let right = pad - left;
    let fill_s = fill.to_string();
    format!("{}{}{}", fill_s.repeat(left), s, fill_s.repeat(right))
}

fn py_str_ljust(s: &str, width: i64, fill: char) -> String {
    let len = s.chars().count() as i64;
    if width <= len {
        return s.to_string();
    }
    let pad = (width - len) as usize;
    let fill_s = fill.to_string();
    format!("{}{}", s, fill_s.repeat(pad))
}

fn py_str_rjust(s: &str, width: i64, fill: char) -> String {
    let len = s.chars().count() as i64;
    if width <= len {
        return s.to_string();
    }
    let pad = (width - len) as usize;
    let fill_s = fill.to_string();
    format!("{}{}", fill_s.repeat(pad), s)
}

fn py_str_zfill(s: &str, width: i64) -> String {
    let width = width.max(0) as usize;
    let len = s.chars().count();
    if len >= width {
        return s.to_string();
    }
    let pad = width - len;
    if let Some(first) = s.chars().next() {
        if first == '+' || first == '-' {
            let mut out = String::new();
            out.push(first);
            out.push_str(&"0".repeat(pad));
            out.push_str(&s[first.len_utf8()..]);
            return out;
        }
    }
    format!("{}{}", "0".repeat(pad), s)
}

fn py_str_isdigit(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|ch| ch.is_ascii_digit())
}

fn py_str_isalpha(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|ch| ch.is_alphabetic())
}

fn py_str_isalnum(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|ch| ch.is_alphanumeric())
}

fn py_str_isspace(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|ch| ch.is_whitespace())
}

fn py_str_isupper(s: &str) -> bool {
    let mut has_cased = false;
    for ch in s.chars() {
        if ch.is_alphabetic() {
            has_cased = true;
            if ch.is_lowercase() {
                return false;
            }
        }
    }
    has_cased
}

fn py_str_islower(s: &str) -> bool {
    let mut has_cased = false;
    for ch in s.chars() {
        if ch.is_alphabetic() {
            has_cased = true;
            if ch.is_uppercase() {
                return false;
            }
        }
    }
    has_cased
}
"#;

/// Static helper body for `range(start, end, step)`.
const HELPER_PY_RANGE3: &str = r#"
fn py_range3(start: i64, end: i64, step: i64) -> Result<Box<dyn Iterator<Item = i64>>, PyError> {
    if step == 0 { return Err(PyError::ValueError("range() arg 3 must not be zero".into())); }
    if step > 0 {
        Ok(Box::new((start..end).step_by(step as usize)))
    } else {
        let step = (-step) as usize;
        if start <= end {
            Ok(Box::new(std::iter::empty::<i64>()))
        } else {
            Ok(Box::new(((end + 1)..=start).rev().step_by(step)))
        }
    }
}
"#;

/// Static helper body for Python-style banker rounding.
const HELPER_PY_ROUND: &str = r#"
fn py_round_ties_even(value: f64) -> f64 {
    let rounded = value.round();
    let diff = (value - rounded).abs();
    if (diff - 0.5).abs() < 1e-12 {
        let floor = value.floor();
        if (floor as i64) % 2 == 0 { floor } else { floor + 1.0 }
    } else {
        rounded
    }
}
fn py_round(value: f64, ndigits: i64) -> f64 {
    let factor = 10f64.powi(ndigits as i32);
    py_round_ties_even(value * factor) / factor
}
"#;

/// Static helper body for `max(...)` over iterables.
const HELPER_PY_MAX: &str = r#"
fn py_max<T: PartialOrd, I: IntoIterator<Item = T>>(iter: I) -> Result<T, PyError> {
    let mut iter = iter.into_iter();
    let mut best = iter.next().ok_or_else(|| PyError::ValueError("max() arg is an empty sequence".into()))?;
    for item in iter {
        if item.partial_cmp(&best).unwrap_or(std::cmp::Ordering::Equal) == std::cmp::Ordering::Greater {
            best = item;
        }
    }
    Ok(best)
}
"#;

/// Static helper body for `min(...)` over iterables.
const HELPER_PY_MIN: &str = r#"
fn py_min<T: PartialOrd, I: IntoIterator<Item = T>>(iter: I) -> Result<T, PyError> {
    let mut iter = iter.into_iter();
    let mut best = iter.next().ok_or_else(|| PyError::ValueError("min() arg is an empty sequence".into()))?;
    for item in iter {
        if item.partial_cmp(&best).unwrap_or(std::cmp::Ordering::Equal) == std::cmp::Ordering::Less {
            best = item;
        }
    }
    Ok(best)
}
"#;

/// Static helper body for `list` string representation helpers (part 1).
const HELPER_PY_LIST_REPR_HEAD: &str = r#"
trait PyListRepr {
    fn py_repr(&self) -> String;
}
impl PyListRepr for i64 {
    fn py_repr(&self) -> String { self.to_string() }
}
impl PyListRepr for f64 {
    fn py_repr(&self) -> String { py_float_str(*self) }
}
impl PyListRepr for bool {
    fn py_repr(&self) -> String { if *self { "True".to_string() } else { "False".to_string() } }
}
impl PyListRepr for String {
    fn py_repr(&self) -> String { py_str_repr(self) }
}
"#;

/// Static helper body for `PyRepr` list representation support.
const HELPER_PY_LIST_REPR_PY_REPR: &str = r#"
impl PyListRepr for PyRepr {
    fn py_repr(&self) -> String { self.0.clone() }
}
"#;

/// Static helper body for `list` string representation helpers (part 2).
const HELPER_PY_LIST_REPR_TAIL: &str = r#"
impl<T1: PyListRepr> PyListRepr for (T1,) {
    fn py_repr(&self) -> String { format!("({},)", self.0.py_repr()) }
}
impl<T1: PyListRepr, T2: PyListRepr> PyListRepr for (T1, T2) {
    fn py_repr(&self) -> String { format!("({}, {})", self.0.py_repr(), self.1.py_repr()) }
}
impl<T1: PyListRepr, T2: PyListRepr, T3: PyListRepr> PyListRepr for (T1, T2, T3) {
    fn py_repr(&self) -> String { format!("({}, {}, {})", self.0.py_repr(), self.1.py_repr(), self.2.py_repr()) }
}
impl<T1: PyListRepr, T2: PyListRepr, T3: PyListRepr, T4: PyListRepr> PyListRepr for (T1, T2, T3, T4) {
    fn py_repr(&self) -> String { format!("({}, {}, {}, {})", self.0.py_repr(), self.1.py_repr(), self.2.py_repr(), self.3.py_repr()) }
}
impl<T: PyListRepr> PyListRepr for Vec<T> {
    fn py_repr(&self) -> String { py_list_str_vec(self) }
}
impl<T: PyListRepr> PyListRepr for Arc<Mutex<Vec<T>>> {
    fn py_repr(&self) -> String { py_list_str(self) }
}
impl<T: PyListRepr> PyListRepr for Rc<RefCell<Vec<T>>> {
    fn py_repr(&self) -> String { py_list_str_rc(self) }
}
fn py_list_str_vec<T: PyListRepr>(list: &[T]) -> String {
    let mut out = "[".to_string();
    for (idx, item) in list.iter().enumerate() {
        if idx > 0 { out.push_str(", "); }
        out.push_str(&item.py_repr());
    }
    out.push(']');
    out
}
fn py_list_str<T: PyListRepr>(list: &Arc<Mutex<Vec<T>>>) -> String {
    let guard = list.lock().expect("list mutex poisoned");
    py_list_str_vec(&guard)
}
fn py_list_str_rc<T: PyListRepr>(list: &Rc<RefCell<Vec<T>>>) -> String {
    let guard = list.borrow();
    py_list_str_vec(&guard)
}
"#;

/// Static helper body for PyError enum and trait impls.
const HELPER_PY_ERROR_ENUM: &str = r#"
pub type PyErrorMsg = std::borrow::Cow<'static, str>;

#[derive(Debug, Clone)]
pub enum PyError {
    Exception(PyErrorMsg),
    ValueError(PyErrorMsg),
    TypeError(PyErrorMsg),
    RuntimeError(PyErrorMsg),
    KeyError(PyErrorMsg),
    IndexError(PyErrorMsg),
    AttributeError(PyErrorMsg),
    ZeroDivisionError(PyErrorMsg),
    NameError(PyErrorMsg),
    AssertionError(PyErrorMsg),
    StopIteration(PyErrorMsg),
    NotImplementedError(PyErrorMsg),
    IOError(PyErrorMsg),
    OverflowError(PyErrorMsg),
    GeneratorExit(PyErrorMsg),
    MemoryError(PyErrorMsg),
}

impl std::fmt::Display for PyError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            PyError::Exception(msg) => write!(f, "Exception: {}", msg),
            PyError::ValueError(msg) => write!(f, "ValueError: {}", msg),
            PyError::TypeError(msg) => write!(f, "TypeError: {}", msg),
            PyError::RuntimeError(msg) => write!(f, "RuntimeError: {}", msg),
            PyError::KeyError(msg) => write!(f, "KeyError: {}", msg),
            PyError::IndexError(msg) => write!(f, "IndexError: {}", msg),
            PyError::AttributeError(msg) => write!(f, "AttributeError: {}", msg),
            PyError::ZeroDivisionError(msg) => write!(f, "ZeroDivisionError: {}", msg),
            PyError::NameError(msg) => write!(f, "NameError: {}", msg),
            PyError::AssertionError(msg) => write!(f, "AssertionError: {}", msg),
            PyError::StopIteration(msg) => write!(f, "StopIteration: {}", msg),
            PyError::NotImplementedError(msg) => write!(f, "NotImplementedError: {}", msg),
            PyError::IOError(msg) => write!(f, "IOError: {}", msg),
            PyError::OverflowError(msg) => write!(f, "OverflowError: {}", msg),
            PyError::GeneratorExit(msg) => write!(f, "GeneratorExit: {}", msg),
            PyError::MemoryError(msg) => write!(f, "MemoryError: {}", msg),
        }
    }
}

impl std::error::Error for PyError {}
"#;

impl<'a> Codegen<'a> {
    /// Emit a raw helper block line-by-line with current indentation.
    ///
    /// The input block should be left-aligned Rust source.
    fn push_block(&mut self, block: &str) {
        for line in block.trim_matches('\n').lines() {
            self.push_line(line);
        }
    }

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
        // List/tuple repr relies on shared float/string repr helpers.
        if self.uses.py_list_str {
            self.uses.py_float_str = true;
            self.uses.py_str_repr = true;
        }

        // PyError enum is needed for exception handling.
        if self.needs_py_error() {
            self.emit_py_error_enum();
        }

        // PyIter wrapper makes iterators clonable (Python's for loops clone iterators).
        if self.uses.py_iter {
            // Poisoned mutex means iterator state is invalid; panic with context.
            self.push_block(HELPER_PY_ITER);
        }

        if self.uses.print {
            self.push_block(HELPER_PY_PRINT);
        }
        if self.uses.py_int {
            self.push_block(HELPER_PY_INT);
        }
        if self.uses.py_repr {
            // PyRepr wraps preformatted strings so list Debug output matches Python repr style.
            self.push_line("#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]");
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
        if self.uses.py_float_str {
            self.push_block(HELPER_PY_FLOAT_STR);
        }
        if self.uses.py_str_repr {
            self.push_block(HELPER_PY_STR_REPR);
        }
        if self.uses.py_ascii {
            self.push_block(HELPER_PY_ASCII);
        }
        if self.uses.py_string_methods {
            self.push_block(HELPER_PY_STRING_METHODS);
        }
        if self.uses.len {
            // Tuples have fixed arity, so provide PyLen impls for common tuple sizes.
            self.push_block(HELPER_PY_LEN);
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
            self.push_block(HELPER_PY_RANGE3);
        }
        if self.uses.round {
            // Python's round() uses "round half to even" (banker's rounding).
            // Unlike Rust's f64::round() which uses "round half away from zero".
            // Example: round(0.5) = 0, round(1.5) = 2, round(2.5) = 2
            // This minimizes bias in repeated rounding operations.
            self.push_block(HELPER_PY_ROUND);
        }
        if self.uses.type_name {
            self.push_block(HELPER_PY_TYPE_NAME);
        }
        if self.uses.py_max {
            // Use PartialOrd to support floats and fall back to equality on NaN comparisons.
            self.push_block(HELPER_PY_MAX);
        }
        if self.uses.py_min {
            // Use PartialOrd to support floats and fall back to equality on NaN comparisons.
            self.push_block(HELPER_PY_MIN);
        }
        if self.uses.py_parse_int {
            self.push_line("fn py_parse_int(s: &str) -> Result<i64, PyError> {");
            self.indent += 1;
            self.push_line("let trimmed = s.trim();");
            self.push_line(
                "trimmed.parse().map_err(|_| PyError::ValueError(format!(\"invalid literal for int() with base 10: '{}'\", trimmed).into()))",
            );
            self.indent -= 1;
            self.push_line("}");
        }
        if self.uses.py_parse_float {
            self.push_line("fn py_parse_float(s: &str) -> Result<f64, PyError> {");
            self.indent += 1;
            self.push_line("let trimmed = s.trim();");
            self.push_line(
                "trimmed.parse().map_err(|_| PyError::ValueError(format!(\"could not convert string to float: '{}'\", trimmed).into()))",
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
            self.push_line("return Err(PyError::ValueError(\"negative count\".into()));");
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
            self.push_line("return Err(PyError::ValueError(\"unsupported encoding\".into()));");
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
                "return Err(PyError::ValueError(\"chr() arg not in range(0x110000)\".into()));",
            );
            self.indent -= 1;
            self.push_line("}");
            self.push_line("match std::char::from_u32(value as u32) {");
            self.indent += 1;
            self.push_line("Some(c) => Ok(c.to_string()),");
            self.push_line(
                "None => Err(PyError::ValueError(\"chr() arg not in range(0x110000)\".into())),",
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
            self.push_line("_ => Err(PyError::TypeError(\"ord() expects a character\".into())),");
            self.indent -= 1;
            self.push_line("}");
            self.indent -= 1;
            self.push_line("}");
        }
        if self.uses.py_next {
            self.push_block(HELPER_PY_NEXT);
        }
        if self.uses.py_dict_get {
            self.push_line(
                "fn py_dict_get<K: Eq + std::hash::Hash, V: Clone>(map: &HashMap<K, V>, key: &K) -> Result<V, PyError> {",
            );
            self.indent += 1;
            self.push_line(
                "map.get(key).cloned().ok_or_else(|| PyError::KeyError(\"KeyError\".into()))",
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
            self.push_line("Err(PyError::IndexError(\"IndexError\".into()))");
            self.indent -= 1;
            self.push_line("} else {");
            self.indent += 1;
            self.push_line("Ok(items[adj as usize].clone())");
            self.indent -= 1;
            self.push_line("}");
            self.indent -= 1;
            self.push_line("}");
        }
        if self.uses.py_str_get {
            self.push_block(HELPER_PY_STR_GET);
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
            self.push_line("Err(PyError::ValueError(\"ValueError\".into()))");
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
        if self.uses.py_list_str {
            self.push_block(HELPER_PY_LIST_REPR_HEAD);
            if self.uses.py_repr {
                self.push_block(HELPER_PY_LIST_REPR_PY_REPR);
            }
            self.push_block(HELPER_PY_LIST_REPR_TAIL);
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
            self.push_line("Err(PyError::IndexError(\"IndexError\".into()))");
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
            self.push_block(HELPER_PY_LIST_SLICE_STEP);
        }
        if self.uses.py_str_slice_step {
            self.push_block(HELPER_PY_STR_SLICE_STEP);
        }
        if self.uses.py_file {
            self.push_block(HELPER_PY_FILE);
        }
        if self.uses.py_file || self.uses.py_os_remove {
            self.push_block(HELPER_PY_OS_REMOVE);
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
            || self.uses.py_str_get
            || self.uses.py_index
            || self.uses.py_ascii
            || self.uses.py_string_methods
            || self.uses.py_str_slice
            || self.uses.py_list_slice_step
            || self.uses.py_str_slice_step
            || self.uses.py_file
            || self.uses.py_os_remove
        {
            self.push_line("");
        }
    }

    /// Decide whether the PyError enum and trait impls are needed.
    fn needs_py_error(&self) -> bool {
        self.top_level_can_throw
            || self.uses.py_error
            || self.ctx.functions.values().any(|sig| sig.can_throw)
            || self.uses.py_parse_int
            || self.uses.py_parse_float
            || self.uses.py_index
            || self.uses.py_list_get
            || self.uses.py_str_get
            || self.uses.py_dict_get
            || self.uses.py_chr
            || self.uses.py_ord
            || self.uses.py_next
            || self.uses.py_max
            || self.uses.py_min
            || self.uses.py_list_slice_step
            || self.uses.py_str_slice_step
            || self.uses.py_file
            || self.uses.py_os_remove
            || self.uses.range3
    }

    /// Emit the PyError enum plus Display/Error implementations.
    fn emit_py_error_enum(&mut self) {
        self.push_block(HELPER_PY_ERROR_ENUM);
        self.push_line("");
    }
}
