//! Read-only MCP resources. The adapter never re-derives project structure:
//! it asks the frontend (which owns the `!Send` `Project`) to serialize via
//! domain code over the bridge query channel.

use anyhow::Result;
use application::bridge::{CommandBridge, QueryKind};
use rmcp::model::{Annotated, RawResource, ReadResourceResult, Resource, ResourceContents};

pub const URI_PROJECT: &str = "openrig://project";
pub const URI_DEVICES: &str = "openrig://devices";

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
    ]
}

/// Resolve a resource URI by querying the frontend.
pub async fn read(bridge: &CommandBridge, uri: &str) -> Result<ReadResourceResult> {
    let kind = match uri {
        URI_PROJECT => QueryKind::ProjectYaml,
        URI_DEVICES => QueryKind::Devices,
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
