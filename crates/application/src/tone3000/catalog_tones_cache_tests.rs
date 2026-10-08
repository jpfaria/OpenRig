//! The TONE3000 browser polls the catalog four times a second; the catalog
//! is only walked again when its generation moves.

use std::cell::Cell;

use super::GenerationCache;

#[test]
fn the_value_is_computed_once_per_generation() {
    let cache = GenerationCache::default();
    let computed = Cell::new(0);
    let compute = || {
        computed.set(computed.get() + 1);
        vec![computed.get()]
    };
    assert_eq!(cache.get(7, compute), vec![1]);
    assert_eq!(cache.get(7, compute), vec![1]);
    assert_eq!(cache.get(7, compute), vec![1]);
    assert_eq!(computed.get(), 1, "the same generation was walked again");
}

#[test]
fn a_new_generation_computes_the_value_again() {
    let cache = GenerationCache::default();
    assert_eq!(cache.get(1, || vec!["old"]), vec!["old"]);
    assert_eq!(cache.get(2, || vec!["new"]), vec!["new"]);
    assert_eq!(cache.get(2, || vec!["stale"]), vec!["new"]);
}
