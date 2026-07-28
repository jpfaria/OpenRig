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
/// #829: live tuner readings — read parity for `SetTunerEnabled`.
pub const URI_TUNER: &str = "openrig://tuner";
/// #829: live spectrum readings — read parity for `SetSpectrumEnabled`.
pub const URI_SPECTRUM: &str = "openrig://spectrum";
/// #829: per-chain DI loop state — read parity for the DI commands.
pub const URI_DI: &str = "openrig://di";
/// #829: per-chain latency probe. Concrete URIs look like
/// `openrig://chains/<chain_id>/latency`.
pub const URI_CHAIN_LATENCY_TEMPLATE: &str = "openrig://chains/{chain}/latency";
pub const URI_PRESETS: &str = "openrig://presets";
/// #582: effective resolved system paths (data root + every
/// configurable directory). Lets skills and other MCP clients read
/// target locations dynamically instead of re-implementing the
/// per-platform OS-default logic. The envelope is built from a struct
/// in `application::query::ResolvedPaths` so adding a new path field
/// is a hard compile error here too.
pub const URI_PATHS: &str = "openrig://paths";
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

/// #791: URI template for one chain's objective quality report (THD+N, noise
/// floor, level, dynamic range, clipping). Concrete URIs look like
/// `openrig://chains/<chain_id>/quality`.
pub const URI_CHAIN_QUALITY_TEMPLATE: &str = "openrig://chains/{chain}/quality";

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
                URI_TUNER,
                "Live tuner readings (note / cents / frequency per tap) — JSON",
            ),
            None,
        ),
        Annotated::new(
            RawResource::new(
                URI_SPECTRUM,
                "Live spectrum readings (per-band levels and peak holds) — JSON",
            ),
            None,
        ),
        Annotated::new(
            RawResource::new(
                URI_DI,
                "Per-chain DI loop state (playing, playback peaks, source) — JSON",
            ),
            None,
        ),
        Annotated::new(
            RawResource::new(
                URI_CHAIN_LATENCY_TEMPLATE,
                "Measured chain latency (replace {chain} with a chain id) — JSON",
            ),
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
                URI_PATHS,
                "Effective resolved system paths (data root + every configurable directory) — JSON",
            ),
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
                URI_CHAIN_QUALITY_TEMPLATE,
                "Objective chain quality report (replace {chain} with a chain id) — JSON",
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
    let kind = kind_for_uri(uri)?;
    let text = bridge
        .query(kind)
        .await
        .map_err(|_| anyhow::anyhow!("frontend dropped the bridge"))?
        .map_err(|e| anyhow::anyhow!(e))?;
    Ok(ReadResourceResult::new(vec![ResourceContents::text(
        text, uri,
    )]))
}

/// Pure URI → [`QueryKind`] resolution. Split out of [`read`] so the
/// read-parity guard can round-trip every kind without a live bridge.
pub fn kind_for_uri(uri: &str) -> Result<QueryKind> {
    // #572: `openrig://plugins/<id>/params` and
    // `openrig://chains/<cid>/blocks/<bid>/params` are sub-resources
    // of namespaces matched by simpler patterns — match BEFORE the
    // broader arms so the `/params` suffix is not swallowed.
    let kind = if let Some((chain, block)) = parse_block_params_uri(uri) {
        QueryKind::GetBlockParams {
            chain: ChainId(chain),
            block: domain::ids::BlockId(block),
        }
    } else if let Some(chain_id) = parse_chain_latency_uri(uri) {
        QueryKind::ChainLatency {
            chain: ChainId(chain_id),
        }
    } else if let Some(chain_id) = parse_chain_quality_uri(uri) {
        QueryKind::ChainQualityReport {
            chain: ChainId(chain_id),
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
            URI_TUNER => QueryKind::TunerReadings,
            URI_SPECTRUM => QueryKind::SpectrumReadings,
            URI_DI => QueryKind::DiLoopState,
            URI_PRESETS => QueryKind::ListProjectPresets,
            URI_PLUGINS => QueryKind::ListPluginCatalog,
            URI_PATHS => QueryKind::Paths,
            other if other.starts_with(URI_PLUGIN_SEARCH_PREFIX) => QueryKind::FindPlugins {
                query: other[URI_PLUGIN_SEARCH_PREFIX.len()..].to_string(),
            },
            other if other.starts_with(URI_PLUGIN_PREFIX) => QueryKind::GetPlugin {
                id: other[URI_PLUGIN_PREFIX.len()..].to_string(),
            },
            other => anyhow::bail!("unknown resource: {other}"),
        }
    };
    Ok(kind)
}

/// #829 read-parity guard: the URI that serves each [`QueryKind`].
///
/// The match has **no wildcard arm** on purpose — adding a query kind
/// without exposing it over MCP is a build error right here, so "every
/// window the user reads is readable by every transport" cannot regress
/// silently. Templated kinds render a concrete URI from their arguments.
pub fn uri_for(kind: &QueryKind) -> String {
    match kind {
        QueryKind::ProjectYaml => URI_PROJECT.to_string(),
        QueryKind::Devices => URI_DEVICES.to_string(),
        QueryKind::Ids => URI_IDS.to_string(),
        QueryKind::ChainMeters => URI_METERS.to_string(),
        QueryKind::TunerReadings => URI_TUNER.to_string(),
        QueryKind::SpectrumReadings => URI_SPECTRUM.to_string(),
        QueryKind::DiLoopState => URI_DI.to_string(),
        QueryKind::ChainLatency { chain } => {
            format!("openrig://chains/{}/latency", chain.0)
        }
        QueryKind::ListProjectPresets => URI_PRESETS.to_string(),
        QueryKind::ListPluginCatalog => URI_PLUGINS.to_string(),
        QueryKind::Paths => URI_PATHS.to_string(),
        QueryKind::ListChainPresets { chain } => {
            format!("openrig://chains/{}/presets", chain.0)
        }
        QueryKind::ChainQualityReport { chain } => {
            format!("openrig://chains/{}/quality", chain.0)
        }
        QueryKind::GetBlockParams { chain, block } => {
            format!("openrig://chains/{}/blocks/{}/params", chain.0, block.0)
        }
        QueryKind::GetPluginParams { plugin_id } => {
            format!("{URI_PLUGIN_PREFIX}{plugin_id}/params")
        }
        QueryKind::GetPlugin { id } => format!("{URI_PLUGIN_PREFIX}{id}"),
        QueryKind::FindPlugins { query } => format!("{URI_PLUGIN_SEARCH_PREFIX}{query}"),
    }
}

/// Extract `<chain>` from `openrig://chains/<chain>/presets`. Returns
/// `None` for any other URI shape.
fn parse_chain_presets_uri(uri: &str) -> Option<String> {
    uri.strip_prefix("openrig://chains/")
        .and_then(|rest| rest.strip_suffix("/presets"))
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

/// Extract `<chain>` from `openrig://chains/<chain>/latency`. Returns
/// `None` for any other URI shape. #829.
fn parse_chain_latency_uri(uri: &str) -> Option<String> {
    uri.strip_prefix("openrig://chains/")
        .and_then(|rest| rest.strip_suffix("/latency"))
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

/// Extract `<chain>` from `openrig://chains/<chain>/quality`. Returns
/// `None` for any other URI shape. #791.
fn parse_chain_quality_uri(uri: &str) -> Option<String> {
    uri.strip_prefix("openrig://chains/")
        .and_then(|rest| rest.strip_suffix("/quality"))
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
#[path = "resources_tests.rs"]
mod tests;
