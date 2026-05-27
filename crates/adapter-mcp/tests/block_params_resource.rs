//! Red-first (#572) tests for the
//! `openrig://chains/{chain}/blocks/{block}/params` MCP resource URI
//! parser. Pinned wire shape so MCP clients always know how to address
//! a placed block's parameter snapshot.

use adapter_mcp::resources::{parse_block_params_uri, URI_BLOCK_PARAMS_TEMPLATE};

#[test]
fn parse_block_params_uri_extracts_chain_and_block() {
    assert_eq!(
        parse_block_params_uri("openrig://chains/rig:input-1/blocks/blk:42/params"),
        Some(("rig:input-1".to_string(), "blk:42".to_string()))
    );
}

#[test]
fn parse_block_params_uri_ignores_unrelated_uris() {
    assert_eq!(parse_block_params_uri("openrig://chains"), None);
    assert_eq!(
        parse_block_params_uri("openrig://chains/rig:input-1/presets"),
        None
    );
    assert_eq!(
        parse_block_params_uri("openrig://chains/rig:input-1/blocks/blk:42"),
        None
    );
    // Empty chain or block segment is rejected.
    assert_eq!(
        parse_block_params_uri("openrig://chains//blocks/blk:42/params"),
        None
    );
    assert_eq!(
        parse_block_params_uri("openrig://chains/rig:input-1/blocks//params"),
        None
    );
}

#[test]
fn block_params_template_constant_matches_spec() {
    assert_eq!(
        URI_BLOCK_PARAMS_TEMPLATE,
        "openrig://chains/{chain}/blocks/{block}/params"
    );
}
