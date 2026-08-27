//! Responsibility: says whether a rig is internally consistent.
//!
//! Split out of `rig_methods.rs` (#873).

use crate::block::{duplicates_chain_binding, AudioBlockKind};
use crate::rig::RigProject;

impl RigProject {
    /// Validate cross-references in the rig model.
    ///
    /// Rules (closed in #436 / scoped by #449; device-channel conflicts
    /// moved to runtime activation in #716):
    /// 1. every `bank` value must name a preset in `presets`;
    /// 2. each input's `active_preset` must be a key in its own `bank`;
    /// 3. each input's `active_scene` ∈ `1..=8`;
    /// 4. no preset may contain an `Input`/`Output` block;
    /// 5. every `routing` target must name an `outputs` entry.
    ///
    /// Device endpoints no longer live in the model (model A, #716), so any
    /// capture/output exclusivity is enforced by the engine at runtime
    /// against the per-machine binding registry, not by this static model.
    pub fn validate(&self) -> Result<(), String> {
        for (name, input) in &self.inputs {
            for (idx, preset_name) in &input.bank {
                if !self.presets.contains_key(preset_name) {
                    return Err(format!(
                        "input '{name}' bank slot {idx} references unknown preset '{preset_name}'"
                    ));
                }
            }
            if !input.bank.contains_key(&input.active_preset) {
                return Err(format!(
                    "input '{name}' active-preset {} is not a slot in its bank",
                    input.active_preset
                ));
            }
            if !(1..=8).contains(&input.active_scene) {
                return Err(format!(
                    "input '{name}' active-scene {} out of range 1..=8",
                    input.active_scene
                ));
            }
            for target in &input.routing {
                if !self.outputs.contains_key(target) {
                    return Err(format!(
                        "input '{name}' routes to unknown output '{target}'"
                    ));
                }
            }
        }
        for (name, preset) in &self.presets {
            // #85: a preset carries the blocks the user placed, and a mid
            // `Input`/`Output` port is one of them — the same kind of thing an
            // `Insert` is, which this rule has always accepted. What it must
            // keep rejecting is the legacy HEAD/TAIL leftover: a port bound to a
            // binding ITS OWN chain already carries (#716), which duplicates
            // that chain's I/O and starves the device. Judged against the chains
            // that actually play this preset — an E/S another chain carries is
            // an aux send, and rejecting it made the app refuse its own file.
            let carriers: Vec<&Vec<String>> = self
                .inputs
                .values()
                .filter(|input| input.bank.values().any(|slot| slot == name))
                .map(|input| &input.io_binding_ids)
                .collect();
            for block in &preset.blocks {
                let io = match &block.kind {
                    AudioBlockKind::Input(b) => &b.io,
                    AudioBlockKind::Output(b) => &b.io,
                    AudioBlockKind::Nam(_)
                    | AudioBlockKind::Core(_)
                    | AudioBlockKind::Select(_)
                    | AudioBlockKind::Insert(_) => continue,
                };
                if carriers
                    .iter()
                    .any(|bindings| duplicates_chain_binding(block, bindings))
                {
                    return Err(format!(
                        "preset '{name}' contains an I/O block ({}) bound to '{io}', a \
                         binding its own chain already carries; that duplicates the \
                         chain's own I/O",
                        block.kind.label()
                    ));
                }
            }
            for (idx, scene) in &preset.scenes {
                if !(1..=8).contains(idx) {
                    return Err(format!("preset '{name}' scene {idx} out of range 1..=8"));
                }
                for key in scene.params.keys() {
                    if !preset.scene_params.contains(key) {
                        return Err(format!(
                            "preset '{name}' scene {idx} sets '{key}' which is not a marked scene-param"
                        ));
                    }
                }
            }
        }
        Ok(())
    }
}
