//! `RigProject` → engine bridge (#451).
//!
//! The engine only understands the legacy [`Chain`]. Rather than teach the
//! audio thread a new model, each [`RigInput`] (with its active preset and
//! routed outputs) is projected onto **one synthetic [`Chain`]**, then fed
//! through the existing, proven `build_runtime_graph` / per-input-runtime
//! machinery. Isolation (#4) is already enforced there — one runtime per
//! input, distinct `ChainId` per input. Pure and hardware-free.

use domain::ids::{BlockId, ChainId};
use project::block::{AudioBlock, AudioBlockKind, InputBlock, OutputBlock};
use project::chain::Chain;
use project::rig::RigProject;

/// Project each input of a `RigProject` onto one synthetic legacy `Chain`:
/// `Input(sources)` → active-preset processing blocks → `Output(routing)`.
///
/// Deterministic, ordered by input name. Each chain gets a distinct
/// `ChainId` (`rig:<input-name>`) so the existing runtime graph keeps the
/// inputs in fully isolated runtimes (invariant #4). Inputs whose active
/// preset is absent are skipped (a validated `RigProject` never hits this).
pub fn rig_to_chains(rig: &RigProject) -> Vec<Chain> {
    let mut chains = Vec::with_capacity(rig.inputs.len());
    for (name, input) in &rig.inputs {
        let Some(preset_name) = input.bank.get(&input.active_preset) else {
            continue;
        };
        let Some(preset) = rig.presets.get(preset_name) else {
            continue;
        };

        let mut blocks = Vec::with_capacity(preset.blocks.len() + 2);
        blocks.push(AudioBlock {
            id: BlockId(format!("rig:{name}:in")),
            enabled: true,
            kind: AudioBlockKind::Input(InputBlock {
                model: "standard".to_string(),
                entries: input.sources.clone(),
            }),
        });
        blocks.extend(preset.blocks.iter().cloned());
        let routed: Vec<_> = input
            .routing
            .iter()
            .filter_map(|t| rig.outputs.get(t))
            .map(|o| o.entry.clone())
            .collect();
        if !routed.is_empty() {
            blocks.push(AudioBlock {
                id: BlockId(format!("rig:{name}:out")),
                enabled: true,
                kind: AudioBlockKind::Output(OutputBlock {
                    model: "standard".to_string(),
                    entries: routed,
                }),
            });
        }

        chains.push(Chain {
            id: ChainId(format!("rig:{name}")),
            description: input.label.clone().or_else(|| Some(name.clone())),
            instrument: block_core::DEFAULT_INSTRUMENT.to_string(),
            enabled: true,
            volume: 100.0,
            blocks,
        });
    }
    chains
}

#[cfg(test)]
#[path = "rig_runtime_tests.rs"]
mod tests;
