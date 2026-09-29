//! Responsibility: mirrors a block model swap on a rig chain into the attached rig.
//!
//! #986 — the new model has to land in the rig preset right away, with only
//! the scene overrides it can no longer take dropped, so the preset never
//! disagrees with the chain; and the live block has to sound like the active
//! scene resolves it (its surviving overrides applied), not like bare defaults.

use domain::ids::{BlockId, ChainId};
use project::rig_sync::sync_synthetic_into_rig;

use crate::local_dispatcher::LocalDispatcher;

impl LocalDispatcher {
    /// After `block` on `chain` took a new model: capture the chain into the
    /// rig (the model swap write-back keeps every scene) and re-resolve the
    /// live block through the active scene, keeping its live `enabled`.
    /// No-op for a non-rig chain or when no rig is attached.
    pub(crate) fn mirror_model_swap_into_rig(&self, chain: &ChainId, block: &BlockId) {
        let Some(input) = chain.0.strip_prefix("rig:") else {
            return;
        };
        let Some(rig) = self.rig.borrow().clone() else {
            return;
        };
        sync_synthetic_into_rig(&mut rig.borrow_mut(), &self.project.borrow());

        let resolved = {
            let rig = rig.borrow();
            rig.inputs.get(input).and_then(|ri| {
                let preset = rig.presets.get(ri.bank.get(&ri.active_preset)?)?;
                preset
                    .apply_scene(ri.active_scene)
                    .into_iter()
                    .find(|b| b.id == *block)
            })
        };
        let Some(resolved) = resolved else {
            return;
        };
        let mut project = self.project.borrow_mut();
        if let Some(live) = project
            .chains
            .iter_mut()
            .find(|c| c.id == *chain)
            .and_then(|c| c.blocks.iter_mut().find(|b| b.id == *block))
        {
            live.kind = resolved.kind;
        }
    }
}
