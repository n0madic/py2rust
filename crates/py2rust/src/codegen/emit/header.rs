// Header emission for generated Rust files.

use super::super::*;

impl<'a> Codegen<'a> {
    /// Emit file header with necessary imports and suppressions.
    ///
    /// We use #![allow(unused)] liberally because:
    /// 1. Generated code may have unused variables (Python allows this)
    /// 2. We don't want to burden users with clippy warnings on generated code
    /// 3. Dead code is harmless in generated output
    ///
    /// Map/set imports are only emitted when actually used (tracked by scan pass).
    pub(crate) fn emit_header(&mut self) {
        // Import names are namespace-mangled for collision safety and may violate
        // Rust style lints in generated code.
        self.push_line("#![allow(unused, non_snake_case, non_camel_case_types)]");
        let needs_hashmap = self.uses.hash_map || self.uses.index_map || self.uses.py_dict_get;
        if needs_hashmap {
            self.push_line("use std::collections::HashMap;");
        }
        if self.uses.index_map || self.uses.py_dict_get {
            // Emit insertion-order-preserving IndexMap implementation.
            self.emit_index_map_helper();
        }
        if self.uses.hash_set {
            self.push_line("use std::collections::HashSet;");
        }
        self.push_line("use std::cell::RefCell;");
        self.push_line("use std::rc::Rc;");
        // Arc/Mutex are required for list semantics and globals.
        self.push_line("use std::sync::{Arc, Mutex, OnceLock};");
        // Atomic types for shared mutable scalar fields (Arc<AtomicU64> etc.).
        if self.uses.shared_mutable_fields {
            self.push_line("use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};");
        }
        self.push_line("const __NAME__: &str = \"__main__\";");
        self.push_line("");
    }

    /// Emit a minimal insertion-order-preserving IndexMap that matches Python dict semantics.
    ///
    /// Uses Vec<(K,V)> for ordering + HashMap<K,usize> for O(1) lookup. This avoids
    /// external crate dependencies while preserving Python's guaranteed dict insertion order.
    fn emit_index_map_helper(&mut self) {
        self.push_line(INDEX_MAP_HELPER);
    }
}

/// Inline insertion-order-preserving IndexMap implementation for generated code.
const INDEX_MAP_HELPER: &str = r#"
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
"#;
