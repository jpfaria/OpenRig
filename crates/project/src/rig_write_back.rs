//! Responsibility: writes a chain's edited state back into the rig it came from.
//!
//! Split out of `rig_methods.rs` (#873).

use crate::block::{AudioBlock, AudioBlockKind};
use crate::rig::RigProject;
use domain::value_objects::ParameterValue;
use std::collections::BTreeMap;

impl RigProject {
    /// Persist a block/param edit made on the projected synthetic chain
    /// back into the active preset, **per scene (snapshot semantics)**:
    /// the edit is captured into the input's *active scene* only, so each
    /// scene keeps its own values. `preset.blocks` stays the factory
    /// template; a float param / bypass that differs from the template is
    /// stored as that scene's override (and the key auto-marked as a
    /// scene-param so `apply_scene` applies it). A value back at the
    /// template clears the override. Non-float params (Bool/Int/String)
    /// cannot live in the f32 scene diff — they are written into the
    /// preset base itself, shared by every scene (issue #690). No-op if
    /// input/preset is unknown.
    pub fn write_back_processing_blocks(
        &mut self,
        input: &str,
        blocks: Vec<crate::block::AudioBlock>,
    ) {
        let Some((preset_name, scene_idx)) = self.inputs.get(input).and_then(|ri| {
            ri.bank
                .get(&ri.active_preset)
                .cloned()
                .map(|n| (n, ri.active_scene))
        }) else {
            return;
        };
        let Some(preset) = self.presets.get_mut(&preset_name) else {
            return;
        };

        // Factory template, indexed by block id (immutable diff base).
        let base: BTreeMap<String, AudioBlock> = preset
            .blocks
            .iter()
            .map(|b| (b.id.0.clone(), b.clone()))
            .collect();

        let mut set_param: Vec<(String, f32)> = Vec::new();
        let mut clear_param: Vec<String> = Vec::new();
        let mut set_base_param: Vec<(String, String, ParameterValue)> = Vec::new();
        let mut set_port_target: Vec<(String, AudioBlockKind)> = Vec::new();
        let mut set_bypass: Vec<(String, bool)> = Vec::new();
        let mut clear_bypass: Vec<String> = Vec::new();

        for edited in &blocks {
            let bid = edited.id.0.clone();
            let Some(base_blk) = base.get(&bid) else {
                continue;
            };
            if edited.enabled != base_blk.enabled {
                set_bypass.push((bid.clone(), !edited.enabled));
            } else {
                clear_bypass.push(bid.clone());
            }
            // #85: a port carries no params — WHERE it points is its whole
            // state, and it lives in the block kind, not in a `ParameterSet`.
            // A scene can only hold f32 overrides, so re-pointing a port is a
            // preset-level edit; without this the new E/S was dropped here and
            // the port came back on its old binding after save + reopen.
            if matches!(
                edited.kind,
                AudioBlockKind::Input(_) | AudioBlockKind::Output(_) | AudioBlockKind::Insert(_)
            ) && edited.kind != base_blk.kind
            {
                set_port_target.push((bid.clone(), edited.kind.clone()));
            }
            let pair = match (&edited.kind, &base_blk.kind) {
                (AudioBlockKind::Core(e), AudioBlockKind::Core(b)) => Some((&e.params, &b.params)),
                (AudioBlockKind::Nam(e), AudioBlockKind::Nam(b)) => Some((&e.params, &b.params)),
                _ => None,
            };
            if let Some((ep, bp)) = pair {
                for (pid, val) in &ep.values {
                    match val {
                        ParameterValue::Float(v) => {
                            let key = format!("{bid}.{pid}");
                            if bp.get_f32(pid) != Some(*v) {
                                set_param.push((key, *v));
                            } else {
                                clear_param.push(key);
                            }
                        }
                        // Scenes can only carry f32 overrides (Helix
                        // snapshot rule), so a Bool/Int/String/enum edit
                        // is preset-level: write it into the base
                        // template, shared by every scene. Issue #690 —
                        // the NAM noise-gate toggle was silently dropped
                        // here and reverted on save+reload.
                        other => {
                            if bp.get(pid) != Some(other) {
                                set_base_param.push((bid.clone(), pid.clone(), other.clone()));
                            }
                        }
                    }
                }
            }
        }

        for (bid, kind) in set_port_target {
            if let Some(block) = preset.blocks.iter_mut().find(|b| b.id.0 == bid) {
                block.kind = kind;
            }
        }

        for (bid, pid, val) in set_base_param {
            let params =
                preset
                    .blocks
                    .iter_mut()
                    .find(|b| b.id.0 == bid)
                    .and_then(|b| match &mut b.kind {
                        AudioBlockKind::Core(c) => Some(&mut c.params),
                        AudioBlockKind::Nam(n) => Some(&mut n.params),
                        _ => None,
                    });
            if let Some(params) = params {
                params.insert(pid, val);
            }
        }

        let scene = preset.scenes.entry(scene_idx).or_default();
        for (b, v) in &set_bypass {
            scene.bypass.insert(b.clone(), *v);
        }
        for b in &clear_bypass {
            scene.bypass.remove(b);
        }
        for (k, v) in &set_param {
            scene.params.insert(k.clone(), *v);
        }
        for k in &clear_param {
            scene.params.remove(k);
        }
        for (k, _) in &set_param {
            if !preset.scene_params.contains(k) {
                preset.scene_params.push(k.clone());
            }
        }
    }

