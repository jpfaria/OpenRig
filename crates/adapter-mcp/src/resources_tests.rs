use super::{parse_chain_presets_uri, parse_chain_quality_uri, parse_chain_tone_uri};

#[test]
fn parses_chain_tone_uri() {
    assert_eq!(
        parse_chain_tone_uri("openrig://chains/rig:input-1/tone"),
        Some("rig:input-1".to_string())
    );
}

#[test]
fn rejects_non_tone_uris() {
    // The tone verdict and the objective quality report are different reads —
    // an agent asking for one must never silently get the other.
    assert_eq!(
        parse_chain_tone_uri("openrig://chains/rig:x/quality"),
        None,
        "quality is a different resource"
    );
    assert_eq!(parse_chain_tone_uri("openrig://chains//tone"), None);
    assert_eq!(parse_chain_tone_uri("openrig://project"), None);
}

#[test]
fn parses_chain_quality_uri() {
    assert_eq!(
        parse_chain_quality_uri("openrig://chains/rig:input-1/quality"),
        Some("rig:input-1".to_string())
    );
    assert_eq!(
        parse_chain_quality_uri("openrig://chains/standalone/quality"),
        Some("standalone".to_string())
    );
}

#[test]
fn rejects_non_quality_uris() {
    assert_eq!(parse_chain_quality_uri("openrig://chains//quality"), None);
    assert_eq!(
        parse_chain_quality_uri("openrig://chains/rig:x/presets"),
        None
    );
    assert_eq!(parse_chain_quality_uri("openrig://project"), None);
}

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
