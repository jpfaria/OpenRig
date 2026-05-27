//! Read-only MCP resources. The adapter never re-derives project structure:
//! it asks the frontend (which owns the `!Send` `Project`) to serialize via
//! domain code over the bridge query channel.

use anyhow::Result;
use application::bridge::{CommandBridge, QueryKind};
use rmcp::model::{Annotated, RawResource, ReadResourceResult, Resource, ResourceContents};

pub const URI_PROJECT: &str = "openrig://project";
pub const URI_DEVICES: &str = "openrig://devices";
pub const URI_IDS: &str = "openrig://ids";
pub const URI_METERS: &str = "openrig://meters";
/// #561 (expanded scope): full plugin catalog as JSON.
pub const URI_PLUGINS: &str = "openrig://plugins";
/// #561 (expanded scope): URI template for a single plugin by id.
/// Concrete URIs look like `openrig://plugins/<manifest_id>`.
pub const URI_PLUGIN_PREFIX: &str = "openrig://plugins/";

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
            RawResource::new(URI_PLUGINS, "Plugin catalog (id, kind, backend)"),
            None,
        ),
    ]
}

/// Resolve a resource URI by querying the frontend.
pub async fn read(bridge: &CommandBridge, uri: &str) -> Result<ReadResourceResult> {
    let kind = match uri {
        URI_PROJECT => QueryKind::ProjectYaml,
        URI_DEVICES => QueryKind::Devices,
        URI_IDS => QueryKind::Ids,
        URI_METERS => QueryKind::ChainMeters,
        URI_PLUGINS => QueryKind::ListPluginCatalog,
        other if other.starts_with(URI_PLUGIN_PREFIX) => QueryKind::GetPlugin {
            id: other[URI_PLUGIN_PREFIX.len()..].to_string(),
        },
        other => anyhow::bail!("unknown resource: {other}"),
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
