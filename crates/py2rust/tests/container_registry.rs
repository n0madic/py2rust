use py2rust::container::registry::{container_method_specs, resolve_container_method, ContainerId};
use std::collections::HashSet;

#[test]
fn container_registry_names_are_unique_per_container() {
    let mut seen = HashSet::new();
    for spec in container_method_specs() {
        assert!(
            seen.insert((spec.container, spec.name)),
            "duplicate container method in registry: {:?}.{}",
            spec.container,
            spec.name
        );
    }
}

#[test]
fn set_registry_surface_and_arity_are_stable() {
    let expected: HashSet<&str> =
        HashSet::from(["add", "remove", "discard", "clear", "copy", "extend", "pop"]);
    let actual: HashSet<&str> = container_method_specs()
        .iter()
        .filter(|spec| spec.container == ContainerId::Set)
        .map(|spec| spec.name)
        .collect();
    assert_eq!(actual, expected, "set method surface drifted");

    let extend = resolve_container_method(ContainerId::Set, "extend")
        .expect("set.extend must be present in registry");
    assert!(extend.accepts_arity(1));
    assert!(!extend.accepts_arity(0));
    assert_eq!(extend.arity_error(), "set.extend() expects one argument");

    let pop = resolve_container_method(ContainerId::Set, "pop")
        .expect("set.pop must be present in registry");
    assert!(pop.accepts_arity(0));
    assert!(!pop.accepts_arity(1));
    assert_eq!(pop.arity_error(), "set.pop() expects no arguments");
}
