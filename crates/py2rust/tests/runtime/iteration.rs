#![allow(unused, non_snake_case, non_camel_case_types)]
use std::collections::HashMap;

#[derive(Clone)]
struct IndexMap<K: Eq + std::hash::Hash + Clone, V: Clone> {
    entries: Vec<(K, V)>,
    index: HashMap<K, usize>,
}
impl<K: Eq + std::hash::Hash + Clone, V: Clone> IndexMap<K, V> {
    fn new() -> Self { Self { entries: Vec::new(), index: HashMap::new() } }
    fn insert(&mut self, key: K, value: V) -> Option<V> {
        if let Some(&idx) = self.index.get(&key) {
            let old = std::mem::replace(&mut self.entries[idx].1, value);
            Some(old)
        } else {
            let idx = self.entries.len();
            self.index.insert(key.clone(), idx);
            self.entries.push((key, value));
            None
        }
    }
    fn get<Q: ?Sized + Eq + std::hash::Hash>(&self, key: &Q) -> Option<&V>
    where K: std::borrow::Borrow<Q> {
        self.index.get(key).map(|&idx| &self.entries[idx].1)
    }
    fn get_mut<Q: ?Sized + Eq + std::hash::Hash>(&mut self, key: &Q) -> Option<&mut V>
    where K: std::borrow::Borrow<Q> {
        self.index.get(key).copied().map(move |idx| &mut self.entries[idx].1)
    }
    fn contains_key<Q: ?Sized + Eq + std::hash::Hash>(&self, key: &Q) -> bool
    where K: std::borrow::Borrow<Q> {
        self.index.contains_key(key)
    }
    fn remove<Q: ?Sized + Eq + std::hash::Hash>(&mut self, key: &Q) -> Option<V>
    where K: std::borrow::Borrow<Q> {
        self.shift_remove(key)
    }
    fn shift_remove<Q: ?Sized + Eq + std::hash::Hash>(&mut self, key: &Q) -> Option<V>
    where K: std::borrow::Borrow<Q> {
        if let Some(&idx) = self.index.get(key) {
            let (_k, v) = self.entries.remove(idx);
            self.index.remove(key);
            // Re-index entries after the removed one.
            for (&ref ek, ei) in self.index.iter_mut() {
                if *ei > idx { *ei -= 1; }
            }
            Some(v)
        } else {
            None
        }
    }
    fn keys(&self) -> impl Iterator<Item = &K> { self.entries.iter().map(|(k, _)| k) }
    fn values(&self) -> impl Iterator<Item = &V> { self.entries.iter().map(|(_, v)| v) }
    fn iter(&self) -> impl Iterator<Item = (&K, &V)> { self.entries.iter().map(|(k, v)| (k, v)) }
    fn iter_mut(&mut self) -> impl Iterator<Item = (&K, &mut V)> { self.entries.iter_mut().map(|(k, v)| (&*k, v)) }
    fn len(&self) -> usize { self.entries.len() }
    fn is_empty(&self) -> bool { self.entries.is_empty() }
    fn entry(&mut self, key: K) -> IndexMapEntry<'_, K, V> {
        if self.index.contains_key(&key) {
            let idx = self.index[&key];
            IndexMapEntry::Occupied(IndexMapOccupiedEntry { entries: &mut self.entries, idx })
        } else {
            IndexMapEntry::Vacant(IndexMapVacantEntry { map: self, key })
        }
    }
    fn into_keys(self) -> impl Iterator<Item = K> { self.entries.into_iter().map(|(k, _)| k) }
    fn into_values(self) -> impl Iterator<Item = V> { self.entries.into_iter().map(|(_, v)| v) }
}
impl<K: Eq + std::hash::Hash + Clone + PartialEq, V: Clone + PartialEq> PartialEq for IndexMap<K, V> {
    fn eq(&self, other: &Self) -> bool {
        if self.entries.len() != other.entries.len() { return false; }
        self.entries.iter().all(|(k, v)| other.get(k) == Some(v))
    }
}
impl<K: Eq + std::hash::Hash + Clone + std::fmt::Debug, V: Clone + std::fmt::Debug> std::fmt::Debug for IndexMap<K, V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_map().entries(self.entries.iter().map(|(k, v)| (k, v))).finish()
    }
}
enum IndexMapEntry<'a, K: Eq + std::hash::Hash + Clone, V: Clone> {
    Occupied(IndexMapOccupiedEntry<'a, K, V>),
    Vacant(IndexMapVacantEntry<'a, K, V>),
}
struct IndexMapOccupiedEntry<'a, K: Eq + std::hash::Hash + Clone, V: Clone> {
    entries: &'a mut Vec<(K, V)>,
    idx: usize,
}
struct IndexMapVacantEntry<'a, K: Eq + std::hash::Hash + Clone, V: Clone> {
    map: &'a mut IndexMap<K, V>,
    key: K,
}
impl<'a, K: Eq + std::hash::Hash + Clone, V: Clone> IndexMapEntry<'a, K, V> {
    fn or_insert(self, default: V) -> &'a mut V {
        match self {
            Self::Occupied(e) => &mut e.entries[e.idx].1,
            Self::Vacant(e) => {
                let idx = e.map.entries.len();
                e.map.index.insert(e.key.clone(), idx);
                e.map.entries.push((e.key, default));
                &mut e.map.entries[idx].1
            }
        }
    }
}
impl<K: Eq + std::hash::Hash + Clone, V: Clone> IntoIterator for IndexMap<K, V> {
    type Item = (K, V);
    type IntoIter = std::vec::IntoIter<(K, V)>;
    fn into_iter(self) -> Self::IntoIter { self.entries.into_iter() }
}
impl<K: Eq + std::hash::Hash + Clone, V: Clone> std::iter::FromIterator<(K, V)> for IndexMap<K, V> {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(iter: I) -> Self {
        let mut map = IndexMap::new();
        for (k, v) in iter { map.insert(k, v); }
        map
    }
}
impl<K: Eq + std::hash::Hash + Clone, V: Clone, const N: usize> From<[(K, V); N]> for IndexMap<K, V> {
    fn from(arr: [(K, V); N]) -> Self {
        let mut map = IndexMap::new();
        for (k, v) in arr { map.insert(k, v); }
        map
    }
}
impl<K: Eq + std::hash::Hash + Clone, V: Clone> Extend<(K, V)> for IndexMap<K, V> {
    fn extend<I: IntoIterator<Item = (K, V)>>(&mut self, iter: I) {
        for (k, v) in iter { self.insert(k, v); }
    }
}
impl<K: Eq + std::hash::Hash + Clone, V: Clone> std::ops::Index<&K> for IndexMap<K, V> {
    type Output = V;
    fn index(&self, key: &K) -> &V {
        let idx = self.index[key];
        &self.entries[idx].1
    }
}

use std::collections::HashSet;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex, OnceLock};
const __NAME__: &str = "__main__";

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
    SyntaxError(PyErrorMsg),
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
            PyError::SyntaxError(msg) => write!(f, "SyntaxError: {}", msg),
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

enum PyListGuard<'a, T> {
    Sync(std::sync::MutexGuard<'a, Vec<T>>),
    Cell(std::cell::RefMut<'a, Vec<T>>),
}
impl<'a, T> std::ops::Deref for PyListGuard<'a, T> {
    type Target = Vec<T>;
    fn deref(&self) -> &Self::Target {
        match self {
            Self::Sync(guard) => guard,
            Self::Cell(guard) => guard,
        }
    }
}
impl<'a, T> std::ops::DerefMut for PyListGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self {
            Self::Sync(guard) => guard,
            Self::Cell(guard) => guard,
        }
    }
}
trait PyListGuardExt<T> {
    fn py_list_guard(&self) -> PyListGuard<'_, T>;
}
impl<T> PyListGuardExt<T> for Arc<Mutex<Vec<T>>> {
    fn py_list_guard(&self) -> PyListGuard<'_, T> {
        PyListGuard::Sync(self.lock().expect("list mutex poisoned"))
    }
}
impl<T> PyListGuardExt<T> for Rc<RefCell<Vec<T>>> {
    fn py_list_guard(&self) -> PyListGuard<'_, T> {
        PyListGuard::Cell(self.borrow_mut())
    }
}
enum PyDictGuard<'a, K: Eq + std::hash::Hash + Clone, V: Clone> {
    Sync(std::sync::MutexGuard<'a, IndexMap<K, V>>),
}
impl<'a, K: Eq + std::hash::Hash + Clone, V: Clone> std::ops::Deref for PyDictGuard<'a, K, V> {
    type Target = IndexMap<K, V>;
    fn deref(&self) -> &Self::Target {
        match self {
            Self::Sync(guard) => guard,
        }
    }
}
impl<'a, K: Eq + std::hash::Hash + Clone, V: Clone> std::ops::DerefMut for PyDictGuard<'a, K, V> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self {
            Self::Sync(guard) => guard,
        }
    }
}
trait PyDictGuardExt<K: Eq + std::hash::Hash + Clone, V: Clone> {
    fn py_dict_guard(&self) -> PyDictGuard<'_, K, V>;
}
impl<K: Eq + std::hash::Hash + Clone, V: Clone> PyDictGuardExt<K, V> for Arc<Mutex<IndexMap<K, V>>> {
    fn py_dict_guard(&self) -> PyDictGuard<'_, K, V> {
        PyDictGuard::Sync(self.lock().expect("dict mutex poisoned"))
    }
}
fn py_print<T: std::fmt::Display + ?Sized>(v: &T) {
    println!("{v}");
}
fn py_int(value: impl std::borrow::Borrow<i64>) -> i64 {
    *value.borrow()
}
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct PyRepr(String);
impl std::fmt::Debug for PyRepr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
trait PyLen {
    fn py_len(&self) -> i64;
}
impl<T> PyLen for [T] {
    fn py_len(&self) -> i64 { self.len() as i64 }
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
fn py_len<T: PyLen + ?Sized>(v: &T) -> i64 { v.py_len() }
impl<K: Eq + std::hash::Hash + Clone, V: Clone> PyLen for IndexMap<K, V> {
    fn py_len(&self) -> i64 { self.len() as i64 }
}
impl<K: Eq + std::hash::Hash + Clone, V: Clone> PyLen for Arc<Mutex<IndexMap<K, V>>> {
    fn py_len(&self) -> i64 { self.lock().expect("dict mutex poisoned").len() as i64 }
}
fn py_range(end: i64) -> std::ops::Range<i64> { 0..end }
fn py_range2(start: i64, end: i64) -> std::ops::Range<i64> { start..end }
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
fn py_next<T>(value: Option<T>) -> Result<T, PyError> {
    value.ok_or_else(|| PyError::StopIteration(String::new().into()))
}
fn py_dict_get<K: Eq + std::hash::Hash + Clone, V: Clone>(map: &IndexMap<K, V>, key: &K) -> Result<V, PyError> {
    map.get(key).cloned().ok_or_else(|| PyError::KeyError("KeyError".into()))
}
fn py_list_get<T: Clone>(items: &[T], idx: i64) -> Result<T, PyError> {
    let len = items.len() as i64;
    let adj = if idx < 0 { len + idx } else { idx };
    if adj < 0 || adj >= len {
        Err(PyError::IndexError("IndexError".into()))
    } else {
        Ok(items[adj as usize].clone())
    }
}
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

#[derive(Debug, Clone, PartialEq)]
pub enum PyUnionIntStr {
    Int(i64),
    Str(String),
}

impl std::fmt::Display for PyUnionIntStr {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            PyUnionIntStr::Int(v) => write!(f, "{}" , v),
            PyUnionIntStr::Str(v) => write!(f, "{}" , v),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum PyUnionBoolIntStr {
    Bool(bool),
    Int(i64),
    Str(String),
}

impl std::fmt::Display for PyUnionBoolIntStr {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            PyUnionBoolIntStr::Bool(v) => write!(f, "{}" , v),
            PyUnionBoolIntStr::Int(v) => write!(f, "{}" , v),
            PyUnionBoolIntStr::Str(v) => write!(f, "{}" , v),
        }
    }
}

pub fn make_squares(n: i64) -> Vec<i64> {
    return { let mut _tmp0 = Vec::new(); for i in py_range(n) { _tmp0.push(py_int(i).checked_mul(py_int(i)).ok_or_else(|| PyError::OverflowError("integer overflow".into())).unwrap_or_else(|e| panic!("Unhandled exception: {}", e))); } _tmp0 };
}

pub fn make_sq_dict(n: i64) -> Arc<Mutex<IndexMap<i64, i64>>> {
    return { let _tmp3 = { let mut _tmp2 = Vec::new(); for i in py_range(n) { _tmp2.push((i, py_int(i).checked_mul(py_int(i)).ok_or_else(|| PyError::OverflowError("integer overflow".into())).unwrap_or_else(|e| panic!("Unhandled exception: {}", e)))); } Arc::new(Mutex::new(_tmp2)) }.clone(); let _tmp4 = _tmp3.py_list_guard(); Arc::new(Mutex::new((_tmp4.iter().cloned()).collect::<IndexMap<_, _>>())) };
}

pub fn str_len(s: String) -> i64 {
    return py_len(&s);
}

pub fn negate(x: i64) -> i64 {
    return (-x);
}

pub fn myabs(x: i64) -> i64 {
    if (x < 0i64) {
        return (-x);
    }
    return x;
}

pub fn first_char(s: String) -> String {
    return py_str_get(&s, 0i64).unwrap_or_else(|e| panic!("Unhandled exception: {}", e));
}

pub fn get_len(s: String) -> i64 {
    return py_len(&s);
}

