use super::current_version;

#[test]
fn current_version_without_override_is_the_compiled_version() {
    assert_eq!(current_version(None, "0.4.5"), "0.4.5");
}

#[test]
fn current_version_override_replaces_the_compiled_version() {
    assert_eq!(current_version(Some("0.0.1".into()), "0.4.5"), "0.0.1");
}

#[test]
fn current_version_blank_override_is_ignored() {
    assert_eq!(current_version(Some("  ".into()), "0.4.5"), "0.4.5");
}
