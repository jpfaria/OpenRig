//! Responsibility: applies the global tempo to the tempo-synced blocks of the project.
//! One global tempo (the metronome's BPM) drives every synced delay
//! and modulation; a rig preset may carry its own BPM, and the per-machine
//! "use global tempo" lock keeps a preset load from changing it.
//!
//! Retiming happens here, on the control thread: the synced `time_ms` /
//! `rate_hz` values are rewritten in the project and each changed chain is
//! synced through the same door a knob turn uses. The audio thread only ever
//! sees the resulting numbers.

use anyhow::Result;

use block_core::tempo_sync::{sync_path_for_value, SYNC_OFF};
use domain::ids::ChainId;
use domain::value_objects::ParameterValue;
use feature_dsp::metronome::{BPM_MAX, BPM_MIN};
use project::block::{AudioBlock, AudioBlockKind};
use project::tempo_retime::retime_blocks;

use crate::command::{Command, MetronomeCommand};
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;

impl LocalDispatcher {
    pub(crate) fn handle_set_global_tempo_lock(&self, enabled: bool) -> Result<Vec<Event>> {
        self.metronome_state()
            .borrow_mut()
            .set_global_tempo_lock(enabled);
        self.persist_metronome_field(move |config| config.global_tempo_lock = enabled);
        Ok(vec![Event::GlobalTempoLockChanged { enabled }])
    }

    /// Store (`Some`) or clear (`None`) the tempo of the chain's active rig
    /// preset. Storing it also makes it the current tempo, unless the lock
    /// says the global one wins.
    pub(crate) fn handle_set_rig_preset_bpm(
        &self,
        chain: ChainId,
        bpm: Option<f32>,
    ) -> Result<Vec<Event>> {
        let Some(input) = chain.0.strip_prefix("rig:") else {
            return Ok(vec![]);
        };
        let Some(rig) = self.rig.borrow().clone() else {
            return Ok(vec![]);
        };
        let bpm = bpm.map(|bpm| bpm.clamp(BPM_MIN, BPM_MAX));
        {
            let mut rig = rig.borrow_mut();
            let Some(key) = rig
                .inputs
                .get(input)
                .and_then(|ri| ri.bank.get(&ri.active_preset).cloned())
            else {
                return Ok(vec![]);
            };
            let Some(preset) = rig.presets.get_mut(&key) else {
                return Ok(vec![]);
            };
            preset.bpm = bpm;
        }
        let mut events = vec![
            Event::RigPresetBpmChanged {
                chain: chain.clone(),
                bpm,
            },
            Event::ProjectMutated,
        ];
        if let Some(bpm) = bpm {
            if !self.metronome_snapshot().global_tempo_lock {
                events.extend(self.handle_metronome(Command::Metronome(
                    MetronomeCommand::SetMetronomeBpm { bpm },
                ))?);
            }
        }
        Ok(events)
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

    /// After a rig nav reloaded `chain`: a loaded preset with its own tempo
    /// sets the global one (lock off), and the reloaded chain is retimed in
    /// place — its `ChainReloaded` rebuild carries the new values, so it is
    /// not synced a second time here.
    pub(crate) fn apply_tempo_after_nav(
        &self,
        chain: &ChainId,
        input: &str,
        loads_preset: bool,
    ) -> Result<Vec<Event>> {
        let snapshot = self.metronome_snapshot();
        let preset_bpm = if loads_preset && !snapshot.global_tempo_lock {
            self.active_preset_bpm(input)
        } else {
            None
        }
        .map(|bpm| bpm.clamp(BPM_MIN, BPM_MAX));
        let bpm = preset_bpm.unwrap_or(snapshot.settings.bpm);
        if let Some(slot) = self
            .project
            .borrow_mut()
            .chains
            .iter_mut()
            .find(|c| &c.id == chain)
        {
            retime_blocks(&mut slot.blocks, bpm);
        }
        match preset_bpm {
            Some(bpm) if bpm != snapshot.settings.bpm => {
                self.handle_metronome(Command::Metronome(MetronomeCommand::SetMetronomeBpm {
                    bpm,
                }))
            }
            _ => Ok(vec![]),
        }
    }

    fn active_preset_bpm(&self, input: &str) -> Option<f32> {
        let rig = self.rig.borrow().clone()?;
        let rig = rig.borrow();
        let key = rig
            .inputs
            .get(input)
            .and_then(|ri| ri.bank.get(&ri.active_preset))?;
        rig.presets.get(key)?.bpm
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