fn main() {
    let _result = (|| -> Result<(), PyError> {
        let squares: Vec<i64> = { let mut _tmp5 = Vec::new(); for x in py_range(5i64) { _tmp5.push((x * x)); } _tmp5 };
        if !((py_len(&squares) == 5i64)) { return Err(PyError::AssertionError(("len(squares) should equal 5".to_string()).into())); }
        if !(((py_list_get(&squares, 0i64)?) == 0i64)) { return Err(PyError::AssertionError(("squares[0] should equal 0".to_string()).into())); }
        if !(((py_list_get(&squares, 1i64)?) == 1i64)) { return Err(PyError::AssertionError(("squares[1] should equal 1".to_string()).into())); }
        if !(((py_list_get(&squares, 4i64)?) == 16i64)) { return Err(PyError::AssertionError(("squares[4] should equal 16".to_string()).into())); }
        let evens: Vec<i64> = { let mut _tmp6 = Vec::new(); for x in py_range(10i64) { if ({ let _tmp7 = x; let _tmp8 = 2i64; let _tmp9 = _tmp7 % _tmp8; ((_tmp9 + _tmp8) % _tmp8) } == 0i64) { _tmp6.push(x); } } _tmp6 };
        if !((py_len(&evens) == 5i64)) { return Err(PyError::AssertionError(("len(evens) should equal 5".to_string()).into())); }
        if !(((py_list_get(&evens, 0i64)?) == 0i64)) { return Err(PyError::AssertionError(("evens[0] should equal 0".to_string()).into())); }
        if !(((py_list_get(&evens, 2i64)?) == 4i64)) { return Err(PyError::AssertionError(("evens[2] should equal 4".to_string()).into())); }
        if !(((py_list_get(&evens, 4i64)?) == 8i64)) { return Err(PyError::AssertionError(("evens[4] should equal 8".to_string()).into())); }
        let pairs: Vec<i64> = { let mut _tmp10 = Vec::new(); for x in py_range(3i64) { for y in py_range(2i64) { _tmp10.push((x + y)); } } _tmp10 };
        if !((py_len(&pairs) == 6i64)) { return Err(PyError::AssertionError(("len(pairs) should equal 6".to_string()).into())); }
        if !(((py_list_get(&pairs, 0i64)?) == 0i64)) { return Err(PyError::AssertionError(("pairs[0] should equal 0".to_string()).into())); }
        if !(((py_list_get(&pairs, 1i64)?) == 1i64)) { return Err(PyError::AssertionError(("pairs[1] should equal 1".to_string()).into())); }
        if !(((py_list_get(&pairs, 2i64)?) == 1i64)) { return Err(PyError::AssertionError(("pairs[2] should equal 1".to_string()).into())); }
        if !(((py_list_get(&pairs, 3i64)?) == 2i64)) { return Err(PyError::AssertionError(("pairs[3] should equal 2".to_string()).into())); }
        if !(((py_list_get(&pairs, 4i64)?) == 2i64)) { return Err(PyError::AssertionError(("pairs[4] should equal 2".to_string()).into())); }
        if !(((py_list_get(&pairs, 5i64)?) == 3i64)) { return Err(PyError::AssertionError(("pairs[5] should equal 3".to_string()).into())); }
        let doubled: Vec<i64> = { let mut _tmp11 = Vec::new(); for x in vec![1i64, 2i64, 3i64].iter().copied() { _tmp11.push((x * 2i64)); } _tmp11 };
        if !((py_len(&doubled) == 3i64)) { return Err(PyError::AssertionError(("len(doubled) should equal 3".to_string()).into())); }
        if !(((py_list_get(&doubled, 0i64)?) == 2i64)) { return Err(PyError::AssertionError(("doubled[0] should equal 2".to_string()).into())); }
        if !(((py_list_get(&doubled, 1i64)?) == 4i64)) { return Err(PyError::AssertionError(("doubled[1] should equal 4".to_string()).into())); }
        if !(((py_list_get(&doubled, 2i64)?) == 6i64)) { return Err(PyError::AssertionError(("doubled[2] should equal 6".to_string()).into())); }
        let chars: Vec<String> = { let mut _tmp12 = Vec::new(); for c in "abc".to_string().chars().map(|c| c.to_string()).collect::<Vec<_>>().into_iter() { _tmp12.push(c); } _tmp12 };
        if !((py_len(&chars) == 3i64)) { return Err(PyError::AssertionError(("len(chars) should equal 3".to_string()).into())); }
        if !(((py_list_get(&chars, 0i64)?) == "a".to_string())) { return Err(PyError::AssertionError(("chars[0] should equal \"a\"".to_string()).into())); }
        if !(((py_list_get(&chars, 1i64)?) == "b".to_string())) { return Err(PyError::AssertionError(("chars[1] should equal \"b\"".to_string()).into())); }
        if !(((py_list_get(&chars, 2i64)?) == "c".to_string())) { return Err(PyError::AssertionError(("chars[2] should equal \"c\"".to_string()).into())); }
        let transformed: Vec<String> = { let mut _tmp13 = Vec::new(); for x in py_range(3i64) { _tmp13.push(x.to_string()); } _tmp13 };
        if !((py_len(&transformed) == 3i64)) { return Err(PyError::AssertionError(("len(transformed) should equal 3".to_string()).into())); }
        if !(((py_list_get(&transformed, 0i64)?) == "0".to_string())) { return Err(PyError::AssertionError(("transformed[0] should equal \"0\"".to_string()).into())); }
        if !(((py_list_get(&transformed, 1i64)?) == "1".to_string())) { return Err(PyError::AssertionError(("transformed[1] should equal \"1\"".to_string()).into())); }
        if !(((py_list_get(&transformed, 2i64)?) == "2".to_string())) { return Err(PyError::AssertionError(("transformed[2] should equal \"2\"".to_string()).into())); }
        let result: Arc<Mutex<Vec<i64>>> = Arc::new(Mutex::new(make_squares(4i64)));
        if !((py_len(&result) == 4i64)) { return Err(PyError::AssertionError(("len(result) should equal 4".to_string()).into())); }
        if !((({ let _tmp14 = 3i64; py_list_get(&result.py_list_guard(), _tmp14) }?) == 9i64)) { return Err(PyError::AssertionError(("result[3] should equal 9".to_string()).into())); }
        let flag: bool = true;
        if flag {
            let inner_comp: Vec<i64> = { let mut _tmp15 = Vec::new(); for x in py_range(3i64) { _tmp15.push(x); } _tmp15 };
            if !((py_len(&inner_comp) == 3i64)) { return Err(PyError::AssertionError(("len(inner_comp) should equal 3".to_string()).into())); }
        }
        let mut total: i64 = 0i64;
        {
            for i in py_range(3i64) {
                let comp: Vec<i64> = { let mut _tmp16 = Vec::new(); for x in py_range((i + 1i64)) { _tmp16.push(x); } _tmp16 };
                total = (total + py_len(&comp));
            }
        }
        if !((total == 6i64)) { return Err(PyError::AssertionError(("total should equal 6".to_string()).into())); }
        let first: Vec<i64> = { let mut _tmp17 = Vec::new(); for a in py_range(2i64) { _tmp17.push(a); } _tmp17 };
        let second: Vec<i64> = { let mut _tmp18 = Vec::new(); for b in py_range(3i64) { _tmp18.push(b); } _tmp18 };
        if !((py_len(&first) == 2i64)) { return Err(PyError::AssertionError(("len(first) should equal 2".to_string()).into())); }
        if !((py_len(&second) == 3i64)) { return Err(PyError::AssertionError(("len(second) should equal 3".to_string()).into())); }
        let multi_filter: Vec<i64> = { let mut _tmp19 = Vec::new(); for x in py_range(20i64) { if ({ let _tmp20 = x; let _tmp21 = 2i64; let _tmp22 = _tmp20 % _tmp21; ((_tmp22 + _tmp21) % _tmp21) } == 0i64) && ({ let _tmp23 = x; let _tmp24 = 3i64; let _tmp25 = _tmp23 % _tmp24; ((_tmp25 + _tmp24) % _tmp24) } == 0i64) { _tmp19.push(x); } } _tmp19 };
        if !((py_len(&multi_filter) == 4i64)) { return Err(PyError::AssertionError(("len(multi_filter) should equal 4".to_string()).into())); }
        if !(((py_list_get(&multi_filter, 0i64)?) == 0i64)) { return Err(PyError::AssertionError(("multi_filter[0] should equal 0".to_string()).into())); }
        if !(((py_list_get(&multi_filter, 1i64)?) == 6i64)) { return Err(PyError::AssertionError(("multi_filter[1] should equal 6".to_string()).into())); }
        if !(((py_list_get(&multi_filter, 2i64)?) == 12i64)) { return Err(PyError::AssertionError(("multi_filter[2] should equal 12".to_string()).into())); }
        if !(((py_list_get(&multi_filter, 3i64)?) == 18i64)) { return Err(PyError::AssertionError(("multi_filter[3] should equal 18".to_string()).into())); }
        let sq_dict: IndexMap<i64, i64> = { let _tmp27 = { let mut _tmp26 = Vec::new(); for x in py_range(4i64) { _tmp26.push((x, (x * x))); } Arc::new(Mutex::new(_tmp26)) }.clone(); let _tmp28 = _tmp27.py_list_guard(); (_tmp28.iter().cloned()).collect::<IndexMap<_, _>>() };
        if !((py_len(&sq_dict) == 4i64)) { return Err(PyError::AssertionError(("len(sq_dict) should equal 4".to_string()).into())); }
        if !(((py_dict_get(&sq_dict, &0i64)?) == 0i64)) { return Err(PyError::AssertionError(("sq_dict[0] should equal 0".to_string()).into())); }
        if !(((py_dict_get(&sq_dict, &1i64)?) == 1i64)) { return Err(PyError::AssertionError(("sq_dict[1] should equal 1".to_string()).into())); }
        if !(((py_dict_get(&sq_dict, &2i64)?) == 4i64)) { return Err(PyError::AssertionError(("sq_dict[2] should equal 4".to_string()).into())); }
        if !(((py_dict_get(&sq_dict, &3i64)?) == 9i64)) { return Err(PyError::AssertionError(("sq_dict[3] should equal 9".to_string()).into())); }
        let filtered: IndexMap<i64, i64> = { let _tmp30 = { let mut _tmp29 = Vec::new(); for x in py_range(5i64) { if (x > 1i64) { _tmp29.push((x, (x * 2i64))); } } Arc::new(Mutex::new(_tmp29)) }.clone(); let _tmp31 = _tmp30.py_list_guard(); (_tmp31.iter().cloned()).collect::<IndexMap<_, _>>() };
        if !((py_len(&filtered) == 3i64)) { return Err(PyError::AssertionError(("len(filtered) should equal 3".to_string()).into())); }
        if !(!(filtered.contains_key(&0i64))) { return Err(PyError::AssertionError(("0 not should be in filtered".to_string()).into())); }
        if !(!(filtered.contains_key(&1i64))) { return Err(PyError::AssertionError(("1 not should be in filtered".to_string()).into())); }
        if !(((py_dict_get(&filtered, &2i64)?) == 4i64)) { return Err(PyError::AssertionError(("filtered[2] should equal 4".to_string()).into())); }
        if !(((py_dict_get(&filtered, &3i64)?) == 6i64)) { return Err(PyError::AssertionError(("filtered[3] should equal 6".to_string()).into())); }
        if !(((py_dict_get(&filtered, &4i64)?) == 8i64)) { return Err(PyError::AssertionError(("filtered[4] should equal 8".to_string()).into())); }
        let doubled_dict: IndexMap<i64, i64> = { let _tmp33 = { let mut _tmp32 = Vec::new(); for i in py_range(3i64) { _tmp32.push((i, (i * 2i64))); } Arc::new(Mutex::new(_tmp32)) }.clone(); let _tmp34 = _tmp33.py_list_guard(); (_tmp34.iter().cloned()).collect::<IndexMap<_, _>>() };
        if !(((py_dict_get(&doubled_dict, &0i64)?) == 0i64)) { return Err(PyError::AssertionError(("doubled_dict[0] should equal 0".to_string()).into())); }
        if !(((py_dict_get(&doubled_dict, &1i64)?) == 2i64)) { return Err(PyError::AssertionError(("doubled_dict[1] should equal 2".to_string()).into())); }
        if !(((py_dict_get(&doubled_dict, &2i64)?) == 4i64)) { return Err(PyError::AssertionError(("doubled_dict[2] should equal 4".to_string()).into())); }
        let sq_result: Arc<Mutex<IndexMap<i64, i64>>> = make_sq_dict(3i64);
        if !((py_len(&sq_result) == 3i64)) { return Err(PyError::AssertionError(("len(sq_result) should equal 3".to_string()).into())); }
        if !((({ let _tmp35 = sq_result.py_dict_guard(); py_dict_get(&_tmp35, &2i64) }?) == 4i64)) { return Err(PyError::AssertionError(("sq_result[2] should equal 4".to_string()).into())); }
        let nums: Arc<Mutex<Vec<i64>>> = Arc::new(Mutex::new(vec![10i64, 20i64, 30i64]));
        let mut it = { let _tmp37 = nums.clone(); let mut _tmp38: usize = 0; std::iter::from_fn(move || { let _tmp39 = _tmp37.py_list_guard(); if _tmp38 < _tmp39.len() { let item = _tmp39[_tmp38]; _tmp38 += 1; Some(item) } else { None } }) };
        if !(((py_next(it.next())?) == 10i64)) { return Err(PyError::AssertionError(("next(it) should equal 10".to_string()).into())); }
        if !(((py_next(it.next())?) == 20i64)) { return Err(PyError::AssertionError(("next(it) should equal 20".to_string()).into())); }
        if !(((py_next(it.next())?) == 30i64)) { return Err(PyError::AssertionError(("next(it) should equal 30".to_string()).into())); }
        let _try_result = (|| -> Result<(), PyError> {
            (py_next(it.next())?);
            if !(false) { return Err(PyError::AssertionError(("should have raised StopIteration".to_string()).into())); }
            Ok(())
        })();
        match _try_result {
            Ok(_) => {
            }
            Err(_e) => {
                ();
            }
        }
        let mut s = "AB".to_string().chars().map(|c| c.to_string()).collect::<Vec<_>>().into_iter();
        if !(((py_next(s.next())?) == "A".to_string())) { return Err(PyError::AssertionError(("next(s) should equal \"A\"".to_string()).into())); }
        if !(((py_next(s.next())?) == "B".to_string())) { return Err(PyError::AssertionError(("next(s) should equal \"B\"".to_string()).into())); }
        let _try_result = (|| -> Result<(), PyError> {
            (py_next(s.next())?);
            if !(false) { return Err(PyError::AssertionError(("should have raised StopIteration".to_string()).into())); }
            Ok(())
        })();
        match _try_result {
            Ok(_) => {
            }
            Err(_e) => {
                ();
            }
        }
        let mut t = { let _tmp40 = (1i64, 2i64); vec![_tmp40.0, _tmp40.1].into_iter() };
        if !(((py_next(t.next())?) == 1i64)) { return Err(PyError::AssertionError(("next(t) should equal 1".to_string()).into())); }
        if !(((py_next(t.next())?) == 2i64)) { return Err(PyError::AssertionError(("next(t) should equal 2".to_string()).into())); }
        let d: Arc<Mutex<IndexMap<String, i64>>> = Arc::new(Mutex::new({ let mut _tmp41 = IndexMap::new(); _tmp41.insert("a".to_string(), 1i64); _tmp41.insert("b".to_string(), 2i64); _tmp41 }));
        let mut di = { let _tmp42 = d.clone(); let _tmp43 = _tmp42.py_dict_guard(); let _tmp44 = _tmp43.keys().cloned().collect::<Vec<_>>(); _tmp44.into_iter() };
        let k1 = (py_next(di.next())?);
        let k2 = (py_next(di.next())?);
        if !(((k1 == "a".to_string()) || (k1 == "b".to_string()))) { return Err(PyError::AssertionError(("k1 should equal \"a\" or k1 == \"b\"".to_string()).into())); }
        let mut ri = py_range(3i64);
        if !(((py_next(ri.next())?) == 0i64)) { return Err(PyError::AssertionError(("next(ri) should equal 0".to_string()).into())); }
        if !(((py_next(ri.next())?) == 1i64)) { return Err(PyError::AssertionError(("next(ri) should equal 1".to_string()).into())); }
        if !(((py_next(ri.next())?) == 2i64)) { return Err(PyError::AssertionError(("next(ri) should equal 2".to_string()).into())); }
        let _try_result = (|| -> Result<(), PyError> {
            (py_next(ri.next())?);
            if !(false) { return Err(PyError::AssertionError(("should have raised StopIteration".to_string()).into())); }
            Ok(())
        })();
        match _try_result {
            Ok(_) => {
            }
            Err(_e) => {
                ();
            }
        }
        let mut ri2 = py_range2(5i64, 8i64);
        if !(((py_next(ri2.next())?) == 5i64)) { return Err(PyError::AssertionError(("next(ri2) should equal 5".to_string()).into())); }
        if !(((py_next(ri2.next())?) == 6i64)) { return Err(PyError::AssertionError(("next(ri2) should equal 6".to_string()).into())); }
        if !(((py_next(ri2.next())?) == 7i64)) { return Err(PyError::AssertionError(("next(ri2) should equal 7".to_string()).into())); }
        let mut ri3 = (py_range3(3i64, 0i64, (-1i64))?);
        if !(((py_next(ri3.next())?) == 3i64)) { return Err(PyError::AssertionError(("next(ri3) should equal 3".to_string()).into())); }
        if !(((py_next(ri3.next())?) == 2i64)) { return Err(PyError::AssertionError(("next(ri3) should equal 2".to_string()).into())); }
        if !(((py_next(ri3.next())?) == 1i64)) { return Err(PyError::AssertionError(("next(ri3) should equal 1".to_string()).into())); }
        let mut result_list: Vec<i64> = Vec::<i64>::new();
        {
            for i in vec![1i64, 2i64, 3i64].iter().copied() {
                result_list.push(i);
            }
        }
        if !({ let _left = &(result_list); let _right = &(vec![1i64, 2i64, 3i64]); _left.iter().eq(_right.iter()) }) { return Err(PyError::AssertionError(("for-loop over list failed".to_string()).into())); }
        let mut result_bytes: Vec<i64> = Vec::<i64>::new();
        {
            for b in vec![104i64, 101i64, 108i64, 108i64, 111i64].into_iter() {
                result_bytes.push(b);
            }
        }
        if !({ let _left = &(result_bytes); let _right = &(vec![104i64, 101i64, 108i64, 108i64, 111i64]); _left.iter().eq(_right.iter()) }) { return Err(PyError::AssertionError(("for-loop over bytes failed".to_string()).into())); }
        let mut result_string: Vec<String> = Vec::<String>::new();
        {
            for c in "hello".to_string().chars().map(|c| c.to_string()).collect::<Vec<_>>().into_iter() {
                result_string.push(c);
            }
        }
        if !({ let _left = &(result_string); let _right = &(vec!["h".to_string(), "e".to_string(), "l".to_string(), "l".to_string(), "o".to_string()]); _left.iter().eq(_right.iter()) }) { return Err(PyError::AssertionError(("for-loop over string failed".to_string()).into())); }
        let mut result_range: Vec<i64> = Vec::<i64>::new();
        {
            for i in py_range(3i64) {
                result_range.push(i);
            }
        }
        if !({ let _left = &(result_range); let _right = &(vec![0i64, 1i64, 2i64]); _left.iter().eq(_right.iter()) }) { return Err(PyError::AssertionError(("for-loop over range failed".to_string()).into())); }
        py_print(&("Testing reversed list..."));
        let rev_nums: Vec<i64> = vec![10i64, 20i64, 30i64];
        let mut rev_it = { let _tmp46 = &rev_nums; let mut _tmp45: usize = _tmp46.len(); std::iter::from_fn(move || { if _tmp45 == 0 { None } else { _tmp45 -= 1; Some(_tmp46[_tmp45]) } }) };
        let v1 = (py_next(rev_it.next())?);
        if !((v1 == 30i64)) { return Err(PyError::AssertionError(("v1 should equal 30".to_string()).into())); }
        let v2 = (py_next(rev_it.next())?);
        if !((v2 == 20i64)) { return Err(PyError::AssertionError(("v2 should equal 20".to_string()).into())); }
        let v3 = (py_next(rev_it.next())?);
        if !((v3 == 10i64)) { return Err(PyError::AssertionError(("v3 should equal 10".to_string()).into())); }
        py_print(&("Reversed list: OK"));
        let _try_result = (|| -> Result<(), PyError> {
            (py_next(rev_it.next())?);
            if !(false) { return Err(PyError::AssertionError(("should have raised StopIteration".to_string()).into())); }
            Ok(())
        })();
        match _try_result {
            Ok(_) => {
            }
            Err(_e) => {
                ();
            }
        }
        py_print(&("Testing reversed tuple..."));
        let rev_t: (i64, i64, i64) = (1i64, 2i64, 3i64);
        let mut ti = ({ let _tmp47 = &rev_t; vec![_tmp47.0, _tmp47.1, _tmp47.2].into_iter() }).rev();
        let tv1 = (py_next(ti.next())?);
        if !((tv1 == 3i64)) { return Err(PyError::AssertionError(("tv1 should equal 3".to_string()).into())); }
        let tv2 = (py_next(ti.next())?);
        if !((tv2 == 2i64)) { return Err(PyError::AssertionError(("tv2 should equal 2".to_string()).into())); }
        let tv3 = (py_next(ti.next())?);
        if !((tv3 == 1i64)) { return Err(PyError::AssertionError(("tv3 should equal 1".to_string()).into())); }
        py_print(&("Reversed tuple: OK"));
        py_print(&("Testing reversed string..."));
        let mut rev_s = ("ABC".to_string().chars().map(|c| c.to_string()).collect::<Vec<_>>().into_iter()).rev();
        let sc1 = (py_next(rev_s.next())?);
        if !((sc1 == "C".to_string())) { return Err(PyError::AssertionError(("sc1 should equal \"C\"".to_string()).into())); }
        let sc2 = (py_next(rev_s.next())?);
        if !((sc2 == "B".to_string())) { return Err(PyError::AssertionError(("sc2 should equal \"B\"".to_string()).into())); }
        let sc3 = (py_next(rev_s.next())?);
        if !((sc3 == "A".to_string())) { return Err(PyError::AssertionError(("sc3 should equal \"A\"".to_string()).into())); }
        py_print(&("Reversed string: OK"));
        py_print(&("Testing reversed range(5)..."));
        let mut rev_ri = (py_range(5i64)).rev();
        let r1 = (py_next(rev_ri.next())?);
        if !((r1 == 4i64)) { return Err(PyError::AssertionError(("r1 should equal 4".to_string()).into())); }
        let r2 = (py_next(rev_ri.next())?);
        if !((r2 == 3i64)) { return Err(PyError::AssertionError(("r2 should equal 3".to_string()).into())); }
        let r3 = (py_next(rev_ri.next())?);
        if !((r3 == 2i64)) { return Err(PyError::AssertionError(("r3 should equal 2".to_string()).into())); }
        let r4 = (py_next(rev_ri.next())?);
        if !((r4 == 1i64)) { return Err(PyError::AssertionError(("r4 should equal 1".to_string()).into())); }
        let r5 = (py_next(rev_ri.next())?);
        if !((r5 == 0i64)) { return Err(PyError::AssertionError(("r5 should equal 0".to_string()).into())); }
        py_print(&("Reversed range(5): OK"));
        py_print(&("Testing reversed range(2, 6)..."));
        let mut rev_ri2 = (py_range2(2i64, 6i64)).rev();
        let rr1 = (py_next(rev_ri2.next())?);
        if !((rr1 == 5i64)) { return Err(PyError::AssertionError(("rr1 should equal 5".to_string()).into())); }
        let rr2 = (py_next(rev_ri2.next())?);
        if !((rr2 == 4i64)) { return Err(PyError::AssertionError(("rr2 should equal 4".to_string()).into())); }
        let rr3 = (py_next(rev_ri2.next())?);
        if !((rr3 == 3i64)) { return Err(PyError::AssertionError(("rr3 should equal 3".to_string()).into())); }
        let rr4 = (py_next(rev_ri2.next())?);
        if !((rr4 == 2i64)) { return Err(PyError::AssertionError(("rr4 should equal 2".to_string()).into())); }
        py_print(&("Reversed range(2,6): OK"));
        py_print(&("Testing reversed range(0, 10, 2)..."));
        let mut rev_ri3 = ((py_range3(0i64, 10i64, 2i64)?)).collect::<Vec<_>>().into_iter().rev();
        let rs1 = (py_next(rev_ri3.next())?);
        if !((rs1 == 8i64)) { return Err(PyError::AssertionError(("rs1 should equal 8".to_string()).into())); }
        let rs2 = (py_next(rev_ri3.next())?);
        if !((rs2 == 6i64)) { return Err(PyError::AssertionError(("rs2 should equal 6".to_string()).into())); }
        let rs3 = (py_next(rev_ri3.next())?);
        if !((rs3 == 4i64)) { return Err(PyError::AssertionError(("rs3 should equal 4".to_string()).into())); }
        let rs4 = (py_next(rev_ri3.next())?);
        if !((rs4 == 2i64)) { return Err(PyError::AssertionError(("rs4 should equal 2".to_string()).into())); }
        let rs5 = (py_next(rev_ri3.next())?);
        if !((rs5 == 0i64)) { return Err(PyError::AssertionError(("rs5 should equal 0".to_string()).into())); }
        py_print(&("Reversed range with step: OK"));
        py_print(&("Testing reversed dict..."));
        let rev_d: Arc<Mutex<IndexMap<String, i64>>> = Arc::new(Mutex::new({ let mut _tmp48 = IndexMap::new(); _tmp48.insert("a".to_string(), 1i64); _tmp48.insert("b".to_string(), 2i64); _tmp48.insert("c".to_string(), 3i64); _tmp48 }));
        let mut forward_it = { let _tmp49 = rev_d.clone(); let _tmp50 = _tmp49.py_dict_guard(); let _tmp51 = _tmp50.keys().cloned().collect::<Vec<_>>(); _tmp51.into_iter() };
        let fk1 = (py_next(forward_it.next())?);
        let fk2 = (py_next(forward_it.next())?);
        let fk3 = (py_next(forward_it.next())?);
        if !((fk1 == "a".to_string())) { return Err(PyError::AssertionError(("fk1 should equal 'a'".to_string()).into())); }
        if !((fk2 == "b".to_string())) { return Err(PyError::AssertionError(("fk2 should equal 'b'".to_string()).into())); }
        if !((fk3 == "c".to_string())) { return Err(PyError::AssertionError(("fk3 should equal 'c'".to_string()).into())); }
        let mut rev_di = ({ let _tmp52 = rev_d.clone(); let _tmp53 = _tmp52.py_dict_guard(); let _tmp54 = _tmp53.keys().cloned().collect::<Vec<_>>(); _tmp54.into_iter() }).rev();
        let dk1 = (py_next(rev_di.next())?);
        let dk2 = (py_next(rev_di.next())?);
        let dk3 = (py_next(rev_di.next())?);
        if !((dk1 == fk3)) { return Err(PyError::AssertionError(("dk1 should equal fk3".to_string()).into())); }
        if !((dk2 == fk2)) { return Err(PyError::AssertionError(("dk2 should equal fk2".to_string()).into())); }
        if !((dk3 == fk1)) { return Err(PyError::AssertionError(("dk3 should equal fk1".to_string()).into())); }
        py_print(&("Reversed dict: OK"));
        let sorted_nums: Arc<Mutex<Vec<i64>>> = Arc::new(Mutex::new(vec![3i64, 1i64, 4i64, 1i64, 5i64, 9i64, 2i64, 6i64]));
        let sorted_list: Arc<Mutex<Vec<i64>>> = { let _tmp55 = sorted_nums.clone(); let _tmp56 = _tmp55.py_list_guard(); { let mut _tmp57 = (_tmp56.iter().copied()).collect::<Vec<_>>(); _tmp57.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)); if false { _tmp57.reverse(); } Arc::new(Mutex::new(_tmp57)) } };
        if !((({ let _tmp58 = 0i64; py_list_get(&sorted_list.py_list_guard(), _tmp58) }?) == 1i64)) { return Err(PyError::AssertionError(("sorted_list[0] should equal 1".to_string()).into())); }
        if !((({ let _tmp59 = 1i64; py_list_get(&sorted_list.py_list_guard(), _tmp59) }?) == 1i64)) { return Err(PyError::AssertionError(("sorted_list[1] should equal 1".to_string()).into())); }
        if !((({ let _tmp60 = 2i64; py_list_get(&sorted_list.py_list_guard(), _tmp60) }?) == 2i64)) { return Err(PyError::AssertionError(("sorted_list[2] should equal 2".to_string()).into())); }
        if !((({ let _tmp61 = 3i64; py_list_get(&sorted_list.py_list_guard(), _tmp61) }?) == 3i64)) { return Err(PyError::AssertionError(("sorted_list[3] should equal 3".to_string()).into())); }
        if !((({ let _tmp62 = 4i64; py_list_get(&sorted_list.py_list_guard(), _tmp62) }?) == 4i64)) { return Err(PyError::AssertionError(("sorted_list[4] should equal 4".to_string()).into())); }
        if !((({ let _tmp63 = 5i64; py_list_get(&sorted_list.py_list_guard(), _tmp63) }?) == 5i64)) { return Err(PyError::AssertionError(("sorted_list[5] should equal 5".to_string()).into())); }
        if !((({ let _tmp64 = 6i64; py_list_get(&sorted_list.py_list_guard(), _tmp64) }?) == 6i64)) { return Err(PyError::AssertionError(("sorted_list[6] should equal 6".to_string()).into())); }
        if !((({ let _tmp65 = 7i64; py_list_get(&sorted_list.py_list_guard(), _tmp65) }?) == 9i64)) { return Err(PyError::AssertionError(("sorted_list[7] should equal 9".to_string()).into())); }
        let words: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(vec!["banana".to_string(), "apple".to_string(), "cherry".to_string()]));
        let sorted_words: Arc<Mutex<Vec<String>>> = { let _tmp66 = words.clone(); let _tmp67 = _tmp66.py_list_guard(); { let mut _tmp68 = (_tmp67.iter().cloned()).collect::<Vec<_>>(); _tmp68.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)); if false { _tmp68.reverse(); } Arc::new(Mutex::new(_tmp68)) } };
        if !((({ let _tmp69 = 0i64; py_list_get(&sorted_words.py_list_guard(), _tmp69) }?) == "apple".to_string())) { return Err(PyError::AssertionError(("sorted_words[0] should equal \"apple\"".to_string()).into())); }
        if !((({ let _tmp70 = 1i64; py_list_get(&sorted_words.py_list_guard(), _tmp70) }?) == "banana".to_string())) { return Err(PyError::AssertionError(("sorted_words[1] should equal \"banana\"".to_string()).into())); }
        if !((({ let _tmp71 = 2i64; py_list_get(&sorted_words.py_list_guard(), _tmp71) }?) == "cherry".to_string())) { return Err(PyError::AssertionError(("sorted_words[2] should equal \"cherry\"".to_string()).into())); }
        let tup: (i64, i64, i64) = (3i64, 1i64, 2i64);
        let sorted_tup: Arc<Mutex<Vec<i64>>> = { let mut _tmp73 = ({ let _tmp72 = &tup; vec![_tmp72.0, _tmp72.1, _tmp72.2].into_iter() }).collect::<Vec<_>>(); _tmp73.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)); if false { _tmp73.reverse(); } Arc::new(Mutex::new(_tmp73)) };
        if !((({ let _tmp74 = 0i64; py_list_get(&sorted_tup.py_list_guard(), _tmp74) }?) == 1i64)) { return Err(PyError::AssertionError(("sorted_tup[0] should equal 1".to_string()).into())); }
        if !((({ let _tmp75 = 1i64; py_list_get(&sorted_tup.py_list_guard(), _tmp75) }?) == 2i64)) { return Err(PyError::AssertionError(("sorted_tup[1] should equal 2".to_string()).into())); }
        if !((({ let _tmp76 = 2i64; py_list_get(&sorted_tup.py_list_guard(), _tmp76) }?) == 3i64)) { return Err(PyError::AssertionError(("sorted_tup[2] should equal 3".to_string()).into())); }
        let str_s: String = "cba".to_string();
        let sorted_str: Arc<Mutex<Vec<String>>> = { let mut _tmp77 = (str_s.chars().map(|c| c.to_string())).collect::<Vec<_>>(); _tmp77.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)); if false { _tmp77.reverse(); } Arc::new(Mutex::new(_tmp77)) };
        if !((({ let _tmp78 = 0i64; py_list_get(&sorted_str.py_list_guard(), _tmp78) }?) == "a".to_string())) { return Err(PyError::AssertionError(("sorted_str[0] should equal \"a\"".to_string()).into())); }
        if !((({ let _tmp79 = 1i64; py_list_get(&sorted_str.py_list_guard(), _tmp79) }?) == "b".to_string())) { return Err(PyError::AssertionError(("sorted_str[1] should equal \"b\"".to_string()).into())); }
        if !((({ let _tmp80 = 2i64; py_list_get(&sorted_str.py_list_guard(), _tmp80) }?) == "c".to_string())) { return Err(PyError::AssertionError(("sorted_str[2] should equal \"c\"".to_string()).into())); }
        let sorted_d: Arc<Mutex<IndexMap<String, i64>>> = Arc::new(Mutex::new({ let mut _tmp81 = IndexMap::new(); _tmp81.insert("c".to_string(), 3i64); _tmp81.insert("a".to_string(), 1i64); _tmp81.insert("b".to_string(), 2i64); _tmp81 }));
        let sorted_keys: Arc<Mutex<Vec<String>>> = { let _tmp82 = sorted_d.clone(); let _tmp83 = _tmp82.py_dict_guard(); { let mut _tmp84 = (_tmp83.keys().cloned()).collect::<Vec<_>>(); _tmp84.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)); if false { _tmp84.reverse(); } Arc::new(Mutex::new(_tmp84)) } };
        if !((({ let _tmp85 = 0i64; py_list_get(&sorted_keys.py_list_guard(), _tmp85) }?) == "a".to_string())) { return Err(PyError::AssertionError(("sorted_keys[0] should equal \"a\"".to_string()).into())); }
        if !((({ let _tmp86 = 1i64; py_list_get(&sorted_keys.py_list_guard(), _tmp86) }?) == "b".to_string())) { return Err(PyError::AssertionError(("sorted_keys[1] should equal \"b\"".to_string()).into())); }
        if !((({ let _tmp87 = 2i64; py_list_get(&sorted_keys.py_list_guard(), _tmp87) }?) == "c".to_string())) { return Err(PyError::AssertionError(("sorted_keys[2] should equal \"c\"".to_string()).into())); }
        let sorted_range: Arc<Mutex<Vec<i64>>> = { let mut _tmp88 = (py_range(5i64)).collect::<Vec<_>>(); _tmp88.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)); if false { _tmp88.reverse(); } Arc::new(Mutex::new(_tmp88)) };
        if !((({ let _tmp89 = 0i64; py_list_get(&sorted_range.py_list_guard(), _tmp89) }?) == 0i64)) { return Err(PyError::AssertionError(("sorted_range[0] should equal 0".to_string()).into())); }
        if !((({ let _tmp90 = 1i64; py_list_get(&sorted_range.py_list_guard(), _tmp90) }?) == 1i64)) { return Err(PyError::AssertionError(("sorted_range[1] should equal 1".to_string()).into())); }
        if !((({ let _tmp91 = 2i64; py_list_get(&sorted_range.py_list_guard(), _tmp91) }?) == 2i64)) { return Err(PyError::AssertionError(("sorted_range[2] should equal 2".to_string()).into())); }
        if !((({ let _tmp92 = 3i64; py_list_get(&sorted_range.py_list_guard(), _tmp92) }?) == 3i64)) { return Err(PyError::AssertionError(("sorted_range[3] should equal 3".to_string()).into())); }
        if !((({ let _tmp93 = 4i64; py_list_get(&sorted_range.py_list_guard(), _tmp93) }?) == 4i64)) { return Err(PyError::AssertionError(("sorted_range[4] should equal 4".to_string()).into())); }
        let sorted_desc_range: Arc<Mutex<Vec<i64>>> = { let mut _tmp94 = ((py_range3(5i64, 0i64, (-1i64))?)).collect::<Vec<_>>(); _tmp94.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)); if false { _tmp94.reverse(); } Arc::new(Mutex::new(_tmp94)) };
        if !((({ let _tmp95 = 0i64; py_list_get(&sorted_desc_range.py_list_guard(), _tmp95) }?) == 1i64)) { return Err(PyError::AssertionError(("sorted_desc_range[0] should equal 1".to_string()).into())); }
        if !((({ let _tmp96 = 1i64; py_list_get(&sorted_desc_range.py_list_guard(), _tmp96) }?) == 2i64)) { return Err(PyError::AssertionError(("sorted_desc_range[1] should equal 2".to_string()).into())); }
        if !((({ let _tmp97 = 2i64; py_list_get(&sorted_desc_range.py_list_guard(), _tmp97) }?) == 3i64)) { return Err(PyError::AssertionError(("sorted_desc_range[2] should equal 3".to_string()).into())); }
        if !((({ let _tmp98 = 3i64; py_list_get(&sorted_desc_range.py_list_guard(), _tmp98) }?) == 4i64)) { return Err(PyError::AssertionError(("sorted_desc_range[3] should equal 4".to_string()).into())); }
        if !((({ let _tmp99 = 4i64; py_list_get(&sorted_desc_range.py_list_guard(), _tmp99) }?) == 5i64)) { return Err(PyError::AssertionError(("sorted_desc_range[4] should equal 5".to_string()).into())); }
        let empty: Arc<Mutex<Vec<i64>>> = Arc::new(Mutex::new(Vec::<i64>::new()));
        let sorted_empty: Arc<Mutex<Vec<i64>>> = { let _tmp100 = empty.clone(); let _tmp101 = _tmp100.py_list_guard(); { let mut _tmp102 = (_tmp101.iter().copied()).collect::<Vec<_>>(); _tmp102.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)); if false { _tmp102.reverse(); } Arc::new(Mutex::new(_tmp102)) } };
        if !((py_len(&sorted_empty) == 0i64)) { return Err(PyError::AssertionError(("len(sorted_empty) should equal 0".to_string()).into())); }
        let single: Arc<Mutex<Vec<i64>>> = Arc::new(Mutex::new(vec![42i64]));
        let sorted_single: Arc<Mutex<Vec<i64>>> = { let _tmp103 = single.clone(); let _tmp104 = _tmp103.py_list_guard(); { let mut _tmp105 = (_tmp104.iter().copied()).collect::<Vec<_>>(); _tmp105.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)); if false { _tmp105.reverse(); } Arc::new(Mutex::new(_tmp105)) } };
        if !((({ let _tmp106 = 0i64; py_list_get(&sorted_single.py_list_guard(), _tmp106) }?) == 42i64)) { return Err(PyError::AssertionError(("sorted_single[0] should equal 42".to_string()).into())); }
        let original_sorted: Arc<Mutex<Vec<i64>>> = Arc::new(Mutex::new(vec![3i64, 2i64, 1i64]));
        let _ = { let _tmp107 = original_sorted.clone(); let _tmp108 = _tmp107.py_list_guard(); { let mut _tmp109 = (_tmp108.iter().copied()).collect::<Vec<_>>(); _tmp109.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)); if false { _tmp109.reverse(); } Arc::new(Mutex::new(_tmp109)) } };
        if !((({ let _tmp110 = 0i64; py_list_get(&original_sorted.py_list_guard(), _tmp110) }?) == 3i64)) { return Err(PyError::AssertionError(("original_sorted[0] should equal 3".to_string()).into())); }
        if !((({ let _tmp111 = 1i64; py_list_get(&original_sorted.py_list_guard(), _tmp111) }?) == 2i64)) { return Err(PyError::AssertionError(("original_sorted[1] should equal 2".to_string()).into())); }
        if !((({ let _tmp112 = 2i64; py_list_get(&original_sorted.py_list_guard(), _tmp112) }?) == 1i64)) { return Err(PyError::AssertionError(("original_sorted[2] should equal 1".to_string()).into())); }
        let sorted_desc: Arc<Mutex<Vec<i64>>> = { let _tmp113 = sorted_nums.clone(); let _tmp114 = _tmp113.py_list_guard(); { let mut _tmp115 = (_tmp114.iter().copied()).collect::<Vec<_>>(); _tmp115.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)); if true { _tmp115.reverse(); } Arc::new(Mutex::new(_tmp115)) } };
        if !((({ let _tmp116 = 0i64; py_list_get(&sorted_desc.py_list_guard(), _tmp116) }?) == 9i64)) { return Err(PyError::AssertionError(("sorted_desc[0] should equal 9".to_string()).into())); }
        if !((({ let _tmp117 = 1i64; py_list_get(&sorted_desc.py_list_guard(), _tmp117) }?) == 6i64)) { return Err(PyError::AssertionError(("sorted_desc[1] should equal 6".to_string()).into())); }
        if !((({ let _tmp118 = 2i64; py_list_get(&sorted_desc.py_list_guard(), _tmp118) }?) == 5i64)) { return Err(PyError::AssertionError(("sorted_desc[2] should equal 5".to_string()).into())); }
        if !((({ let _tmp119 = 3i64; py_list_get(&sorted_desc.py_list_guard(), _tmp119) }?) == 4i64)) { return Err(PyError::AssertionError(("sorted_desc[3] should equal 4".to_string()).into())); }
        if !((({ let _tmp120 = 4i64; py_list_get(&sorted_desc.py_list_guard(), _tmp120) }?) == 3i64)) { return Err(PyError::AssertionError(("sorted_desc[4] should equal 3".to_string()).into())); }
        if !((({ let _tmp121 = 5i64; py_list_get(&sorted_desc.py_list_guard(), _tmp121) }?) == 2i64)) { return Err(PyError::AssertionError(("sorted_desc[5] should equal 2".to_string()).into())); }
        if !((({ let _tmp122 = 6i64; py_list_get(&sorted_desc.py_list_guard(), _tmp122) }?) == 1i64)) { return Err(PyError::AssertionError(("sorted_desc[6] should equal 1".to_string()).into())); }
        if !((({ let _tmp123 = 7i64; py_list_get(&sorted_desc.py_list_guard(), _tmp123) }?) == 1i64)) { return Err(PyError::AssertionError(("sorted_desc[7] should equal 1".to_string()).into())); }
        let sorted_words_desc: Arc<Mutex<Vec<String>>> = { let _tmp124 = words.clone(); let _tmp125 = _tmp124.py_list_guard(); { let mut _tmp126 = (_tmp125.iter().cloned()).collect::<Vec<_>>(); _tmp126.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)); if true { _tmp126.reverse(); } Arc::new(Mutex::new(_tmp126)) } };
        if !((({ let _tmp127 = 0i64; py_list_get(&sorted_words_desc.py_list_guard(), _tmp127) }?) == "cherry".to_string())) { return Err(PyError::AssertionError(("sorted_words_desc[0] should equal \"cherry\"".to_string()).into())); }
        if !((({ let _tmp128 = 1i64; py_list_get(&sorted_words_desc.py_list_guard(), _tmp128) }?) == "banana".to_string())) { return Err(PyError::AssertionError(("sorted_words_desc[1] should equal \"banana\"".to_string()).into())); }
        if !((({ let _tmp129 = 2i64; py_list_get(&sorted_words_desc.py_list_guard(), _tmp129) }?) == "apple".to_string())) { return Err(PyError::AssertionError(("sorted_words_desc[2] should equal \"apple\"".to_string()).into())); }
        let sorted_str_desc: Arc<Mutex<Vec<String>>> = { let mut _tmp130 = (str_s.chars().map(|c| c.to_string())).collect::<Vec<_>>(); _tmp130.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)); if true { _tmp130.reverse(); } Arc::new(Mutex::new(_tmp130)) };
        if !((({ let _tmp131 = 0i64; py_list_get(&sorted_str_desc.py_list_guard(), _tmp131) }?) == "c".to_string())) { return Err(PyError::AssertionError(("sorted_str_desc[0] should equal \"c\"".to_string()).into())); }
        if !((({ let _tmp132 = 1i64; py_list_get(&sorted_str_desc.py_list_guard(), _tmp132) }?) == "b".to_string())) { return Err(PyError::AssertionError(("sorted_str_desc[1] should equal \"b\"".to_string()).into())); }
        if !((({ let _tmp133 = 2i64; py_list_get(&sorted_str_desc.py_list_guard(), _tmp133) }?) == "a".to_string())) { return Err(PyError::AssertionError(("sorted_str_desc[2] should equal \"a\"".to_string()).into())); }
        let sorted_range_desc: Arc<Mutex<Vec<i64>>> = { let mut _tmp134 = (py_range(5i64)).collect::<Vec<_>>(); _tmp134.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)); if true { _tmp134.reverse(); } Arc::new(Mutex::new(_tmp134)) };
        if !((({ let _tmp135 = 0i64; py_list_get(&sorted_range_desc.py_list_guard(), _tmp135) }?) == 4i64)) { return Err(PyError::AssertionError(("sorted_range_desc[0] should equal 4".to_string()).into())); }
        if !((({ let _tmp136 = 1i64; py_list_get(&sorted_range_desc.py_list_guard(), _tmp136) }?) == 3i64)) { return Err(PyError::AssertionError(("sorted_range_desc[1] should equal 3".to_string()).into())); }
        if !((({ let _tmp137 = 2i64; py_list_get(&sorted_range_desc.py_list_guard(), _tmp137) }?) == 2i64)) { return Err(PyError::AssertionError(("sorted_range_desc[2] should equal 2".to_string()).into())); }
        if !((({ let _tmp138 = 3i64; py_list_get(&sorted_range_desc.py_list_guard(), _tmp138) }?) == 1i64)) { return Err(PyError::AssertionError(("sorted_range_desc[3] should equal 1".to_string()).into())); }
        if !((({ let _tmp139 = 4i64; py_list_get(&sorted_range_desc.py_list_guard(), _tmp139) }?) == 0i64)) { return Err(PyError::AssertionError(("sorted_range_desc[4] should equal 0".to_string()).into())); }
        let key_words: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(vec!["banana".to_string(), "apple".to_string(), "pie".to_string(), "watermelon".to_string()]));
        let sorted_by_len: Arc<Mutex<Vec<String>>> = { let _tmp140 = key_words.clone(); let _tmp141 = _tmp140.py_list_guard(); { let mut _tmp142 = (_tmp141.iter().cloned()).map(|item| ((str_len)(item.clone()), item)).collect::<Vec<_>>(); _tmp142.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal)); if false { _tmp142.reverse(); } Arc::new(Mutex::new(_tmp142.into_iter().map(|(_, item)| item).collect::<Vec<_>>())) } };
        if !((({ let _tmp143 = 0i64; py_list_get(&sorted_by_len.py_list_guard(), _tmp143) }?) == "pie".to_string())) { return Err(PyError::AssertionError(("sorted_by_len[0] should equal \"pie\"".to_string()).into())); }
        if !((({ let _tmp144 = 1i64; py_list_get(&sorted_by_len.py_list_guard(), _tmp144) }?) == "apple".to_string())) { return Err(PyError::AssertionError(("sorted_by_len[1] should equal \"apple\"".to_string()).into())); }
        if !((({ let _tmp145 = 2i64; py_list_get(&sorted_by_len.py_list_guard(), _tmp145) }?) == "banana".to_string())) { return Err(PyError::AssertionError(("sorted_by_len[2] should equal \"banana\"".to_string()).into())); }
        if !((({ let _tmp146 = 3i64; py_list_get(&sorted_by_len.py_list_guard(), _tmp146) }?) == "watermelon".to_string())) { return Err(PyError::AssertionError(("sorted_by_len[3] should equal \"watermelon\"".to_string()).into())); }
        let sorted_by_len_desc: Arc<Mutex<Vec<String>>> = { let _tmp147 = key_words.clone(); let _tmp148 = _tmp147.py_list_guard(); { let mut _tmp149 = (_tmp148.iter().cloned()).map(|item| ((str_len)(item.clone()), item)).collect::<Vec<_>>(); _tmp149.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal)); if true { _tmp149.reverse(); } Arc::new(Mutex::new(_tmp149.into_iter().map(|(_, item)| item).collect::<Vec<_>>())) } };
        if !((({ let _tmp150 = 0i64; py_list_get(&sorted_by_len_desc.py_list_guard(), _tmp150) }?) == "watermelon".to_string())) { return Err(PyError::AssertionError(("sorted_by_len_desc[0] should equal \"watermelon\"".to_string()).into())); }
        if !((({ let _tmp151 = 1i64; py_list_get(&sorted_by_len_desc.py_list_guard(), _tmp151) }?) == "banana".to_string())) { return Err(PyError::AssertionError(("sorted_by_len_desc[1] should equal \"banana\"".to_string()).into())); }
        if !((({ let _tmp152 = 2i64; py_list_get(&sorted_by_len_desc.py_list_guard(), _tmp152) }?) == "apple".to_string())) { return Err(PyError::AssertionError(("sorted_by_len_desc[2] should equal \"apple\"".to_string()).into())); }
        if !((({ let _tmp153 = 3i64; py_list_get(&sorted_by_len_desc.py_list_guard(), _tmp153) }?) == "pie".to_string())) { return Err(PyError::AssertionError(("sorted_by_len_desc[3] should equal \"pie\"".to_string()).into())); }
        let key_nums: Arc<Mutex<Vec<i64>>> = Arc::new(Mutex::new(vec![3i64, 1i64, 4i64, 1i64, 5i64, 9i64, 2i64, 6i64]));
        let sorted_by_neg: Arc<Mutex<Vec<i64>>> = { let _tmp154 = key_nums.clone(); let _tmp155 = _tmp154.py_list_guard(); { let mut _tmp156 = (_tmp155.iter().copied()).map(|item| ((negate)(item.clone()), item)).collect::<Vec<_>>(); _tmp156.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal)); if false { _tmp156.reverse(); } Arc::new(Mutex::new(_tmp156.into_iter().map(|(_, item)| item).collect::<Vec<_>>())) } };
        if !((({ let _tmp157 = 0i64; py_list_get(&sorted_by_neg.py_list_guard(), _tmp157) }?) == 9i64)) { return Err(PyError::AssertionError(("sorted_by_neg[0] should equal 9".to_string()).into())); }
        if !((({ let _tmp158 = 1i64; py_list_get(&sorted_by_neg.py_list_guard(), _tmp158) }?) == 6i64)) { return Err(PyError::AssertionError(("sorted_by_neg[1] should equal 6".to_string()).into())); }
        if !((({ let _tmp159 = 2i64; py_list_get(&sorted_by_neg.py_list_guard(), _tmp159) }?) == 5i64)) { return Err(PyError::AssertionError(("sorted_by_neg[2] should equal 5".to_string()).into())); }
        if !((({ let _tmp160 = 3i64; py_list_get(&sorted_by_neg.py_list_guard(), _tmp160) }?) == 4i64)) { return Err(PyError::AssertionError(("sorted_by_neg[3] should equal 4".to_string()).into())); }
        if !((({ let _tmp161 = 4i64; py_list_get(&sorted_by_neg.py_list_guard(), _tmp161) }?) == 3i64)) { return Err(PyError::AssertionError(("sorted_by_neg[4] should equal 3".to_string()).into())); }
        if !((({ let _tmp162 = 5i64; py_list_get(&sorted_by_neg.py_list_guard(), _tmp162) }?) == 2i64)) { return Err(PyError::AssertionError(("sorted_by_neg[5] should equal 2".to_string()).into())); }
        if !((({ let _tmp163 = 6i64; py_list_get(&sorted_by_neg.py_list_guard(), _tmp163) }?) == 1i64)) { return Err(PyError::AssertionError(("sorted_by_neg[6] should equal 1".to_string()).into())); }
        if !((({ let _tmp164 = 7i64; py_list_get(&sorted_by_neg.py_list_guard(), _tmp164) }?) == 1i64)) { return Err(PyError::AssertionError(("sorted_by_neg[7] should equal 1".to_string()).into())); }
        let mixed: Arc<Mutex<Vec<i64>>> = Arc::new(Mutex::new(vec![(-5i64), 2i64, (-3i64), 1i64, (-4i64)]));
        let sorted_by_abs: Arc<Mutex<Vec<i64>>> = { let _tmp165 = mixed.clone(); let _tmp166 = _tmp165.py_list_guard(); { let mut _tmp167 = (_tmp166.iter().copied()).map(|item| ((myabs)(item.clone()), item)).collect::<Vec<_>>(); _tmp167.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal)); if false { _tmp167.reverse(); } Arc::new(Mutex::new(_tmp167.into_iter().map(|(_, item)| item).collect::<Vec<_>>())) } };
        if !((myabs(({ let _tmp168 = 0i64; py_list_get(&sorted_by_abs.py_list_guard(), _tmp168) }?)) == 1i64)) { return Err(PyError::AssertionError(("myabs(sorted_by_abs[0]) should equal 1".to_string()).into())); }
        if !((myabs(({ let _tmp169 = 1i64; py_list_get(&sorted_by_abs.py_list_guard(), _tmp169) }?)) == 2i64)) { return Err(PyError::AssertionError(("myabs(sorted_by_abs[1]) should equal 2".to_string()).into())); }
        if !((myabs(({ let _tmp170 = 2i64; py_list_get(&sorted_by_abs.py_list_guard(), _tmp170) }?)) == 3i64)) { return Err(PyError::AssertionError(("myabs(sorted_by_abs[2]) should equal 3".to_string()).into())); }
        if !((myabs(({ let _tmp171 = 3i64; py_list_get(&sorted_by_abs.py_list_guard(), _tmp171) }?)) == 4i64)) { return Err(PyError::AssertionError(("myabs(sorted_by_abs[3]) should equal 4".to_string()).into())); }
        if !((myabs(({ let _tmp172 = 4i64; py_list_get(&sorted_by_abs.py_list_guard(), _tmp172) }?)) == 5i64)) { return Err(PyError::AssertionError(("myabs(sorted_by_abs[4]) should equal 5".to_string()).into())); }
        let names: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(vec!["charlie".to_string(), "alice".to_string(), "bob".to_string()]));
        let sorted_by_first: Arc<Mutex<Vec<String>>> = { let _tmp173 = names.clone(); let _tmp174 = _tmp173.py_list_guard(); { let mut _tmp175 = (_tmp174.iter().cloned()).map(|item| ((first_char)(item.clone()), item)).collect::<Vec<_>>(); _tmp175.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal)); if false { _tmp175.reverse(); } Arc::new(Mutex::new(_tmp175.into_iter().map(|(_, item)| item).collect::<Vec<_>>())) } };
        if !((({ let _tmp176 = 0i64; py_list_get(&sorted_by_first.py_list_guard(), _tmp176) }?) == "alice".to_string())) { return Err(PyError::AssertionError(("sorted_by_first[0] should equal \"alice\"".to_string()).into())); }
        if !((({ let _tmp177 = 1i64; py_list_get(&sorted_by_first.py_list_guard(), _tmp177) }?) == "bob".to_string())) { return Err(PyError::AssertionError(("sorted_by_first[1] should equal \"bob\"".to_string()).into())); }
        if !((({ let _tmp178 = 2i64; py_list_get(&sorted_by_first.py_list_guard(), _tmp178) }?) == "charlie".to_string())) { return Err(PyError::AssertionError(("sorted_by_first[2] should equal \"charlie\"".to_string()).into())); }
        let key_tup: (String, String, String) = ("bb".to_string(), "aaa".to_string(), "c".to_string());
        let sorted_key_tup: Arc<Mutex<Vec<String>>> = { let mut _tmp180 = ({ let _tmp179 = &key_tup; vec![_tmp179.0.clone(), _tmp179.1.clone(), _tmp179.2.clone()].into_iter() }).map(|item| ((str_len)(item.clone()), item)).collect::<Vec<_>>(); _tmp180.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal)); if false { _tmp180.reverse(); } Arc::new(Mutex::new(_tmp180.into_iter().map(|(_, item)| item).collect::<Vec<_>>())) };
        if !((({ let _tmp181 = 0i64; py_list_get(&sorted_key_tup.py_list_guard(), _tmp181) }?) == "c".to_string())) { return Err(PyError::AssertionError(("sorted_key_tup[0] should equal \"c\"".to_string()).into())); }
        if !((({ let _tmp182 = 1i64; py_list_get(&sorted_key_tup.py_list_guard(), _tmp182) }?) == "bb".to_string())) { return Err(PyError::AssertionError(("sorted_key_tup[1] should equal \"bb\"".to_string()).into())); }
        if !((({ let _tmp183 = 2i64; py_list_get(&sorted_key_tup.py_list_guard(), _tmp183) }?) == "aaa".to_string())) { return Err(PyError::AssertionError(("sorted_key_tup[2] should equal \"aaa\"".to_string()).into())); }
        let original_key: Arc<Mutex<Vec<i64>>> = Arc::new(Mutex::new(vec![3i64, 2i64, 1i64]));
        _ = { let _tmp184 = original_key.clone(); let _tmp185 = _tmp184.py_list_guard(); { let mut _tmp186 = (_tmp185.iter().copied()).map(|item| ((negate)(item.clone()), item)).collect::<Vec<_>>(); _tmp186.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal)); if false { _tmp186.reverse(); } Arc::new(Mutex::new(_tmp186.into_iter().map(|(_, item)| item).collect::<Vec<_>>())) } };
        if !((({ let _tmp187 = 0i64; py_list_get(&original_key.py_list_guard(), _tmp187) }?) == 3i64)) { return Err(PyError::AssertionError(("original_key[0] should equal 3".to_string()).into())); }
        if !((({ let _tmp188 = 1i64; py_list_get(&original_key.py_list_guard(), _tmp188) }?) == 2i64)) { return Err(PyError::AssertionError(("original_key[1] should equal 2".to_string()).into())); }
        if !((({ let _tmp189 = 2i64; py_list_get(&original_key.py_list_guard(), _tmp189) }?) == 1i64)) { return Err(PyError::AssertionError(("original_key[2] should equal 1".to_string()).into())); }
        let min_key_words: Vec<String> = vec!["banana".to_string(), "apple".to_string(), "pie".to_string(), "watermelon".to_string()];
        let min_by_len: String = ({ let mut _tmp190 = min_key_words.iter().cloned(); match _tmp190.next() { Some(first_item) => { let mut _tmp191 = first_item; let mut _tmp192 = (str_len)(_tmp191.clone()); for _tmp193 in _tmp190 { let _tmp194 = (str_len)(_tmp193.clone()); if _tmp194 < _tmp192 { _tmp191 = _tmp193; _tmp192 = _tmp194; } } Ok(_tmp191) }, None => Err(PyError::ValueError("min() arg is an empty sequence".into())) } }?);
        if !((min_by_len == "pie".to_string())) { return Err(PyError::AssertionError(("min_by_len should equal \"pie\"".to_string()).into())); }
        let max_by_len: String = ({ let mut _tmp195 = min_key_words.iter().cloned(); match _tmp195.next() { Some(first_item) => { let mut _tmp196 = first_item; let mut _tmp197 = (str_len)(_tmp196.clone()); for _tmp198 in _tmp195 { let _tmp199 = (str_len)(_tmp198.clone()); if _tmp199 > _tmp197 { _tmp196 = _tmp198; _tmp197 = _tmp199; } } Ok(_tmp196) }, None => Err(PyError::ValueError("max() arg is an empty sequence".into())) } }?);
        if !((max_by_len == "watermelon".to_string())) { return Err(PyError::AssertionError(("max_by_len should equal \"watermelon\"".to_string()).into())); }
        let min_mixed: Vec<i64> = vec![(-5i64), 2i64, (-3i64), 1i64, (-4i64)];
        let min_by_abs: i64 = ({ let mut _tmp200 = min_mixed.iter().copied(); match _tmp200.next() { Some(first_item) => { let mut _tmp201 = first_item; let mut _tmp202 = (myabs)(_tmp201); for _tmp203 in _tmp200 { let _tmp204 = (myabs)(_tmp203); if _tmp204 < _tmp202 { _tmp201 = _tmp203; _tmp202 = _tmp204; } } Ok(_tmp201) }, None => Err(PyError::ValueError("min() arg is an empty sequence".into())) } }?);
        if !((min_by_abs == 1i64)) { return Err(PyError::AssertionError(("min_by_abs should equal 1".to_string()).into())); }
        let max_by_abs: i64 = ({ let mut _tmp205 = min_mixed.iter().copied(); match _tmp205.next() { Some(first_item) => { let mut _tmp206 = first_item; let mut _tmp207 = (myabs)(_tmp206); for _tmp208 in _tmp205 { let _tmp209 = (myabs)(_tmp208); if _tmp209 > _tmp207 { _tmp206 = _tmp208; _tmp207 = _tmp209; } } Ok(_tmp206) }, None => Err(PyError::ValueError("max() arg is an empty sequence".into())) } }?);
        if !((max_by_abs == (-5i64))) { return Err(PyError::AssertionError(("max_by_abs should equal -5".to_string()).into())); }
        let min_nums: Vec<i64> = vec![3i64, 1i64, 4i64, 1i64, 5i64, 9i64, 2i64, 6i64];
        let min_by_neg: i64 = ({ let mut _tmp210 = min_nums.iter().copied(); match _tmp210.next() { Some(first_item) => { let mut _tmp211 = first_item; let mut _tmp212 = (negate)(_tmp211); for _tmp213 in _tmp210 { let _tmp214 = (negate)(_tmp213); if _tmp214 < _tmp212 { _tmp211 = _tmp213; _tmp212 = _tmp214; } } Ok(_tmp211) }, None => Err(PyError::ValueError("min() arg is an empty sequence".into())) } }?);
        if !((min_by_neg == 9i64)) { return Err(PyError::AssertionError(("min_by_neg should equal 9".to_string()).into())); }
        let max_by_neg: i64 = ({ let mut _tmp215 = min_nums.iter().copied(); match _tmp215.next() { Some(first_item) => { let mut _tmp216 = first_item; let mut _tmp217 = (negate)(_tmp216); for _tmp218 in _tmp215 { let _tmp219 = (negate)(_tmp218); if _tmp219 > _tmp217 { _tmp216 = _tmp218; _tmp217 = _tmp219; } } Ok(_tmp216) }, None => Err(PyError::ValueError("max() arg is an empty sequence".into())) } }?);
        if !((max_by_neg == 1i64)) { return Err(PyError::AssertionError(("max_by_neg should equal 1".to_string()).into())); }
        let min_key_tup: (String, String, String) = ("bb".to_string(), "aaa".to_string(), "c".to_string());
        let min_tup_result: String = ({ let mut _tmp221 = { let _tmp220 = &min_key_tup; vec![_tmp220.0.clone(), _tmp220.1.clone(), _tmp220.2.clone()].into_iter() }; match _tmp221.next() { Some(first_item) => { let mut _tmp222 = first_item; let mut _tmp223 = (str_len)(_tmp222.clone()); for _tmp224 in _tmp221 { let _tmp225 = (str_len)(_tmp224.clone()); if _tmp225 < _tmp223 { _tmp222 = _tmp224; _tmp223 = _tmp225; } } Ok(_tmp222) }, None => Err(PyError::ValueError("min() arg is an empty sequence".into())) } }?);
        if !((min_tup_result == "c".to_string())) { return Err(PyError::AssertionError(("min_tup_result should equal \"c\"".to_string()).into())); }
        let max_tup_result: String = ({ let mut _tmp227 = { let _tmp226 = &min_key_tup; vec![_tmp226.0.clone(), _tmp226.1.clone(), _tmp226.2.clone()].into_iter() }; match _tmp227.next() { Some(first_item) => { let mut _tmp228 = first_item; let mut _tmp229 = (str_len)(_tmp228.clone()); for _tmp230 in _tmp227 { let _tmp231 = (str_len)(_tmp230.clone()); if _tmp231 > _tmp229 { _tmp228 = _tmp230; _tmp229 = _tmp231; } } Ok(_tmp228) }, None => Err(PyError::ValueError("max() arg is an empty sequence".into())) } }?);
        if !((max_tup_result == "aaa".to_string())) { return Err(PyError::AssertionError(("max_tup_result should equal \"aaa\"".to_string()).into())); }
        let shortest: String = ({ let mut _tmp232 = (vec!["hello".to_string(), "hi".to_string(), "hey".to_string()].iter().cloned()).collect::<Vec<_>>().into_iter(); match _tmp232.next() { Some(first_item) => { let mut _tmp233 = first_item; let mut _tmp234 = (get_len)(_tmp233.clone()); for _tmp235 in _tmp232 { let _tmp236 = (get_len)(_tmp235.clone()); if _tmp236 < _tmp234 { _tmp233 = _tmp235; _tmp234 = _tmp236; } } Ok(_tmp233) }, None => Err(PyError::ValueError("min() arg is an empty sequence".into())) } }?);
        if !((shortest == "hi".to_string())) { return Err(PyError::AssertionError(("shortest should equal \"hi\"".to_string()).into())); }
        if !((py_len(&shortest) == 2i64)) { return Err(PyError::AssertionError(("len(shortest) should equal 2".to_string()).into())); }
        let min_key_set: HashSet<i64> = HashSet::from([(-5i64), 2i64, (-3i64), 1i64]);
        let min_set_by_abs: i64 = ({ let mut _tmp237 = min_key_set.iter().copied(); match _tmp237.next() { Some(first_item) => { let mut _tmp238 = first_item; let mut _tmp239 = (myabs)(_tmp238); for _tmp240 in _tmp237 { let _tmp241 = (myabs)(_tmp240); if _tmp241 < _tmp239 { _tmp238 = _tmp240; _tmp239 = _tmp241; } } Ok(_tmp238) }, None => Err(PyError::ValueError("min() arg is an empty sequence".into())) } }?);
        if !((min_set_by_abs == 1i64)) { return Err(PyError::AssertionError(("min_set_by_abs should equal 1".to_string()).into())); }
        let max_set_by_abs: i64 = ({ let mut _tmp242 = min_key_set.iter().copied(); match _tmp242.next() { Some(first_item) => { let mut _tmp243 = first_item; let mut _tmp244 = (myabs)(_tmp243); for _tmp245 in _tmp242 { let _tmp246 = (myabs)(_tmp245); if _tmp246 > _tmp244 { _tmp243 = _tmp245; _tmp244 = _tmp246; } } Ok(_tmp243) }, None => Err(PyError::ValueError("max() arg is an empty sequence".into())) } }?);
        if !((max_set_by_abs == (-5i64))) { return Err(PyError::AssertionError(("max_set_by_abs should equal -5".to_string()).into())); }
        let test_set: HashSet<i64> = HashSet::from([3i64, 1i64, 4i64, 1i64, 5i64, 9i64, 2i64, 6i64]);
        let min_by_negate: i64 = ({ let mut _tmp247 = test_set.iter().copied(); match _tmp247.next() { Some(first_item) => { let mut _tmp248 = first_item; let mut _tmp249 = (negate)(_tmp248); for _tmp250 in _tmp247 { let _tmp251 = (negate)(_tmp250); if _tmp251 < _tmp249 { _tmp248 = _tmp250; _tmp249 = _tmp251; } } Ok(_tmp248) }, None => Err(PyError::ValueError("min() arg is an empty sequence".into())) } }?);
        if !((min_by_negate == 9i64)) { return Err(PyError::AssertionError(("min with negate should equal 9".to_string()).into())); }
        let max_by_negate: i64 = ({ let mut _tmp252 = test_set.iter().copied(); match _tmp252.next() { Some(first_item) => { let mut _tmp253 = first_item; let mut _tmp254 = (negate)(_tmp253); for _tmp255 in _tmp252 { let _tmp256 = (negate)(_tmp255); if _tmp256 > _tmp254 { _tmp253 = _tmp255; _tmp254 = _tmp256; } } Ok(_tmp253) }, None => Err(PyError::ValueError("max() arg is an empty sequence".into())) } }?);
        if !((max_by_negate == 1i64)) { return Err(PyError::AssertionError(("max with negate should equal 1".to_string()).into())); }
        let str_set: HashSet<String> = HashSet::from(["hello".to_string(), "hi".to_string(), "hey".to_string(), "h".to_string()]);
        let shortest_in_set: String = ({ let mut _tmp257 = str_set.iter().cloned(); match _tmp257.next() { Some(first_item) => { let mut _tmp258 = first_item; let mut _tmp259 = (str_len)(_tmp258.clone()); for _tmp260 in _tmp257 { let _tmp261 = (str_len)(_tmp260.clone()); if _tmp261 < _tmp259 { _tmp258 = _tmp260; _tmp259 = _tmp261; } } Ok(_tmp258) }, None => Err(PyError::ValueError("min() arg is an empty sequence".into())) } }?);
        if !((shortest_in_set == "h".to_string())) { return Err(PyError::AssertionError(("shortest string should be 'h'".to_string()).into())); }
        let longest_in_set: String = ({ let mut _tmp262 = str_set.iter().cloned(); match _tmp262.next() { Some(first_item) => { let mut _tmp263 = first_item; let mut _tmp264 = (str_len)(_tmp263.clone()); for _tmp265 in _tmp262 { let _tmp266 = (str_len)(_tmp265.clone()); if _tmp266 > _tmp264 { _tmp263 = _tmp265; _tmp264 = _tmp266; } } Ok(_tmp263) }, None => Err(PyError::ValueError("max() arg is an empty sequence".into())) } }?);
        if !((longest_in_set == "hello".to_string())) { return Err(PyError::AssertionError(("longest string should be 'hello'".to_string()).into())); }
        py_print(&("min/max with key= tests passed!"));
        let enum_items: Vec<String> = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let mut enum_idx_sum: i64 = 0i64;
        {
            for (enum_i, enum_v) in ({ let _tmp268 = &enum_items; let mut _tmp267: usize = 0; std::iter::from_fn(move || { if _tmp267 < _tmp268.len() { let item = _tmp268[_tmp267].clone(); _tmp267 += 1; Some(item) } else { None } }) }).enumerate().map(|(i, v)| (i as i64 + 0i64, v)) {
                if (enum_i == 0i64) {
                    if !((enum_v == "a".to_string())) { return Err(PyError::AssertionError(("enum_v should equal \"a\"".to_string()).into())); }
                }
                if (enum_i == 1i64) {
                    if !((enum_v == "b".to_string())) { return Err(PyError::AssertionError(("enum_v should equal \"b\"".to_string()).into())); }
                }
                if (enum_i == 2i64) {
                    if !((enum_v == "c".to_string())) { return Err(PyError::AssertionError(("enum_v should equal \"c\"".to_string()).into())); }
                }
                enum_idx_sum = (enum_idx_sum + enum_i);
            }
        }
        if !((enum_idx_sum == 3i64)) { return Err(PyError::AssertionError(("enum_idx_sum should equal 3".to_string()).into())); }
        let mut enum_start_sum: i64 = 0i64;
        {
            for (enum_si, enum_sv) in (Arc::new(Mutex::new(vec!["x".to_string(), "y".to_string()])).py_list_guard().iter().cloned()).enumerate().map(|(i, v)| (i as i64 + 10i64, v)) {
                enum_start_sum = (enum_start_sum + enum_si);
                if (enum_si == 10i64) {
                    if !((enum_sv == "x".to_string())) { return Err(PyError::AssertionError(("enum_sv should equal \"x\"".to_string()).into())); }
                }
                if (enum_si == 11i64) {
                    if !((enum_sv == "y".to_string())) { return Err(PyError::AssertionError(("enum_sv should equal \"y\"".to_string()).into())); }
                }
            }
        }
        if !((enum_start_sum == 21i64)) { return Err(PyError::AssertionError(("enum_start_sum should equal 21".to_string()).into())); }
        {
            for (enum_ri, enum_rv) in (py_range(5i64)).enumerate().map(|(i, v)| (i as i64 + 0i64, v)) {
                if !((enum_ri == enum_rv)) { return Err(PyError::AssertionError(("enum_ri should equal enum_rv".to_string()).into())); }
            }
        }
        let mut enum_entered: bool = false;
        {
            for (enum_ei, enum_ev) in (Arc::new(Mutex::new(Vec::<PyRepr>::new())).py_list_guard().iter().cloned()).enumerate().map(|(i, v)| (i as i64 + 0i64, v)) {
                enum_entered = true;
            }
        }
        if !(((enum_entered as i64) == (false as i64))) { return Err(PyError::AssertionError(("enum_entered should equal False".to_string()).into())); }
        let enum_int_items: Vec<i64> = vec![10i64, 20i64, 30i64];
        {
            for (enum_ii, enum_iv) in ({ let _tmp270 = &enum_int_items; let mut _tmp269: usize = 0; std::iter::from_fn(move || { if _tmp269 < _tmp270.len() { let item = _tmp270[_tmp269]; _tmp269 += 1; Some(item) } else { None } }) }).enumerate().map(|(i, v)| (i as i64 + 0i64, v)) {
                if (enum_ii == 0i64) {
                    if !((enum_iv == 10i64)) { return Err(PyError::AssertionError(("enum_iv should equal 10".to_string()).into())); }
                }
                if (enum_ii == 1i64) {
                    if !((enum_iv == 20i64)) { return Err(PyError::AssertionError(("enum_iv should equal 20".to_string()).into())); }
                }
                if (enum_ii == 2i64) {
                    if !((enum_iv == 30i64)) { return Err(PyError::AssertionError(("enum_iv should equal 30".to_string()).into())); }
                }
            }
        }
        {
            for (enum_s1i, enum_s1v) in (Arc::new(Mutex::new(vec!["first".to_string(), "second".to_string()])).py_list_guard().iter().cloned()).enumerate().map(|(i, v)| (i as i64 + 1i64, v)) {
                if (enum_s1i == 1i64) {
                    if !((enum_s1v == "first".to_string())) { return Err(PyError::AssertionError(("enum_s1v should equal \"first\"".to_string()).into())); }
                }
                if (enum_s1i == 2i64) {
                    if !((enum_s1v == "second".to_string())) { return Err(PyError::AssertionError(("enum_s1v should equal \"second\"".to_string()).into())); }
                }
            }
        }
        let mut enum_unpack_results: Vec<i64> = Vec::<i64>::new();
        let enum_pairs: Vec<(i64, i64)> = vec![(1i64, 10i64), (2i64, 20i64), (3i64, 30i64)];
        {
            for (enum_ua, enum_ub) in enum_pairs.iter().cloned() {
                enum_unpack_results.push((enum_ua + enum_ub));
            }
        }
        if !(((py_list_get(&enum_unpack_results, 0i64)?) == 11i64)) { return Err(PyError::AssertionError(("enum_unpack_results[0] should equal 11".to_string()).into())); }
        if !(((py_list_get(&enum_unpack_results, 1i64)?) == 22i64)) { return Err(PyError::AssertionError(("enum_unpack_results[1] should equal 22".to_string()).into())); }
        if !(((py_list_get(&enum_unpack_results, 2i64)?) == 33i64)) { return Err(PyError::AssertionError(("enum_unpack_results[2] should equal 33".to_string()).into())); }
        let enum_names: Vec<(String, String)> = vec![("Alice".to_string(), "Smith".to_string()), ("Bob".to_string(), "Jones".to_string())];
        {
            for (enum_first, enum_last) in enum_names.iter().cloned() {
                if (enum_first == "Alice".to_string()) {
                    if !((enum_last == "Smith".to_string())) { return Err(PyError::AssertionError(("enum_last should equal \"Smith\"".to_string()).into())); }
                }
                if (enum_first == "Bob".to_string()) {
                    if !((enum_last == "Jones".to_string())) { return Err(PyError::AssertionError(("enum_last should equal \"Jones\"".to_string()).into())); }
                }
            }
        }
        let zip_list_a: Vec<String> = vec!["1".to_string(), "2".to_string(), "3".to_string()];
        let zip_list_b: Vec<String> = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let mut zip_results: Vec<String> = Vec::<String>::new();
        {
            for (a, b) in (zip_list_a.iter().cloned()).zip(zip_list_b.iter().cloned()) {
                zip_results.push(format!("{}:{}", a, b));
            }
        }
        if !({ let _left = &(zip_results); let _right = &(vec!["1:a".to_string(), "2:b".to_string(), "3:c".to_string()]); _left.iter().eq(_right.iter()) }) { return Err(PyError::AssertionError(("zip string iteration failed".to_string()).into())); }
        let zip_short: Vec<String> = vec!["x".to_string(), "y".to_string()];
        let zip_long: Vec<String> = vec!["1".to_string(), "2".to_string(), "3".to_string(), "4".to_string()];
        let mut zip_results2: Vec<String> = Vec::<String>::new();
        {
            for (a, b) in (zip_short.iter().cloned()).zip(zip_long.iter().cloned()) {
                zip_results2.push(format!("{}-{}", a, b));
            }
        }
        if !({ let _left = &(zip_results2); let _right = &(vec!["x-1".to_string(), "y-2".to_string()]); _left.iter().eq(_right.iter()) }) { return Err(PyError::AssertionError(("zip unequal lengths failed".to_string()).into())); }
        let mut zip_results3: Vec<String> = Vec::<String>::new();
        {
            for (a, b) in (zip_long.iter().cloned()).zip(zip_short.iter().cloned()) {
                zip_results3.push(format!("{}-{}", a, b));
            }
        }
        if !({ let _left = &(zip_results3); let _right = &(vec!["1-x".to_string(), "2-y".to_string()]); _left.iter().eq(_right.iter()) }) { return Err(PyError::AssertionError(("zip unequal lengths (reverse) failed".to_string()).into())); }
        let mut zip_iter = ({ let _tmp271 = Arc::new(Mutex::new(vec!["a".to_string(), "b".to_string()])).clone(); let mut _tmp272: usize = 0; std::iter::from_fn(move || { let _tmp273 = _tmp271.py_list_guard(); if _tmp272 < _tmp273.len() { let item = _tmp273[_tmp272].clone(); _tmp272 += 1; Some(item) } else { None } }) }).zip({ let _tmp274 = Arc::new(Mutex::new(vec!["1".to_string(), "2".to_string()])).clone(); let mut _tmp275: usize = 0; std::iter::from_fn(move || { let _tmp276 = _tmp274.py_list_guard(); if _tmp275 < _tmp276.len() { let item = _tmp276[_tmp275].clone(); _tmp275 += 1; Some(item) } else { None } }) });
        let zip_item1 = (py_next(zip_iter.next())?);
        let _tmp277 = zip_item1;
        let a1 = (_tmp277).0;
        let b1 = (_tmp277).1;
        if !((a1 == "a".to_string())) { return Err(PyError::AssertionError(("zip next() first item [0] failed".to_string()).into())); }
        if !((b1 == "1".to_string())) { return Err(PyError::AssertionError(("zip next() first item [1] failed".to_string()).into())); }
        let zip_item2 = (py_next(zip_iter.next())?);
        let _tmp278 = zip_item2;
        let a2 = (_tmp278).0;
        let b2 = (_tmp278).1;
        if !((a2 == "b".to_string())) { return Err(PyError::AssertionError(("zip next() second item [0] failed".to_string()).into())); }
        if !((b2 == "2".to_string())) { return Err(PyError::AssertionError(("zip next() second item [1] failed".to_string()).into())); }
        py_print(&("zip() tests passed!"));
        let items1: Vec<(i64, i64, i64)> = vec![(1i64, 2i64, 3i64), (4i64, 5i64, 6i64)];
        let mut first_elem_values: Vec<i64> = Vec::<i64>::new();
        let mut rest_elem_values: Vec<Arc<Mutex<Vec<i64>>>> = Vec::<Arc<Mutex<Vec<i64>>>>::new();
        let mut _tmp279: usize = 0;
        while _tmp279 < items1.len() {
            let __py_for_item_26860 = items1[_tmp279].clone();
            _tmp279 += 1;
            let _tmp280 = __py_for_item_26860;
            let first_elem = (_tmp280).0;
            let _tmp281 = Arc::new(Mutex::new(vec![_tmp280.1.clone(), _tmp280.2.clone()]));
            let rest_elems = _tmp281.clone();
            first_elem_values.push(first_elem);
            rest_elem_values.push(rest_elems);
        }
        if !((py_len(&first_elem_values) == 2i64)) { return Err(PyError::AssertionError(("Should have 2 first values".to_string()).into())); }
        if !(((py_list_get(&first_elem_values, 0i64)?) == 1i64)) { return Err(PyError::AssertionError(("First value [0] should be 1".to_string()).into())); }
        if !(((py_list_get(&first_elem_values, 1i64)?) == 4i64)) { return Err(PyError::AssertionError(("First value [1] should be 4".to_string()).into())); }
        if !((py_len(&rest_elem_values) == 2i64)) { return Err(PyError::AssertionError(("Should have 2 rest lists".to_string()).into())); }
        if !((py_len(&(py_list_get(&rest_elem_values, 0i64)?)) == 2i64)) { return Err(PyError::AssertionError(("rest_elem_values[0] length should be 2".to_string()).into())); }
        if !((({ let _tmp282 = 0i64; py_list_get(&(py_list_get(&rest_elem_values, 0i64)?).py_list_guard(), _tmp282) }?) == 2i64)) { return Err(PyError::AssertionError(("rest_elem_values[0][0] should be 2".to_string()).into())); }
        if !((({ let _tmp283 = 1i64; py_list_get(&(py_list_get(&rest_elem_values, 0i64)?).py_list_guard(), _tmp283) }?) == 3i64)) { return Err(PyError::AssertionError(("rest_elem_values[0][1] should be 3".to_string()).into())); }
        if !((py_len(&(py_list_get(&rest_elem_values, 1i64)?)) == 2i64)) { return Err(PyError::AssertionError(("rest_elem_values[1] length should be 2".to_string()).into())); }
        if !((({ let _tmp284 = 0i64; py_list_get(&(py_list_get(&rest_elem_values, 1i64)?).py_list_guard(), _tmp284) }?) == 5i64)) { return Err(PyError::AssertionError(("rest_elem_values[1][0] should be 5".to_string()).into())); }
        if !((({ let _tmp285 = 1i64; py_list_get(&(py_list_get(&rest_elem_values, 1i64)?).py_list_guard(), _tmp285) }?) == 6i64)) { return Err(PyError::AssertionError(("rest_elem_values[1][1] should be 6".to_string()).into())); }
        let items2: Vec<(i64, i64, i64)> = vec![(1i64, 2i64, 3i64), (4i64, 5i64, 6i64)];
        let mut start_elem_values: Vec<Arc<Mutex<Vec<i64>>>> = Vec::<Arc<Mutex<Vec<i64>>>>::new();
        let mut last_elem_values: Vec<i64> = Vec::<i64>::new();
        let mut _tmp286: usize = 0;
        while _tmp286 < items2.len() {
            let __py_for_item_27876 = items2[_tmp286].clone();
            _tmp286 += 1;
            let _tmp287 = __py_for_item_27876;
            let _tmp288 = Arc::new(Mutex::new(vec![_tmp287.0.clone(), _tmp287.1.clone()]));
            let start_elem = _tmp288.clone();
            let last_elem = (_tmp287).2;
            start_elem_values.push(start_elem);
            last_elem_values.push(last_elem);
        }
        if !((py_len(&last_elem_values) == 2i64)) { return Err(PyError::AssertionError(("Should have 2 last values".to_string()).into())); }
        if !(((py_list_get(&last_elem_values, 0i64)?) == 3i64)) { return Err(PyError::AssertionError(("Last value [0] should be 3".to_string()).into())); }
        if !(((py_list_get(&last_elem_values, 1i64)?) == 6i64)) { return Err(PyError::AssertionError(("Last value [1] should be 6".to_string()).into())); }
        if !((py_len(&start_elem_values) == 2i64)) { return Err(PyError::AssertionError(("Should have 2 start lists".to_string()).into())); }
        if !((py_len(&(py_list_get(&start_elem_values, 0i64)?)) == 2i64)) { return Err(PyError::AssertionError(("start_elem_values[0] length should be 2".to_string()).into())); }
        if !((({ let _tmp289 = 0i64; py_list_get(&(py_list_get(&start_elem_values, 0i64)?).py_list_guard(), _tmp289) }?) == 1i64)) { return Err(PyError::AssertionError(("start_elem_values[0][0] should be 1".to_string()).into())); }
        if !((({ let _tmp290 = 1i64; py_list_get(&(py_list_get(&start_elem_values, 0i64)?).py_list_guard(), _tmp290) }?) == 2i64)) { return Err(PyError::AssertionError(("start_elem_values[0][1] should be 2".to_string()).into())); }
        if !((py_len(&(py_list_get(&start_elem_values, 1i64)?)) == 2i64)) { return Err(PyError::AssertionError(("start_elem_values[1] length should be 2".to_string()).into())); }
        if !((({ let _tmp291 = 0i64; py_list_get(&(py_list_get(&start_elem_values, 1i64)?).py_list_guard(), _tmp291) }?) == 4i64)) { return Err(PyError::AssertionError(("start_elem_values[1][0] should be 4".to_string()).into())); }
        if !((({ let _tmp292 = 1i64; py_list_get(&(py_list_get(&start_elem_values, 1i64)?).py_list_guard(), _tmp292) }?) == 5i64)) { return Err(PyError::AssertionError(("start_elem_values[1][1] should be 5".to_string()).into())); }
        let items3: Vec<(i64, i64, i64, i64)> = vec![(1i64, 2i64, 3i64, 4i64), (5i64, 6i64, 7i64, 8i64)];
        let mut a_elem_values: Vec<i64> = Vec::<i64>::new();
        let mut mid_elem_values: Vec<Arc<Mutex<Vec<i64>>>> = Vec::<Arc<Mutex<Vec<i64>>>>::new();
        let mut z_elem_values: Vec<i64> = Vec::<i64>::new();
        let mut _tmp293: usize = 0;
        while _tmp293 < items3.len() {
            let __py_for_item_28937 = items3[_tmp293].clone();
            _tmp293 += 1;
            let _tmp294 = __py_for_item_28937;
            let a_elem = (_tmp294).0;
            let _tmp295 = Arc::new(Mutex::new(vec![_tmp294.1.clone(), _tmp294.2.clone()]));
            let mid_elem = _tmp295.clone();
            let z_elem = (_tmp294).3;
            a_elem_values.push(a_elem);
            mid_elem_values.push(mid_elem);
            z_elem_values.push(z_elem);
        }
        if !((py_len(&a_elem_values) == 2i64)) { return Err(PyError::AssertionError(("Should have 2 a values".to_string()).into())); }
        if !(((py_list_get(&a_elem_values, 0i64)?) == 1i64)) { return Err(PyError::AssertionError(("a value [0] should be 1".to_string()).into())); }
        if !(((py_list_get(&a_elem_values, 1i64)?) == 5i64)) { return Err(PyError::AssertionError(("a value [1] should be 5".to_string()).into())); }
        if !((py_len(&z_elem_values) == 2i64)) { return Err(PyError::AssertionError(("Should have 2 z values".to_string()).into())); }
        if !(((py_list_get(&z_elem_values, 0i64)?) == 4i64)) { return Err(PyError::AssertionError(("z value [0] should be 4".to_string()).into())); }
        if !(((py_list_get(&z_elem_values, 1i64)?) == 8i64)) { return Err(PyError::AssertionError(("z value [1] should be 8".to_string()).into())); }
        if !((py_len(&mid_elem_values) == 2i64)) { return Err(PyError::AssertionError(("Should have 2 mid lists".to_string()).into())); }
        if !((py_len(&(py_list_get(&mid_elem_values, 0i64)?)) == 2i64)) { return Err(PyError::AssertionError(("mid_elem_values[0] length should be 2".to_string()).into())); }
        if !((({ let _tmp296 = 0i64; py_list_get(&(py_list_get(&mid_elem_values, 0i64)?).py_list_guard(), _tmp296) }?) == 2i64)) { return Err(PyError::AssertionError(("mid_elem_values[0][0] should be 2".to_string()).into())); }
        if !((({ let _tmp297 = 1i64; py_list_get(&(py_list_get(&mid_elem_values, 0i64)?).py_list_guard(), _tmp297) }?) == 3i64)) { return Err(PyError::AssertionError(("mid_elem_values[0][1] should be 3".to_string()).into())); }
        if !((py_len(&(py_list_get(&mid_elem_values, 1i64)?)) == 2i64)) { return Err(PyError::AssertionError(("mid_elem_values[1] length should be 2".to_string()).into())); }
        if !((({ let _tmp298 = 0i64; py_list_get(&(py_list_get(&mid_elem_values, 1i64)?).py_list_guard(), _tmp298) }?) == 6i64)) { return Err(PyError::AssertionError(("mid_elem_values[1][0] should be 6".to_string()).into())); }
        if !((({ let _tmp299 = 1i64; py_list_get(&(py_list_get(&mid_elem_values, 1i64)?).py_list_guard(), _tmp299) }?) == 7i64)) { return Err(PyError::AssertionError(("mid_elem_values[1][1] should be 7".to_string()).into())); }
        let list_items: Vec<Arc<Mutex<Vec<i64>>>> = vec![Arc::new(Mutex::new(vec![10i64, 20i64, 30i64])), Arc::new(Mutex::new(vec![40i64, 50i64, 60i64]))];
        let mut list_first_elem: Vec<i64> = Vec::<i64>::new();
        let mut list_rest_elem: Vec<Arc<Mutex<Vec<i64>>>> = Vec::<Arc<Mutex<Vec<i64>>>>::new();
        let mut _tmp300: usize = 0;
        while _tmp300 < list_items.len() {
            let __py_for_item_30085 = list_items[_tmp300].clone();
            _tmp300 += 1;
            let _tmp301 = __py_for_item_30085;
            let _tmp302 = _tmp301.py_list_guard().len();
            if _tmp302 < 1 { panic!("Unpacking expected at least 1 values, got {}", _tmp302); }
            let first_item = ({ let _tmp303 = 0i64; py_list_get(&_tmp301.py_list_guard(), _tmp303) }?);
            let rest_items = Arc::new(Mutex::new((py_list_slice_step(&_tmp301.py_list_guard(), Some(1i64), None, 1i64)?)));
            list_first_elem.push(first_item);
            list_rest_elem.push(rest_items);
        }
        if !((py_len(&list_first_elem) == 2i64)) { return Err(PyError::AssertionError(("Should have 2 list first values".to_string()).into())); }
        if !(((py_list_get(&list_first_elem, 0i64)?) == 10i64)) { return Err(PyError::AssertionError(("list_first_elem[0] should be 10".to_string()).into())); }
        if !(((py_list_get(&list_first_elem, 1i64)?) == 40i64)) { return Err(PyError::AssertionError(("list_first_elem[1] should be 40".to_string()).into())); }
        if !((py_len(&list_rest_elem) == 2i64)) { return Err(PyError::AssertionError(("Should have 2 list rest values".to_string()).into())); }
        if !((py_len(&(py_list_get(&list_rest_elem, 0i64)?)) == 2i64)) { return Err(PyError::AssertionError(("list_rest_elem[0] length should be 2".to_string()).into())); }
        if !((({ let _tmp304 = 0i64; py_list_get(&(py_list_get(&list_rest_elem, 0i64)?).py_list_guard(), _tmp304) }?) == 20i64)) { return Err(PyError::AssertionError(("list_rest_elem[0][0] should be 20".to_string()).into())); }
        if !((({ let _tmp305 = 1i64; py_list_get(&(py_list_get(&list_rest_elem, 0i64)?).py_list_guard(), _tmp305) }?) == 30i64)) { return Err(PyError::AssertionError(("list_rest_elem[0][1] should be 30".to_string()).into())); }
        py_print(&("For loop starred unpacking tests passed!"));
        let mixed_tuple: (i64, String, bool) = (42i64, "hello".to_string(), true);
        let mut mixed_items: Vec<PyUnionBoolIntStr> = Vec::<PyUnionBoolIntStr>::new();
        {
            for item in { let _tmp306 = &mixed_tuple; vec![PyRepr(format!("{:?}", _tmp306.0)), PyRepr(format!("{:?}", _tmp306.1.clone())), PyRepr(format!("{:?}", _tmp306.2))].into_iter() } {
                mixed_items.push(item);
            }
        }
        if !((py_len(&mixed_items) == 3i64)) { return Err(PyError::AssertionError(("mixed_items should have 3 elements".to_string()).into())); }
        let int_tuple: (i64, i64, i64) = (1i64, 2i64, 3i64);
        let mut int_sum: i64 = 0i64;
        {
            for x in { let _tmp307 = &int_tuple; vec![_tmp307.0, _tmp307.1, _tmp307.2].into_iter() } {
                int_sum = (int_sum + x);
            }
        }
        if !((int_sum == 6i64)) { return Err(PyError::AssertionError(("int_sum should equal 6".to_string()).into())); }
        let pair_tuple: (String, i64) = ("key".to_string(), 42i64);
        let mut pair_items: Vec<PyUnionIntStr> = Vec::<PyUnionIntStr>::new();
        {
            for pair_item in { let _tmp308 = &pair_tuple; vec![PyRepr(format!("{:?}", _tmp308.0.clone())), PyRepr(format!("{:?}", _tmp308.1))].into_iter() } {
                pair_items.push(pair_item);
            }
        }
        if !((py_len(&pair_items) == 2i64)) { return Err(PyError::AssertionError(("pair_items should have 2 elements".to_string()).into())); }
        py_print(&("Mixed-type tuple iteration tests passed!"));
        py_print(&("All iteration and comprehension tests passed!"));
        Ok(())
    })();

    if let Err(e) = _result {
        eprintln!("Uncaught exception: {}", e);
        std::process::exit(1);
    }
}
