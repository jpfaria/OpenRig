use super::is_newer;

#[test]
fn is_newer_higher_patch_tag_returns_true() {
    assert!(is_newer("v0.4.6", "0.4.5"));
}

#[test]
fn is_newer_higher_minor_or_major_tag_returns_true() {
    assert!(is_newer("v0.5.0", "0.4.9"));
    assert!(is_newer("v1.0.0", "0.9.9"));
}

#[test]
fn is_newer_same_version_returns_false() {
    assert!(!is_newer("v0.4.5", "0.4.5"));
}

#[test]
fn is_newer_older_tag_returns_false() {
    assert!(!is_newer("v0.4.4", "0.4.5"));
    assert!(!is_newer("v0.4.5", "0.5.0-beta.1"));
}

#[test]
fn is_newer_final_release_beats_its_own_prerelease() {
    assert!(is_newer("v0.5.0", "0.5.0-beta.3"));
}

#[test]
fn is_newer_compares_numerically_not_lexically() {
    assert!(is_newer("v0.4.10", "0.4.9"));
}

#[test]
fn is_newer_unparsable_input_returns_false() {
    assert!(!is_newer("nightly", "0.4.5"));
    assert!(!is_newer("v0.4.6", "dev"));
    assert!(!is_newer("", "0.4.5"));
}
