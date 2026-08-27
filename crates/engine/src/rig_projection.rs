//! Responsibility: projects a rig into the chains the engine runs.

use crate::rig_runtime_normalize::duplicates_chain_binding;
use domain::ids::{BlockId, ChainId};
use project::block::{AudioBlock, AudioBlockKind, InputBlock, OutputBlock};
use project::chain::Chain;
use project::rig::RigProject;

/// Project each input of a `RigProject` onto one synthetic legacy `Chain`:
/// `Input(sources)` → active-preset processing blocks → `Output(routing)`.
///
/// Deterministic. By default ordered alphabetically by input name; when
/// `rig.chain_order` is non-empty (set by [`project::rig_sync::sync_synthetic_into_rig`]
/// to persist a user reorder, issue #502) the projection honours that
/// order, with any inputs missing from `chain_order` appended in
/// alphabetical order so a freshly-added input still shows up.
/// Each chain gets a distinct `ChainId` (`rig:<input-name>`) so the
/// existing runtime graph keeps the inputs in fully isolated runtimes
/// (invariant #4). Inputs whose active preset is absent are skipped
/// (a validated `RigProject` never hits this).
pub fn rig_to_chains(rig: &RigProject) -> Vec<Chain> {
    let mut chains = Vec::with_capacity(rig.inputs.len());
    for name in ordered_input_names(rig) {
        let Some(input) = rig.inputs.get(&name) else {
            continue;
        };
        let Some(preset_name) = input.bank.get(&input.active_preset) else {
            continue;
        };
        let Some(preset) = rig.presets.get(preset_name) else {
            continue;
        };

        let mut blocks = Vec::with_capacity(preset.blocks.len() + 2);
        // #716: a binding-bound chain (io_binding_ids) discovers its I/O from
        // the registry at runtime — do NOT synthesize device Input/Output
        // blocks, or they show in the chain strip (the "monster") and double
        // routing. Legacy per-block (io/endpoint/entries) chains still
        // synthesize them.
        let bound = !input.io_binding_ids.is_empty();
        if !bound {
            blocks.push(AudioBlock {
                id: BlockId(format!("rig:{name}:in")),
                enabled: true,
                kind: AudioBlockKind::Input(InputBlock {
                    model: "standard".to_string(),
                    // Propagate the binding reference stored on RigInput (#716).
                    io: input.io.clone(),
                    endpoint: input.endpoint.clone(),
                }),
            });
        }
        blocks.extend(preset.apply_scene(input.active_scene));
        let routed_outputs: Vec<_> = input
            .routing
            .iter()
            .filter_map(|t| rig.outputs.get(t).map(|o| (t, o)))
            .collect();
        if !bound && !routed_outputs.is_empty() {
            // Propagate io/endpoint from the first routed output that carries
            // a binding reference. Multiple routing entries all share the same
            // binding reference in the current model (one binding per chain).
            let (first_io, first_ep) = routed_outputs
                .first()
                .map(|(_, o)| (o.io.clone(), o.endpoint.clone()))
                .unwrap_or_default();
            blocks.push(AudioBlock {
                id: BlockId(format!("rig:{name}:out")),
                enabled: true,
                kind: AudioBlockKind::Output(OutputBlock {
                    model: "standard".to_string(),
                    io: first_io,
                    endpoint: first_ep,
                }),
            });
        }

        // #716: the chain's head/tail I/O is the system binding, never blocks.
        // Drop the legacy leftovers that duplicate it; a port pointing at
        // another E/S is a mid port the user placed (#85) and stays.
        if bound {
            blocks.retain(|b| !duplicates_chain_binding(b, &input.io_binding_ids));
        }

        chains.push(Chain {
            id: ChainId(format!("rig:{name}")),
            // The chain title is the *input* label (the chain's own
            // name); the preset name lives next to the preset combobox
            // and must not bleed into the chain title — otherwise
            // switching preset visibly renames the chain.
            description: Some(
                input
                    .label
                    .clone()
                    .or_else(|| preset.name.clone())
                    .unwrap_or_else(|| project::rig::humanize_preset_label(preset_name)),
            ),
            instrument: input.instrument.clone(),
            enabled: true,
            // Invariant #10: carry the preset's volume (legacy migration
            // preserved Chain.volume → RigPreset.volume). Hardcoding 100
            // would silently retune every preset on the rig path. The
            // active scene may override it (#436); a scene with no
            // override resolves to `preset.volume` ⇒ audibly unchanged
            // for every pre-#436 project (back-compat).
            volume: preset.scene_volume(input.active_scene),
            io_binding_ids: input.io_binding_ids.clone(),
            blocks,
            di_output: None,
            loopers: input.loopers.clone(),
        });
    }
    chains
}

