//! Read-only MCP resources. The adapter never re-derives project structure:
//! it asks the frontend (which owns the `!Send` `Project`) to serialize via
//! domain code over the bridge query channel.

use anyhow::Result;
use application::bridge::{CommandBridge, QueryKind};
use domain::ids::ChainId;
use rmcp::model::{Annotated, RawResource, ReadResourceResult, Resource, ResourceContents};

pub const URI_PROJECT: &str = "openrig://project";
pub const URI_DEVICES: &str = "openrig://devices";
pub const URI_IDS: &str = "openrig://ids";
pub const URI_METERS: &str = "openrig://meters";
pub const URI_PRESETS: &str = "openrig://presets";
/// #554: parameterised resource — the chain id replaces `{chain}` in the
/// URI, e.g. `openrig://chains/rig:input-1/presets`.
pub const URI_CHAIN_PRESETS_TEMPLATE: &str = "openrig://chains/{chain}/presets";
/// #561 (expanded scope): full plugin catalog as JSON.
pub const URI_PLUGINS: &str = "openrig://plugins";
/// #561 (expanded scope): URI template for text search.
/// Concrete URIs look like `openrig://plugins/search/<query>`.
/// Matched BEFORE [`URI_PLUGIN_PREFIX`] so `search` is never read
/// as a manifest id.
pub const URI_PLUGIN_SEARCH_PREFIX: &str = "openrig://plugins/search/";
/// #561 (expanded scope): URI template for a single plugin by id.
/// Concrete URIs look like `openrig://plugins/<manifest_id>`.
pub const URI_PLUGIN_PREFIX: &str = "openrig://plugins/";
/// #572: URI template for one plugin's parameter schema (catalog-level).
/// Concrete URIs look like `openrig://plugins/<manifest_id>/params`.
/// Matched BEFORE [`URI_PLUGIN_PREFIX`] so the `/params` suffix is not
/// swallowed into a manifest id.
pub const URI_PLUGIN_PARAMS_TEMPLATE: &str = "openrig://plugins/{id}/params";
/// #572: URI template for one placed block's parameter snapshot
/// (schema + current value per parameter). Concrete URIs look like
/// `openrig://chains/<chain_id>/blocks/<block_id>/params`. Matched
/// BEFORE the chain-presets parser so the URI shapes do not collide.
pub const URI_BLOCK_PARAMS_TEMPLATE: &str = "openrig://chains/{chain}/blocks/{block}/params";

/// Static list of resources this server exposes.
pub fn resources() -> Vec<Resource> {
    vec![
        Annotated::new(
            RawResource::new(URI_PROJECT, "Current project (YAML)"),
            None,
        ),
        Annotated::new(
            RawResource::new(URI_DEVICES, "Available audio devices"),
            None,
        ),
        Annotated::new(
            RawResource::new(URI_IDS, "Chain/block IDs (for midi-map.yaml)"),
            None,
        ),
        Annotated::new(
            RawResource::new(URI_METERS, "Per-chain peak meters (dBFS)"),
            None,
        ),
        Annotated::new(
            RawResource::new(
                URI_PRESETS,
                "Project preset pool (all names in RigProject.presets) — JSON",
            ),
            None,
        ),
        Annotated::new(
            RawResource::new(
                URI_CHAIN_PRESETS_TEMPLATE,
                "Chain preset bank (replace {chain} with a rig:<input> id) — JSON",
            ),
            None,
        ),
        Annotated::new(
            RawResource::new(URI_PLUGINS, "Plugin catalog (id, kind, backend)"),
            None,
        ),
        Annotated::new(
            RawResource::new(
                URI_PLUGIN_PARAMS_TEMPLATE,
                "Plugin parameter schema (replace {id} with a manifest id) — JSON",
            ),
            None,
        ),
        Annotated::new(
            RawResource::new(
                URI_BLOCK_PARAMS_TEMPLATE,
                "Placed-block parameter snapshot (schema + current_value) — JSON",
            ),
            None,
        ),
    ]
}

/// Resolve a resource URI by querying the frontend.
pub async fn read(bridge: &CommandBridge, uri: &str) -> Result<ReadResourceResult> {
    // #572: `openrig://plugins/<id>/params` and
    // `openrig://chains/<cid>/blocks/<bid>/params` are sub-resources
    // of namespaces matched by simpler patterns — match BEFORE the
    // broader arms so the `/params` suffix is not swallowed.
    let kind = if let Some((chain, block)) = parse_block_params_uri(uri) {
        QueryKind::GetBlockParams {
            chain: ChainId(chain),
            block: domain::ids::BlockId(block),
        }
    } else if let Some(chain_id) = parse_chain_presets_uri(uri) {
        QueryKind::ListChainPresets {
            chain: ChainId(chain_id),
        }
    } else if let Some(plugin_id) = parse_plugin_params_uri(uri) {
        QueryKind::GetPluginParams { plugin_id }
    } else {
        match uri {
            URI_PROJECT => QueryKind::ProjectYaml,
            URI_DEVICES => QueryKind::Devices,
            URI_IDS => QueryKind::Ids,
            URI_METERS => QueryKind::ChainMeters,
            URI_PRESETS => QueryKind::ListProjectPresets,
            URI_PLUGINS => QueryKind::ListPluginCatalog,
            other if other.starts_with(URI_PLUGIN_SEARCH_PREFIX) => QueryKind::FindPlugins {
                query: other[URI_PLUGIN_SEARCH_PREFIX.len()..].to_string(),
            },
            other if other.starts_with(URI_PLUGIN_PREFIX) => QueryKind::GetPlugin {
                id: other[URI_PLUGIN_PREFIX.len()..].to_string(),
            },
            other => anyhow::bail!("unknown resource: {other}"),
        }
    };
    let text = bridge
        .query(kind)
        .await
        .map_err(|_| anyhow::anyhow!("frontend dropped the bridge"))?
        .map_err(|e| anyhow::anyhow!(e))?;
    Ok(ReadResourceResult::new(vec![ResourceContents::text(
        text, uri,
    )]))
}

/// Extract `<chain>` from `openrig://chains/<chain>/presets`. Returns
/// `None` for any other URI shape.
fn parse_chain_presets_uri(uri: &str) -> Option<String> {
    uri.strip_prefix("openrig://chains/")
        .and_then(|rest| rest.strip_suffix("/presets"))
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

/// Extract `<id>` from `openrig://plugins/<id>/params`. Returns `None`
/// for any other URI shape — empty id rejected so `.../params` with no
/// id is not addressable. Issue #572.
pub fn parse_plugin_params_uri(uri: &str) -> Option<String> {
    uri.strip_prefix("openrig://plugins/")
        .and_then(|rest| rest.strip_suffix("/params"))
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

/// Extract `(chain, block)` from
/// `openrig://chains/<chain>/blocks/<block>/params`. Returns `None`
/// for any other URI shape. Either segment empty → rejected.
/// Issue #572.
pub fn parse_block_params_uri(uri: &str) -> Option<(String, String)> {
    let rest = uri
        .strip_prefix("openrig://chains/")?
        .strip_suffix("/params")?;
    let (chain, after_chain) = rest.split_once("/blocks/")?;
    if chain.is_empty() || after_chain.is_empty() {
        return None;
    }
    Some((chain.to_string(), after_chain.to_string()))
}

#[cfg(test)]
mod tests {
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
}
