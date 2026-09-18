use super::offered_update;

#[test]
fn offered_update_newer_release_offers_its_tag() {
    let json = r#"{"tag_name":"v0.4.6"}"#;
    assert_eq!(
        offered_update(Some(json), "0.4.5").as_deref(),
        Some("0.4.6")
    );
}

#[test]
fn offered_update_same_release_offers_nothing() {
    assert_eq!(
        offered_update(Some(r#"{"tag_name":"v0.4.5"}"#), "0.4.5"),
        None
    );
}

#[test]
fn offered_update_failed_fetch_offers_nothing() {
    assert_eq!(offered_update(None, "0.4.5"), None);
}
