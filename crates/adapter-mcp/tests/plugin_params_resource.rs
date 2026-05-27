//! Red-first (#572) tests for the `openrig://plugins/{id}/params` MCP
//! resource URI parser. The resource template lives in
//! `adapter-mcp/src/resources.rs`; this test pins its public URI shape.

use adapter_mcp::resources::{parse_plugin_params_uri, URI_PLUGIN_PARAMS_TEMPLATE};

#[test]
fn parse_plugin_params_uri_extracts_plugin_id() {
    assert_eq!(
        parse_plugin_params_uri("openrig://plugins/british_70s/params"),
        Some("british_70s".to_string())
    );
}

#[test]
fn parse_plugin_params_uri_ignores_unrelated_uris() {
    assert_eq!(parse_plugin_params_uri("openrig://plugins"), None);
    assert_eq!(parse_plugin_params_uri("openrig://plugins/british_70s"), None);
    assert_eq!(
        parse_plugin_params_uri("openrig://chains/rig:input-1/presets"),
        None
    );
    // Empty id is rejected: `.../params` with no id is not a valid
    // resource address.
    assert_eq!(parse_plugin_params_uri("openrig://plugins//params"), None);
}

#[test]
fn plugin_params_template_constant_matches_spec() {
    assert_eq!(
        URI_PLUGIN_PARAMS_TEMPLATE,
        "openrig://plugins/{id}/params"
    );
}
