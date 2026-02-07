use py2rust::builtin::registry::{builtin_specs, resolve_builtin};
use std::collections::HashSet;

#[test]
fn builtin_registry_roundtrip_and_uniqueness() {
    let mut seen = HashSet::new();
    for spec in builtin_specs() {
        assert!(
            seen.insert(spec.name),
            "duplicate builtin name in registry: {}",
            spec.name
        );
        let resolved = resolve_builtin(spec.name).expect("builtin must resolve from its own name");
        assert_eq!(
            resolved.id, spec.id,
            "builtin id mismatch for {}",
            spec.name
        );
    }
}

#[test]
fn builtin_keyword_policy_matches_expected_surface() {
    let allowed: HashSet<&str> = builtin_specs()
        .iter()
        .filter(|spec| spec.allow_keywords)
        .map(|spec| spec.name)
        .collect();
    let expected = HashSet::from(["print", "sorted", "max", "min"]);
    assert_eq!(allowed, expected, "keyword policy drifted");
}
