//! Phase 5 wiring red-first (issue #548): bundled profile loader so the
//! daemon doesn't need a filesystem path to the asset directory at
//! runtime. The YAML files in `assets/midi-profiles/` are baked in via
//! `include_str!` so a release binary ships them untouched.

use adapter_midi::profile::load_bundled_profiles;

#[test]
fn loads_at_least_the_chocolate_plus_factory_profile() {
    let profiles = load_bundled_profiles();
    assert!(
        !profiles.is_empty(),
        "expected at least one bundled profile"
    );
    assert!(
        profiles
            .iter()
            .any(|p| p.name.contains("Chocolate") && p.source.as_deref() == Some("FootCtrlPlus")),
        "expected the Chocolate Plus factory profile to be bundled; got names: {:?}",
        profiles.iter().map(|p| &p.name).collect::<Vec<_>>()
    );
}

#[test]
fn every_bundled_profile_parses_cleanly() {
    let profiles = load_bundled_profiles();
    for p in &profiles {
        assert!(!p.bindings.is_empty(), "{} has no bindings", p.name);
    }
}
