//! #913 — where the MIDI daemon looks for profiles.
//!
//! Two roots, and they must never collapse into one: the factory dir ships
//! with the install and is replaced on upgrade, while the user dir is where a
//! dropped `<name>.yaml` survives one. If they resolved to the same place an
//! upgrade would wipe the user's own profiles.

use super::{factory_profiles_dir, user_profiles_dir};

#[test]
fn the_factory_profiles_live_under_the_install_data_root() {
    let dir = factory_profiles_dir();
    assert!(
        dir.ends_with("assets/midi-profiles"),
        "unexpected factory dir: {}",
        dir.display()
    );
    assert!(dir.starts_with(infra_filesystem::detect_data_root()));
}

#[test]
fn the_user_profiles_live_under_this_platforms_data_dir() {
    let dir = user_profiles_dir();
    assert!(
        dir.ends_with("openrig/midi-profiles"),
        "unexpected user dir: {}",
        dir.display()
    );
    assert!(
        dir.is_absolute() || dir.starts_with("."),
        "the fallback is a relative '.', anything else must be absolute: {}",
        dir.display()
    );
}

#[test]
fn the_two_roots_are_never_the_same_directory() {
    assert_ne!(
        factory_profiles_dir(),
        user_profiles_dir(),
        "an upgrade replaces the factory dir — sharing it would delete the \
         user's own profiles"
    );
}

#[test]
fn both_roots_are_stable_across_calls() {
    assert_eq!(factory_profiles_dir(), factory_profiles_dir());
    assert_eq!(user_profiles_dir(), user_profiles_dir());
}
