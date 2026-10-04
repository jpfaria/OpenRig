//! Responsibility: applies the project tempo to the tempo-synced blocks of the project.
//! The project has one tempo (#1050), like a band: it drives every synced
//! delay and modulation, the metronome and the drums, and it is saved in
//! `project.yaml`. A preset never carries one, so a preset load never
//! changes it.
//!
//! Retiming happens here, on the control thread: the synced `time_ms` /
//! `rate_hz` values are rewritten in the project and each changed chain is
//! synced through the same door a knob turn uses. The audio thread only ever
//! sees the resulting numbers.

use anyhow::Result;

use block_core::tempo_sync::{sync_path_for_value, SYNC_OFF};
use domain::ids::ChainId;
use domain::value_objects::ParameterValue;
use feature_dsp::metronome::{BPM_DEFAULT, BPM_MAX, BPM_MIN};
use project::block::{AudioBlock, AudioBlockKind};
use project::tempo_retime::retime_blocks;

use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;

impl LocalDispatcher {
    /// Set the project tempo: the metronome and the drums run at it, it is
    /// stored in the attached rig (so it saves with the project) and every
    /// synced block is retimed.
    pub(crate) fn set_project_bpm(&self, bpm: f32) -> Result<Vec<Event>> {
        let bpm = bpm.clamp(BPM_MIN, BPM_MAX);
        let mut events = vec![Event::MetronomeBpmChanged { bpm }];
        events.extend(self.run_clocks_at(bpm));
        if let Some(rig) = self.rig.borrow().clone() {
            rig.borrow_mut().bpm = Some(bpm);
            events.push(Event::ProjectMutated);
        }
        events.extend(self.retime_chains(bpm)?);
        Ok(events)
    }

    /// A project was attached: run everything at its tempo (the default for
    /// a project saved without one).
    pub(crate) fn adopt_project_tempo(&self) {
        let Some(rig) = self.rig.borrow().clone() else {
            return;
        };
        let bpm = rig
            .borrow()
            .bpm
            .unwrap_or(BPM_DEFAULT)
            .clamp(BPM_MIN, BPM_MAX);
        self.run_clocks_at(bpm);
        let _ = self.retime_chains(bpm);
    }

    /// The metronome and the drums both tick at `bpm`.
    fn run_clocks_at(&self, bpm: f32) -> Vec<Event> {
        self.metronome_state()
            .borrow_mut()
            .update_settings(|settings| settings.bpm = bpm);
        self.push_metronome_settings();
        self.drums_state().borrow_mut().set_bpm(bpm);
        self.settings_event()
    }

    /// Rewrite the synced params of every chain to `bpm` and sync each chain
    /// that changed, through the same door a knob turn uses. One chain at a
    /// time: a retime never touches a stream whose blocks did not move.
    pub(crate) fn retime_chains(&self, bpm: f32) -> Result<Vec<Event>> {
        let changed: Vec<ChainId> = self
            .project
            .borrow_mut()
            .chains
            .iter_mut()
            .filter_map(|chain| retime_blocks(&mut chain.blocks, bpm).then(|| chain.id.clone()))
            .collect();
        let mut events = Vec::new();
        for chain in changed {
            events.extend(self.handle_sync_chain_runtime(chain.clone())?);
            events.push(Event::ChainTempoRetimed { chain });
        }
        Ok(events)
    }

    /// After a rig nav reloaded `chain`, retime it in place at the project
    /// tempo — its `ChainReloaded` rebuild carries the new values, so it is
    /// not synced a second time here.
    pub(crate) fn retime_after_nav(&self, chain: &ChainId) {
        let bpm = self.metronome_snapshot().settings.bpm;
        if let Some(slot) = self
            .project
            .borrow_mut()
            .chains
            .iter_mut()
            .find(|c| &c.id == chain)
        {
            retime_blocks(&mut slot.blocks, bpm);
        }
    }
}

/// Turning a synced block's time/rate by hand frees it from the tempo.
pub(crate) fn release_sync_on_manual_edit(block: &mut AudioBlock, path: &str) {
    let Some(sync_path) = sync_path_for_value(path) else {
        return;
    };
    let AudioBlockKind::Core(core) = &mut block.kind else {
        return;
    };
    if core
        .params
        .get_string(sync_path)
        .is_some_and(|value| value != SYNC_OFF)
    {
        core.params
            .insert(sync_path, ParameterValue::String(SYNC_OFF.to_string()));
    }
}

#[cfg(test)]
#[path = "local_dispatcher_tempo_tests.rs"]
mod tests;
