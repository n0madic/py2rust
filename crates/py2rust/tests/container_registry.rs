use py2rust::container::registry::{all_container_methods, find_container_method, ContainerId};
use std::collections::HashSet;

#[test]
fn container_registry_names_are_unique_per_container() {
    let mut seen = HashSet::new();
    for spec in all_container_methods() {
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
    let actual: HashSet<&str> = all_container_methods()
        .iter()
        .filter(|spec| spec.container == ContainerId::Set)
        .map(|spec| spec.name)
        .collect();
    assert_eq!(actual, expected, "set method surface drifted");

    let extend = find_container_method(ContainerId::Set, "extend")
        .expect("set.extend must be present in registry");
    assert!(extend.shape.arity.accepts(1));
    assert!(!extend.shape.arity.accepts(0));
    let extend_err = extend
        .validate(0, &[])
        .expect_err("set.extend with zero args must fail");
    assert_eq!(extend_err.message(), "set.extend() expects one argument");

    let pop = find_container_method(ContainerId::Set, "pop")
        .expect("set.pop must be present in registry");
    assert!(pop.shape.arity.accepts(0));
    assert!(!pop.shape.arity.accepts(1));
    let pop_err = pop
        .validate(1, &[])
        .expect_err("set.pop with one arg must fail");
    assert_eq!(pop_err.message(), "set.pop() expects no arguments");
}
