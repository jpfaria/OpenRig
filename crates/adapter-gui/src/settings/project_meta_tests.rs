use super::{sanitize_name, should_dispatch_rename};

#[test]
fn empty_name_is_normalized_to_none() {
    assert_eq!(sanitize_name(""), None);
    assert_eq!(sanitize_name("   "), None);
}

#[test]
fn trimmed_non_empty_passes_through() {
    assert_eq!(sanitize_name("  Foo  "), Some("Foo".into()));
}

#[test]
fn should_dispatch_skips_when_unchanged() {
    assert!(!should_dispatch_rename(Some("Foo"), Some("Foo")));
    assert!(should_dispatch_rename(Some("Foo"), Some("Bar")));
    assert!(should_dispatch_rename(None, Some("Bar")));
    assert!(should_dispatch_rename(Some("Foo"), None));
    assert!(!should_dispatch_rename(None, None));
}