/// Build the iteration order for [`rig_to_chains`]: honour
/// `rig.chain_order` first (filtering to names that actually exist in
/// `rig.inputs`, dropping duplicates), then append any remaining inputs
/// in alphabetical order so a newly-added input still surfaces even when
/// the persisted order pre-dates it.
fn ordered_input_names(rig: &RigProject) -> Vec<String> {
    use std::collections::BTreeSet;
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut out: Vec<String> = Vec::with_capacity(rig.inputs.len());
    for name in &rig.chain_order {
        if rig.inputs.contains_key(name) && seen.insert(name.clone()) {
            out.push(name.clone());
        }
    }
    for name in rig.inputs.keys() {
        if seen.insert(name.clone()) {
            out.push(name.clone());
        }
    }
    out
}

/// Project a `RigProject` onto a synthetic **legacy** [`Project`]: **every**
/// input becomes a `Chain` so the existing GUI shows them all, and each
/// chain's `enabled` flag reflects whether that input is in `enabled`.
/// Enabling is the USER's action (in memory, at runtime) — nothing is
/// auto-started; pass an empty set to load everything OFF. Drives the
/// proven cpal/runtime path with zero new audio code; `device_settings`
/// is empty (per-machine settings live elsewhere).
pub fn rig_to_legacy_project(
    rig: &RigProject,
    enabled: &std::collections::BTreeSet<String>,
) -> project::project::Project {
    let chains = rig_to_chains(rig)
        .into_iter()
        .map(|mut c| {
            let on =
                c.id.0
                    .strip_prefix("rig:")
                    .is_some_and(|name| enabled.contains(name));
            c.enabled = on;
            c
        })
        .collect();
    project::project::Project {
        name: rig.name.clone(),
        device_settings: Vec::new(),
        chains,
        // #513: project-owned MIDI bindings travel with `.openrig`. A rig-
        // projected `Project` is a synthetic view — bindings live on the
        // source RigProject if needed, so the projection starts with none.
        midi: None,
    }
}

/// Apply a preset and/or scene change to one input of `rig` **in place**
/// and return that input's freshly-projected synthetic [`Chain`] (the
/// caller upserts it through the proven runtime path — zero new audio
/// code). `preset_slot`/`scene` are applied only when `Some`. Invalid
/// (unknown input, bank slot absent, scene ∉ `1..=8`) ⇒ **no mutation**
/// and `None`, so the GUI can ignore a bad request without corrupting
/// state. `None` is also returned if the resulting preset is unbuildable.
pub fn switch_and_project_input(
    rig: &mut RigProject,
    input: &str,
    preset_slot: Option<usize>,
    scene: Option<usize>,
) -> Option<Chain> {
    {
        // Validate everything before touching state (no partial mutation).
        let ri = rig.inputs.get(input)?;
        if let Some(s) = preset_slot {
            if !ri.bank.contains_key(&s) {
                return None;
            }
        }
        if let Some(sc) = scene {
            if !(1..=8).contains(&sc) {
                return None;
            }
        }
    }
    let ri = rig.inputs.get_mut(input)?;
    if let Some(s) = preset_slot {
        ri.active_preset = s;
    }
    if let Some(sc) = scene {
        ri.active_scene = sc;
    }
    let id = ChainId(format!("rig:{input}"));
    rig_to_chains(rig).into_iter().find(|c| c.id == id)
}
