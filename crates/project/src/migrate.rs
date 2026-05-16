//! Legacy chain-based [`Project`] → [`RigProject`] migration (#450).
//!
//! Pure and deterministic ⇒ idempotent. Processing blocks are preserved
//! bit-identical and in order, and `Chain.volume` is carried to
//! `RigPreset.volume`, so migrated audio is identical to the legacy chain.

use crate::block::AudioBlockKind;
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

/// Transform a legacy [`Project`] (chains with embedded I/O) into a
/// [`RigProject`] (project-level I/O + per-input preset banks).
///
/// Each chain becomes one input (`input-{N}`), one preset in the shared pool,
/// and a single bank slot. Outputs are deduplicated across chains. The result
/// always satisfies [`RigProject::validate`]; running it twice yields an
/// identical `RigProject`.
pub fn migrate_legacy_project(legacy: &Project) -> RigProject {
    let mut inputs = BTreeMap::new();
    let mut presets = BTreeMap::new();
    let mut outputs = BTreeMap::new();
    // First-seen order for deterministic `output-{M}` naming + dedup lookup.
    let mut output_order: Vec<((String, String, Vec<usize>), String)> = Vec::new();

    for (i, chain) in legacy.chains.iter().enumerate() {
        let n = i + 1;

        // Unique, deterministic preset name.
        let base = chain
            .description
            .as_deref()
            .map(slug)
            .unwrap_or_else(|| format!("preset-{n}"));
        let preset_name = if presets.contains_key(&base) {
            format!("{base}-{n}")
        } else {
            base
        };

        let sources = chain
            .input_blocks()
            .into_iter()
            .flat_map(|(_, b)| b.entries.clone())
            .collect();

        let blocks = chain
            .blocks
            .iter()
            .filter(|b| !matches!(b.kind, AudioBlockKind::Input(_) | AudioBlockKind::Output(_)))
            .cloned()
            .collect();

        // Dedup outputs across chains by (device, mode, channels).
        let mut routing = Vec::new();
        for (_, out) in chain.output_blocks() {
            for entry in &out.entries {
                let key = (
                    entry.device_id.0.clone(),
                    format!("{:?}", entry.mode),
                    entry.channels.clone(),
                );
                let name = match output_order.iter().find(|(k, _)| *k == key) {
                    Some((_, name)) => name.clone(),
                    None => {
                        let name = format!("output-{}", output_order.len() + 1);
                        output_order.push((key, name.clone()));
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

        presets.insert(
            preset_name.clone(),
            RigPreset {
                blocks,
                scene_params: Vec::new(),
                scenes: BTreeMap::new(),
                volume: chain.volume,
            },
        );
        inputs.insert(
            format!("input-{n}"),
            RigInput {
                label: chain.description.clone(),
                sources,
                bank: BTreeMap::from([(1, preset_name)]),
                active_preset: 1,
                active_scene: 1,
                routing,
            },
        );
    }

    RigProject {
        name: legacy.name.clone(),
        inputs,
        outputs,
        presets,
    }
}

#[cfg(test)]
#[path = "migrate_tests.rs"]
mod migrate_tests;
