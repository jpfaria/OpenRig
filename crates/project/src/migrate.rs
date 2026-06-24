//! Legacy chain-based [`Project`] → [`RigProject`] migration (#450).
//!
//! Pure and deterministic ⇒ idempotent. Processing blocks are preserved
//! bit-identical and in order, and `Chain.volume` is carried to
//! `RigPreset.volume`, so migrated audio is identical to the legacy chain.

use crate::block::AudioBlockKind;
use crate::chain::Chain;
use crate::project::Project;
use crate::rig::{RigInput, RigOutput, RigPreset, RigProject};
use std::collections::BTreeMap;

/// Lowercase, non-alphanumeric → `-`, collapsed, trimmed. Empty → `preset`.
fn slug(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_dash = false;
    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    let trimmed = out.trim_matches('-');
    if trimmed.is_empty() {
        "preset".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Transform a legacy [`Project`] (chains with embedded device I/O) into a
/// [`RigProject`] (project-level I/O references + per-input preset banks).
///
/// Device endpoints are NOT carried (model A, #716): the per-machine
/// binding registry is the only home for device/channels, so a migrated
/// project loads with its bindings unset — the user re-selects them in the
/// I/O editor. Each legacy chain becomes one input (`input-{N}`, chain
/// order) with a single-preset bank (`active_preset = 1`). The processing
/// blocks are preserved bit-identical and `Chain.volume` is carried per
/// preset (invariant #10). `routing` is kept **structurally**: each output
/// block on the chain maps to a stable placeholder output (`output-{N}`,
/// first-seen by chain output index) so cross-references still resolve in
/// [`RigProject::validate`]. Pure & idempotent.
pub fn migrate_legacy_project(legacy: &Project) -> RigProject {
    let mut inputs: BTreeMap<String, RigInput> = BTreeMap::new();
    let mut presets: BTreeMap<String, RigPreset> = BTreeMap::new();
    let mut outputs = BTreeMap::new();

    for (i, chain) in legacy.chains.iter().enumerate() {
        let input_name = format!("input-{}", i + 1);
        let preset_name = unique_preset_name(chain, i + 1, &presets);
        let blocks = chain
            .blocks
            .iter()
            .filter(|b| !matches!(b.kind, AudioBlockKind::Input(_) | AudioBlockKind::Output(_)))
            .cloned()
            .collect();
        let chain_routing = resolve_chain_routing(chain, &mut outputs);

        presets.insert(
            preset_name.clone(),
            RigPreset {
                id: preset_name.clone(),
                name: chain.description.clone(),
                blocks,
                scene_params: Vec::new(),
                scenes: BTreeMap::new(),
                volume: chain.volume,
            },
        );
        inputs.insert(
            input_name,
            RigInput {
                label: chain.description.clone(),
                bank: BTreeMap::from([(1, preset_name)]),
                active_preset: 1,
                active_scene: 1,
                routing: chain_routing,
                instrument: chain.instrument.clone(),
                io: String::new(),
                endpoint: String::new(),
                io_binding_ids: Vec::new(),
            },
        );
    }

    RigProject {
        name: legacy.name.clone(),
        inputs,
        outputs,
        presets,
        midi: None,
        chain_order: Vec::new(),
    }
}

/// Unique, deterministic preset name in the shared pool: the description
/// slug, suffixed `-{n}` if that slug is already taken.
fn unique_preset_name(chain: &Chain, n: usize, presets: &BTreeMap<String, RigPreset>) -> String {
    let base = chain
        .description
        .as_deref()
        .map(slug)
        .unwrap_or_else(|| format!("preset-{n}"));
    if presets.contains_key(&base) {
        format!("{base}-{n}")
    } else {
        base
    }
}

/// The output names this chain routes to. Device endpoints are gone
/// (model A, #716) so there is nothing to dedupe by anymore: each output
/// block on the chain registers one structural placeholder output
/// (`output-{N}`, first-seen across all chains) with an unset binding, and
/// the chain routes to those names. This keeps `routing → outputs`
/// cross-references resolvable; the user binds each output to a device
/// later in the I/O editor.
fn resolve_chain_routing(chain: &Chain, outputs: &mut BTreeMap<String, RigOutput>) -> Vec<String> {
    let mut routing = Vec::new();
    for _ in chain.output_blocks() {
        let name = format!("output-{}", outputs.len() + 1);
        outputs.insert(
            name.clone(),
            RigOutput {
                label: None,
                io: String::new(),
                endpoint: String::new(),
            },
        );
        routing.push(name);
    }
    routing
}

#[cfg(test)]
#[path = "migrate_tests.rs"]
mod migrate_tests;
