use super::{
    kind_for_uri, parse_chain_presets_uri, parse_chain_quality_uri, parse_chain_tone_uri,
    resources, uri_for,
};
use application::bridge::QueryKind;
use domain::ids::{BlockId, ChainId};

/// One representative of every `QueryKind`. `uri_for` is exhaustive by
/// compilation; this list keeps the round-trip honest — a new variant that
/// compiles there still has to resolve back through `kind_for_uri`.
fn every_query_kind() -> Vec<QueryKind> {
    vec![
        QueryKind::ProjectYaml,
        QueryKind::Devices,
        QueryKind::Ids,
        QueryKind::ChainMeters,
        QueryKind::TunerReadings,
        QueryKind::SpectrumReadings,
        QueryKind::DiLoopState,
        QueryKind::ChainLatency {
            chain: ChainId("rig:input-1".into()),
        },
        QueryKind::ListProjectPresets,
        QueryKind::ListPluginCatalog,
        QueryKind::Paths,
        QueryKind::ListChainPresets {
            chain: ChainId("rig:input-1".into()),
        },
        QueryKind::ChainQualityReport {
            chain: ChainId("rig:input-1".into()),
        },
        QueryKind::ChainToneReport {
            chain: ChainId("rig:input-1".into()),
        },
        QueryKind::GetBlockParams {
            chain: ChainId("rig:input-1".into()),
            block: BlockId("block-1".into()),
        },
        QueryKind::GetPluginParams {
            plugin_id: "amp.native".into(),
        },
        QueryKind::GetPlugin {
            id: "amp.native".into(),
        },
        QueryKind::FindPlugins {
            query: "reverb".into(),
        },
    ]
}

/// #829: every readable state has an MCP URI that resolves back to it.
#[test]
fn every_query_kind_round_trips_through_its_mcp_uri() {
    for kind in every_query_kind() {
        let uri = uri_for(&kind);
        let resolved = kind_for_uri(&uri)
            .unwrap_or_else(|e| panic!("uri {uri} for {kind:?} is not readable: {e}"));
        assert_eq!(
            format!("{resolved:?}"),
            format!("{kind:?}"),
            "uri {uri} resolved to a different query kind"
        );
    }
}

/// #829: the tuner and the spectrum are windows the user reads on screen,
/// so every transport must be able to read them too.
#[test]
fn analyzer_readings_are_listed_resources() {
    let uris: Vec<String> = resources().iter().map(|r| r.raw.uri.to_string()).collect();

    assert!(
        uris.contains(&"openrig://tuner".to_string()),
        "tuner readings missing from the MCP resource list: {uris:?}"
    );
    assert!(
        uris.contains(&"openrig://spectrum".to_string()),
        "spectrum readings missing from the MCP resource list: {uris:?}"
    );
}

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
    assert_eq!(parse_chain_tone_uri("openrig://project"), None);}

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
