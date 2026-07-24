//! #436 architectural fix: the rig-nav handler. This is exactly what
//! the GUI's `chain_rig_nav_wiring::reproject` closure used to do by
//! hand (capture pending edits → apply the preset/scene change →
//! re-project the synthetic chain), now behind the Command/dispatcher
//! so MIDI/MCP/GUI all share one path and the UI carries no business
//! logic. No audio code; pure model + the proven `engine` projection.

use anyhow::{anyhow, Result};

use project::block::{AudioBlock, AudioBlockKind};
use project::rig_command::{rig_command_from_scene, rig_command_from_select, RigCommand};
use project::rig_sync::sync_synthetic_into_rig;

use crate::command::{Command, RigNavKind, SelectionCommand};
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;

impl LocalDispatcher {
    pub(crate) fn handle_rig_nav(&self, cmd: Command) -> Result<Vec<Event>> {
        let Command::Selection(SelectionCommand::ApplyRigNav { chain, kind }) = cmd else {
            unreachable!("handle_rig_nav received non-rig-nav command: {cmd:?}");
        };
        // Non-rig chain or no rig attached ⇒ ignore (mirrors the old
        // GUI closure, which silently returned).
        let Some(input) = chain.0.strip_prefix("rig:").map(str::to_string) else {
            return Ok(vec![]);
        };
        let Some(rig) = self.rig.borrow().clone() else {
            return Ok(vec![]);
        };

        // The GUI sentinel int → the existing pure RigCommand mapping.
        let rig_cmd = match kind {
            RigNavKind::Preset(n) => rig_command_from_select(&input, n),
            RigNavKind::Scene(n) => rig_command_from_scene(&input, n),
            // Footswitch next/previous: resolve the relative delta
            // against the live rig (wrap math is the single source in
            // `RigProject`), then reuse the proven absolute switch.
            RigNavKind::StepPreset(delta) => {
                let pos = rig
                    .borrow()
                    .step_preset(&input, delta)
                    .ok_or_else(|| anyhow!("error-invalid-chain"))?;
                RigCommand::SwitchPreset {
                    input: input.clone(),
                    position: pos,
                }
            }
            RigNavKind::StepScene(delta) => {
                let scene = rig
                    .borrow()
                    .step_scene(&input, delta)
                    .ok_or_else(|| anyhow!("error-invalid-chain"))?;
                RigCommand::SwitchScene {
                    input: input.clone(),
                    scene,
                }
            }
        };

        // 1. Capture pending block/param/volume/source edits on the
        //    synthetic chains so switching never discards them.
        sync_synthetic_into_rig(&mut rig.borrow_mut(), &self.project.borrow());

        // 2. Apply the preset/scene change to the rig.
        rig_cmd
            .apply(&mut rig.borrow_mut())
            .ok_or_else(|| anyhow!("invalid rig-nav command"))?;

        // 3. Re-project the active state through the proven engine path
        //    (engine is untouched — pure reuse).
        let rebuilt = engine::rig_runtime::switch_and_project_input(
            &mut rig.borrow_mut(),
            &input,
            None,
            None,
        )
        .ok_or_else(|| anyhow!("error-invalid-chain"))?;

        // 4. Swap the rebuilt chain in place (same id ⇒ index/alignment
        //    kept; preserve the user's enabled flag and I/O endpoints).
        //    Rig nav switches preset/scene -- it does NOT change audio
        //    routing. The user's `save_chain_input_endpoints` /
        //    `save_chain_output_endpoints` writes the I/O blocks to the
        //    legacy chain only; rig.outputs is typically empty, so a naive
        //    swap would wipe the user's output sink and freeze the meter
        //    at -120 dBFS on every scene change.
        {
            let mut proj = self.project.borrow_mut();
            if let Some(slot) = proj.chains.iter_mut().find(|c| c.id == chain) {
                let was_enabled = slot.enabled;
                let preserved_inputs: Vec<AudioBlock> = slot
                    .blocks
                    .iter()
                    .filter(|b| matches!(b.kind, AudioBlockKind::Input(_)))
                    .cloned()
                    .collect();
                let preserved_outputs: Vec<AudioBlock> = slot
                    .blocks
                    .iter()
                    .filter(|b| matches!(b.kind, AudioBlockKind::Output(_)))
                    .cloned()
                    .collect();
                let mut rebuilt = rebuilt;
                rebuilt.blocks.retain(|b| {
                    !matches!(b.kind, AudioBlockKind::Input(_) | AudioBlockKind::Output(_))
                });
                let mut final_blocks = preserved_inputs;
                final_blocks.append(&mut rebuilt.blocks);
                final_blocks.extend(preserved_outputs);
                rebuilt.blocks = final_blocks;
                *slot = rebuilt;
                slot.enabled = was_enabled;
            }
        }

        Ok(vec![Event::ChainReloaded { chain }, Event::ProjectMutated])
    }

    /// #436: fold pending synthetic-chain edits back into the rig. Was
    /// called by hand in the GUI save path — now a Command so the UI
    /// carries no model mutation. No-op for non-rig sessions.
    pub(crate) fn handle_capture_rig_edits(&self) -> Result<Vec<Event>> {
        let Some(rig) = self.rig.borrow().clone() else {
            return Ok(vec![]);
        };
        sync_synthetic_into_rig(&mut rig.borrow_mut(), &self.project.borrow());
        Ok(vec![Event::ProjectMutated])
    }

    /// #436: rename the chain's active preset (the `name` the select
    /// shows). No-op for non-rig chains / no rig attached.
    pub(crate) fn handle_rename_rig_preset(&self, cmd: Command) -> Result<Vec<Event>> {
        let Command::Selection(SelectionCommand::RenameRigPreset { chain, name }) = cmd else {
            unreachable!("handle_rename_rig_preset got {cmd:?}");
        };
        let Some(input) = chain.0.strip_prefix("rig:") else {
            return Ok(vec![]);
        };
        let Some(rig) = self.rig.borrow().clone() else {
            return Ok(vec![]);
        };
        {
            let mut rig = rig.borrow_mut();
            let Some(key) = rig
                .inputs
                .get(input)
                .and_then(|ri| ri.bank.get(&ri.active_preset).cloned())
            else {
                return Ok(vec![]);
            };
            if let Some(preset) = rig.presets.get_mut(&key) {
                preset.name = Some(name);
            }
        }
        Ok(vec![Event::ChainReloaded { chain }, Event::ProjectMutated])
    }
}
