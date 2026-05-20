//! Legacy chain-based [`Project`] → [`RigProject`] migration (#450).
//!
//! Pure and deterministic ⇒ idempotent. Processing blocks are preserved
//! bit-identical and in order, and `Chain.volume` is carried to
//! `RigPreset.volume`, so migrated audio is identical to the legacy chain.

use crate::block::{AudioBlockKind, InputEntry};
use crate::chain::{Chain, ChainInputMode};
use crate::project::Project;
use crate::rig::{RigInput, RigOutput, RigPreset, RigProject};
use std::collections::BTreeMap;

/// Mono-normalize a capture channel list: a `mono` entry only ever taps
/// ONE physical channel, so `mono [0,1]` (a legacy data quirk) collapses
/// to `[0]`. Other modes keep their channels.
fn norm_channels(mode: &ChainInputMode, channels: &[usize]) -> Vec<usize> {
    if matches!(mode, ChainInputMode::Mono) && channels.len() > 1 {
        channels[..1].to_vec()
    } else {
        channels.to_vec()
    }
}

fn normalize_entry(e: &InputEntry) -> InputEntry {
    InputEntry {
        device_id: e.device_id.clone(),
        mode: e.mode,
        channels: norm_channels(&e.mode, &e.channels),
    }
}

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

/// Transform a legacy [`Project`] (chains with embedded I/O) into a
/// [`RigProject`] (project-level I/O + per-input preset banks).
///
/// Chains are **grouped by capture source** — the set of
/// `(device, mode, mono-normalized channels)` of their input entries.
/// Every chain on the same source becomes one preset in **one input's
/// bank** (one guitar, many songs ⇒ one input + N presets), in chain
/// order; `active_preset = 1`. Distinct sources become distinct inputs
/// (`input-{M}`, first-seen order). Outputs are deduplicated across
/// chains; each input's `routing` is the union of its chains' outputs.
/// `Chain.volume` is carried per preset (invariant #10). The result
/// always satisfies [`RigProject::validate`]; running it twice yields an
/// identical `RigProject`.
pub fn migrate_legacy_project(legacy: &Project) -> RigProject {
    let mut inputs: BTreeMap<String, RigInput> = BTreeMap::new();
    let mut presets: BTreeMap<String, RigPreset> = BTreeMap::new();
    let mut outputs = BTreeMap::new();
    let mut output_order: Vec<((String, String, Vec<usize>), String)> = Vec::new();
    let mut group_order: Vec<(SourceKey, String)> = Vec::new();

    for (i, chain) in legacy.chains.iter().enumerate() {
        let (norm_sources, key) = chain_source(chain);
        let input_name = match group_order.iter().find(|(k, _)| *k == key) {
            Some((_, name)) => name.clone(),
            None => {
                let name = format!("input-{}", group_order.len() + 1);
                group_order.push((key, name.clone()));
                name
            }
        };
        let preset_name = unique_preset_name(chain, i + 1, &presets);
        let blocks = chain
            .blocks
            .iter()
            .filter(|b| !matches!(b.kind, AudioBlockKind::Input(_) | AudioBlockKind::Output(_)))
            .cloned()
            .collect();
        let chain_routing = resolve_chain_routing(chain, &mut outputs, &mut output_order);

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
        match inputs.get_mut(&input_name) {
            Some(input) => {
                let slot = input.bank.keys().max().copied().unwrap_or(0) + 1;
                input.bank.insert(slot, preset_name);
                for r in chain_routing {
                    if !input.routing.contains(&r) {
                        input.routing.push(r);
                    }
                }
            }
            None => {
                inputs.insert(
                    input_name,
                    RigInput {
                        label: chain.description.clone(),
                        sources: norm_sources,
                        bank: BTreeMap::from([(1, preset_name)]),
                        active_preset: 1,
                        active_scene: 1,
                        routing: chain_routing,
                    },
                );
            }
        }
    }

    RigProject {
        name: legacy.name.clone(),
        inputs,
        outputs,
        presets,
        midi: None,
    }
}

/// Grouping key: per input entry, `(device, {:?}mode, channels)` after
/// mono-normalization. Two chains group iff this matches exactly.
type SourceKey = Vec<(String, String, Vec<usize>)>;

/// A chain's mono-normalized capture sources + the [`SourceKey`] derived
/// from them (used to group chains onto the same input).
fn chain_source(chain: &Chain) -> (Vec<InputEntry>, SourceKey) {
    let sources: Vec<InputEntry> = chain
        .input_blocks()
        .into_iter()
        .flat_map(|(_, b)| b.entries.iter().map(normalize_entry).collect::<Vec<_>>())
        .collect();
    let key = sources
        .iter()
        .map(|e| {
            (
                e.device_id.0.clone(),
                format!("{:?}", e.mode),
                e.channels.clone(),
            )
        })
        .collect();
    (sources, key)
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

/// The output names this chain routes to, registering/deduping outputs
/// in the shared pool by `(device, mode, channels)` (first-seen naming).
fn resolve_chain_routing(
    chain: &Chain,
    outputs: &mut BTreeMap<String, RigOutput>,
    output_order: &mut Vec<((String, String, Vec<usize>), String)>,
) -> Vec<String> {
    let mut routing = Vec::new();
    for (_, out) in chain.output_blocks() {
        for entry in &out.entries {
            let okey = (
                entry.device_id.0.clone(),
                format!("{:?}", entry.mode),
                entry.channels.clone(),
            );
            let name = match output_order.iter().find(|(k, _)| *k == okey) {
                Some((_, name)) => name.clone(),
                None => {
                    let name = format!("output-{}", output_order.len() + 1);
                    output_order.push((okey, name.clone()));
                    outputs.insert(
                        name.clone(),
                        RigOutput {
                            label: None,
                            entry: entry.clone(),
                        },
                    );
                    name
                }
            };
            if !routing.contains(&name) {
                routing.push(name);
            }
        }
    }
    routing
}

#[cfg(test)]
#[path = "migrate_tests.rs"]
mod migrate_tests;
