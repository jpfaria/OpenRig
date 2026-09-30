//! Responsibility: serializes one chain's own mixer faders for a read.
//! #1007: `{"chain", "di_gain_db", "strips": [{id, gain_db, muted}]}` — the
//! chain's own fader on each strip it plays through (inputs first), read
//! parity for the chain mixer commands. MCP serves it as
//! `openrig://chains/{chain}/mixer`.

use domain::ids::ChainId;
use domain::io_binding::IoBinding;
use project::project::Project;

use crate::chain_fader_view::chain_fader_views;

pub fn chain_mixer_json(
    project: &Project,
    registry: &[IoBinding],
    chain: &ChainId,
) -> Result<String, String> {
    let c = project
        .chains
        .iter()
        .find(|c| &c.id == chain)
        .ok_or_else(|| format!("chain not found: {}", chain.0))?;
    let strips: Vec<serde_json::Value> = chain_fader_views(c, registry)
        .into_iter()
        .map(|view| {
            serde_json::json!({
                "id": view.strip,
                "gain_db": view.gain_db,
                "muted": view.muted,
            })
        })
        .collect();
    Ok(serde_json::json!({
        "chain": c.id.0,
        "di_gain_db": c.mix.di_gain_db,
        "strips": strips,
    })
    .to_string())
}

#[cfg(test)]
#[path = "query_chain_mixer_tests.rs"]
mod tests;
