use super::parse_chain_presets_uri;

#[test]
fn parses_rig_input_chain_id() {
    assert_eq!(
        parse_chain_presets_uri("openrig://chains/rig:input-1/presets"),
        Some("rig:input-1".to_string())
    );
}

#[test]
fn parses_non_rig_chain_id() {
    assert_eq!(
        parse_chain_presets_uri("openrig://chains/standalone/presets"),
        Some("standalone".to_string())
    );
}

#[test]
fn rejects_missing_chain_segment() {
    assert_eq!(parse_chain_presets_uri("openrig://chains//presets"), None);
}

#[test]
fn rejects_unrelated_uri() {
    assert_eq!(parse_chain_presets_uri("openrig://project"), None);
    assert_eq!(parse_chain_presets_uri("openrig://chains/rig:x"), None);
    assert_eq!(parse_chain_presets_uri("openrig://chains/rig:x/foo"), None);
}
