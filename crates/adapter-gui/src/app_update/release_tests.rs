use super::latest_release_tag;

#[test]
fn latest_release_tag_reads_tag_name() {
    let json = r#"{"tag_name":"v0.4.6","name":"OpenRig v0.4.6","draft":false}"#;
    assert_eq!(latest_release_tag(json).as_deref(), Some("v0.4.6"));
}

#[test]
fn latest_release_tag_missing_field_returns_none() {
    assert_eq!(latest_release_tag(r#"{"message":"Not Found"}"#), None);
}

#[test]
fn latest_release_tag_malformed_json_returns_none() {
    assert_eq!(latest_release_tag("<html>rate limited</html>"), None);
}

#[test]
fn latest_release_tag_empty_tag_returns_none() {
    assert_eq!(latest_release_tag(r#"{"tag_name":""}"#), None);
}