    /// Rewrite the active preset's block LIST when `blocks` is a
    /// **structural** change (different block ids/order/count/model vs the
    /// preset's base) — e.g. a preset was loaded over the slot, or blocks
    /// were added/removed/reordered. `write_back_processing_blocks` is
    /// diff-only (param/bypass keyed by block id) and silently drops such
    /// edits, so they never persisted.
    ///
    /// #986: the list follows the chain, but the preset keeps what it owns.
    /// A block it already had (same id, same model) keeps its base — not the
    /// scene-applied live copy, which baked the active scene into every
    /// scene — and every scene and `scene-params` entry survives except the
    /// ones of blocks that are gone (and, for a block whose model changed,
    /// the params the new model lacks). The caller runs the per-scene diff
    /// afterwards, so the active scene's edits are still captured. Returns
    /// `true` when the list changed; `false` if the input/preset is unknown
    /// or the structure is identical.
    pub fn replace_preset_blocks_if_structural(
        &mut self,
        input: &str,
        blocks: &[AudioBlock],
    ) -> bool {
        let Some(preset_name) = self
            .inputs
            .get(input)
            .and_then(|ri| ri.bank.get(&ri.active_preset).cloned())
        else {
            return false;
        };
        let Some(preset) = self.presets.get_mut(&preset_name) else {
            return false;
        };
        // "Same structure" requires both the same id AND the same model
        // identity. A `ReplaceBlockModel` keeps the id but changes the model
        // (#627); comparing ids alone classified that as a non-structural
        // per-scene diff, so the swapped model was never written into the
        // preset base and reverted on reload. Model identity excludes params,
        // so genuine param/bypass edits still take the diff-only path below.
        let same_structure =
            preset.blocks.len() == blocks.len()
                && preset.blocks.iter().zip(blocks).all(|(a, b)| {
                    a.id == b.id && a.kind.model_identity() == b.kind.model_identity()
                });
        if same_structure {
            return false;
        }
        let owned = |live: &AudioBlock| {
            preset
                .blocks
                .iter()
                .find(|b| b.id == live.id && b.kind.model_identity() == live.kind.model_identity())
                .cloned()
        };
        let next: Vec<AudioBlock> = blocks
            .iter()
            .map(|live| owned(live).unwrap_or_else(|| live.clone()))
            .collect();
        let swapped: BTreeMap<&str, &AudioBlock> = next
            .iter()
            .filter(|b| {
                preset.blocks.iter().any(|old| {
                    old.id == b.id && old.kind.model_identity() != b.kind.model_identity()
                })
            })
            .map(|b| (b.id.0.as_str(), b))
            .collect();
        let has_block = |id: &str| next.iter().any(|b| b.id.0 == id);
        let keeps_param = |key: &str| {
            next.iter().any(|b| {
                let Some(param) = key.strip_prefix(&format!("{}.", b.id.0)) else {
                    return false;
                };
                match swapped.get(b.id.0.as_str()) {
                    Some(new) => match &new.kind {
                        AudioBlockKind::Core(c) => c.params.get(param).is_some(),
                        AudioBlockKind::Nam(n) => n.params.get(param).is_some(),
                        _ => false,
                    },
                    None => true,
                }
            })
        };
        for scene in preset.scenes.values_mut() {
            scene.bypass.retain(|id, _| has_block(id));
            scene.params.retain(|key, _| keeps_param(key));
        }
        preset.scene_params.retain(|key| keeps_param(key));
        preset.blocks = next;
        true
    }

    /// Persist the chain volume edited on the projected synthetic chain
    /// back into the active preset, **per active scene** (snapshot
    /// semantics — mirrors [`Self::write_back_processing_blocks`]). A
    /// value equal to the preset volume clears the per-scene override
    /// (no stale snapshot); anything else is stored for that scene only.
    /// No-op if the input/preset is unknown.
    pub fn write_back_chain_volume(&mut self, input: &str, volume: f32) {
        let Some((preset_name, scene_idx)) = self.inputs.get(input).and_then(|ri| {
            ri.bank
                .get(&ri.active_preset)
                .cloned()
                .map(|n| (n, ri.active_scene))
        }) else {
            return;
        };
        let Some(preset) = self.presets.get_mut(&preset_name) else {
            return;
        };
        let base = preset.volume;
        let scene = preset.scenes.entry(scene_idx).or_default();
        scene.volume = if (volume - base).abs() < f32::EPSILON {
            None
        } else {
            Some(volume)
        };
    }
}
