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
/// #554: parameterised resource — the chain id replaces `{chain}` in the
/// URI, e.g. `openrig://chains/rig:input-1/presets`.
pub const URI_CHAIN_PRESETS_TEMPLATE: &str = "openrig://chains/{chain}/presets";

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
                URI_CHAIN_PRESETS_TEMPLATE,
                "Chain preset bank (replace {chain} with a rig:<input> id) — JSON",
            ),
            None,
        ),
    ]
}

/// Resolve a resource URI by querying the frontend.
pub async fn read(bridge: &CommandBridge, uri: &str) -> Result<ReadResourceResult> {
    let kind = if let Some(chain_id) = parse_chain_presets_uri(uri) {
        QueryKind::ListChainPresets {
            chain: ChainId(chain_id),
        }
    } else {
        match uri {
            URI_PROJECT => QueryKind::ProjectYaml,
            URI_DEVICES => QueryKind::Devices,
            URI_IDS => QueryKind::Ids,
            URI_METERS => QueryKind::ChainMeters,
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
