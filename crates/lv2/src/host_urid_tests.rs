//! #913 — the URID map a plugin gets its integers from.
//!
//! The LV2 contract is stability: the same URI must map to the same integer
//! for the lifetime of the instance. A plugin caches these on instantiation
//! and compares them on every `run()`, so a map that renumbered — or that
//! handed out 0, which LV2 reserves for "no URID" — would make the plugin
//! misread its own atom events.

use super::UridMap;

#[test]
fn the_same_uri_always_maps_to_the_same_urid() {
    let mut map = UridMap::new();
    let first = map.map("http://lv2plug.in/ns/ext/midi#MidiEvent");
    let again = map.map("http://lv2plug.in/ns/ext/midi#MidiEvent");
    assert_eq!(
        first, again,
        "a plugin caches this on instantiation and compares it every run()"
    );
}

#[test]
fn different_uris_get_different_urids() {
    let mut map = UridMap::new();
    let midi = map.map("http://lv2plug.in/ns/ext/midi#MidiEvent");
    let atom = map.map("http://lv2plug.in/ns/ext/atom#Sequence");
    assert_ne!(midi, atom);
}

#[test]
fn no_uri_is_ever_given_the_reserved_zero() {
    // LV2 reserves 0 for "no URID"; handing it out would read as absent.
    let mut map = UridMap::new();
    for uri in ["a", "b", "c"] {
        assert_ne!(map.map(uri), 0, "{uri} was given the reserved URID");
    }
}

#[test]
fn a_uri_mapped_between_two_others_keeps_its_number() {
    let mut map = UridMap::new();
    let first = map.map("first");
    map.map("second");
    map.map("third");
    assert_eq!(
        map.map("first"),
        first,
        "later mappings must not renumber an earlier one"
    );
}

#[test]
fn the_empty_uri_is_mapped_like_any_other() {
    // The C callback maps "" when the plugin hands over a string it cannot
    // decode; that must still be a stable, non-zero URID rather than a panic.
    let mut map = UridMap::new();
    let empty = map.map("");
    assert_ne!(empty, 0);
    assert_eq!(map.map(""), empty);
}

#[test]
fn a_fresh_map_starts_over() {
    // Each plugin instance owns its map: two instances need not agree, but
    // each must be internally consistent from its first call.
    let mut one = UridMap::new();
    let mut other = UridMap::new();
    assert_eq!(one.map("same"), other.map("same"));
    assert_eq!(one.map("same"), one.map("same"));
}
