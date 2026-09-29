//! #827, red-first: `openrig://paths` names the looper take library, so an MCP
//! client can find a saved take (and hand it to `set_chain_di_loop_source` as a
//! `File`) without re-deriving the per-OS data folder itself.

use application::query::ResolvedPaths;
use infra_filesystem::AssetPaths;
use serde_json::Value;

#[test]
fn the_paths_resource_reports_the_looper_take_library() {
    let resolved = ResolvedPaths::from_app_config(&AssetPaths::default());
    let json: Value = serde_json::from_str(&resolved.to_json()).expect("valid JSON envelope");

    assert_eq!(
        json.get("looper_takes_path").and_then(Value::as_str),
        Some(
            infra_filesystem::default_looper_takes_path()
                .to_string_lossy()
                .as_ref()
        ),
        "envelope: {json}"
    );
}
