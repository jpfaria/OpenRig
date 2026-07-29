//! #323 — controller-owned looper state.
//!
//! The redesign's core: the recorded material and transport state of every
//! looper live HERE, on the controller (frontend) thread, NOT inside the
//! volatile `ChainRuntimeState`. The audio thread never touches this — recording
//! is fed from a lock-free input tap drained off-thread (Task 2), and control
//! ops (stop/clear/undo…) mutate the slot directly. Playback is the isolated
//! stream, sourcing [`LooperStore::export`]. This is what makes stop/clear/
//! remove deterministic and makes a loop survive a chain rebuild/toggle.
//!
//! The proven [`engine::looper::LooperSlot`] state machine is reused verbatim;
//! only its OWNER changed.

use std::collections::HashMap;
use std::sync::Arc;

use domain::ids::ChainId;
use engine::spsc::SpscRing;
use engine::{LooperSlot, LooperSpeed, LooperState, LooperStatus, LOOPER_MAX_SECONDS};
use project::block::AudioBlock;
use project::chain::EndpointRef;

/// One looper's state plus the routing it records from / plays to.
struct LoopEntry {
    slot: LooperSlot,
    /// #323: the chosen record-from input; `None` = the chain's first input.
    input: Option<EndpointRef>,
    /// #323: the chosen play-to output; `None` = the chain's main output.
    output: Option<EndpointRef>,
    /// Task 2: the input-tap rings this loop drains while Recording (one per
    /// captured channel). Empty when not recording.
    rings: Vec<Arc<SpscRing<f32>>>,
    /// #323 phase 2: the effect blocks the loop plays THROUGH — resolved from
    /// its linked preset by the adapter and pushed here. `None` = play through
    /// the chain's current blocks (pre-phase-2 behaviour). The controller stays
    /// preset-agnostic: it only holds a block graph, never the rig/preset pool.
    playback_blocks: Option<Vec<AudioBlock>>,
    /// Bumps whenever `playback_blocks` actually changes, so the isolated
    /// stream re-arms when the linked preset is edited or reassigned even
    /// though the recorded loop content is unchanged.
    playback_rev: u64,
}

impl LoopEntry {
    fn new(max_frames: usize) -> Self {
        Self {
            slot: LooperSlot::new(max_frames),
            input: None,
            output: None,
            rings: Vec::new(),
            playback_blocks: None,
            playback_rev: 0,
        }
    }
}

/// All loopers of the whole project, keyed by `(chain, uid)`.
pub struct LooperStore {
    slots: HashMap<(ChainId, u64), LoopEntry>,
    /// Live device rate — sizes each slot's max recording length. Defaults to
    /// 48 kHz until the controller sets the resolved rate.
    sample_rate: u32,
}

impl Default for LooperStore {
    fn default() -> Self {
        Self {
            slots: HashMap::new(),
            sample_rate: 48_000,
        }
    }
}

impl LooperStore {
    /// The longest loop, in frames, at the current rate.
    fn max_frames(&self) -> usize {
        (LOOPER_MAX_SECONDS * self.sample_rate as f32) as usize
    }

    /// Install the resolved device rate (new loops are sized from it).
    pub fn set_sample_rate(&mut self, sample_rate: u32) {
        self.sample_rate = sample_rate.max(1);
    }

    /// Claim a slot for `uid` on `chain` (idempotent — keeps existing material).
    pub fn create(&mut self, chain: &ChainId, uid: u64) {
        let max = self.max_frames();
        self.slots
            .entry((chain.clone(), uid))
            .or_insert_with(|| LoopEntry::new(max));
    }

    /// Drop a looper and everything it recorded.
    pub fn remove(&mut self, chain: &ChainId, uid: u64) {
        self.slots.remove(&(chain.clone(), uid));
    }

    /// The record/overdub tap. Allocates the layer buffer HERE (off the audio
    /// thread) when the tap starts a recording or an overdub; closing needs no
    /// buffer. Mirrors the audio-thread wiring the bank used to do.
    pub fn tap_record(&mut self, chain: &ChainId, uid: u64) {
        let max = self.max_frames();
        let mut just_closed = false;
        if let Some(entry) = self.slots.get_mut(&(chain.clone(), uid)) {
            // Single-take looper (#323): REC starts the one recording (Empty) or
            // closes it (Recording → Playing). It does NOT overdub a loop that
            // already has material — stacking is done with SEPARATE loopers, not
            // layers on one. This is what removes the confusing undo/overdub
            // behaviour (a REC on a playing loop used to silently pile a layer).
            match entry.slot.state() {
                LooperState::Empty => {
                    let buffer = vec![0.0f32; max * 2].into_boxed_slice();
                    entry.slot.tap_record(Some(buffer));
                }
                LooperState::Recording => {
                    entry.slot.tap_record(None);
                    just_closed = true;
                }
                _ => {}
            }
            drain_retired(&mut entry.slot);
            // Closing the recording drops the tap rings so the next recording
            // re-subscribes fresh.
            if !matches!(entry.slot.state(), LooperState::Recording) {
                entry.rings.clear();
            }
        }
        // #323 loop-sync: a freshly closed loop locks to the chain's master.
        if just_closed {
            self.sync_to_master(chain, uid);
        }
    }

    /// #323 loop-sync: quantize the just-closed loop `uid` to a whole MULTIPLE
    /// of the chain's MASTER loop (the shortest OTHER loop that has material)
    /// and restart every playing loop of the chain at 0, so they play locked to
    /// the same bar. No-op when this is the first loop on the chain (it IS the
    /// master — nothing to sync to), so a lone loop keeps its natural length.
    fn sync_to_master(&mut self, chain: &ChainId, uid: u64) {
        let max = self.max_frames();
        let master = self
            .slots
            .iter()
            .filter(|((c, u), e)| {
                c == chain
                    && *u != uid
                    && matches!(
                        e.slot.state(),
                        LooperState::Playing | LooperState::Stopped
                    )
                    && e.slot.len_frames() > 0
            })
            .map(|(_, e)| e.slot.len_frames())
            .min();
        let Some(master) = master else {
            return;
        };
        // Quantize this loop to the nearest whole multiple of the master (≥1×).
        if let Some(entry) = self.slots.get_mut(&(chain.clone(), uid)) {
            let captured = entry.slot.len_frames();
            let mult = ((captured as f64 / master as f64).round() as usize).max(1);
            entry.slot.set_len_frames((mult * master).min(max));
        }
        // Phase-align: restart every playing loop of the chain at 0 together, so
        // the stack begins the bar in lock-step (the isolated streams re-arm
        // cold at position 0 on the next reconcile).
        for ((c, _), e) in self.slots.iter_mut() {
            if c == chain && matches!(e.slot.state(), LooperState::Playing) {
                e.slot.restart();
            }
        }
    }

    /// Append interleaved-stereo `frames` into a Recording/Overdubbing loop.
    /// A no-op for a loop that is not currently recording.
    pub fn record_frames(&mut self, chain: &ChainId, uid: u64, frames: &[f32]) {
        if let Some(entry) = self.slots.get_mut(&(chain.clone(), uid)) {
            for f in frames.chunks_exact(2) {
                let _ = entry.slot.tick([f[0], f[1]]);
            }
        }
    }

    /// Task 2: install the input-tap rings a loop drains while recording. One
    /// ring = mono (broadcast to stereo, invariant #5); two = L/R.
    pub fn set_recording_rings(
        &mut self,
        chain: &ChainId,
        uid: u64,
        rings: Vec<Arc<SpscRing<f32>>>,
    ) {
        if let Some(entry) = self.slots.get_mut(&(chain.clone(), uid)) {
            entry.rings = rings;
        }
    }

    /// Whether this loop currently holds record-tap rings (so the controller
    /// only subscribes once per recording).
    pub fn is_recording_armed(&self, chain: &ChainId, uid: u64) -> bool {
        self.slots
            .get(&(chain.clone(), uid))
            .is_some_and(|e| !e.rings.is_empty())
    }

    /// Drain whatever the audio thread pushed into this loop's tap rings into
    /// its recording layer. Called off the audio thread (the meter tick). No-op
    /// unless the loop is Recording/Overdubbing.
    pub fn drain_recording(&mut self, chain: &ChainId, uid: u64) {
        let Some(entry) = self.slots.get_mut(&(chain.clone(), uid)) else {
            return;
        };
        if !matches!(
            entry.slot.state(),
            LooperState::Recording | LooperState::Overdubbing
        ) {
            return;
        }
        // Pop lock-step across the rings into interleaved stereo, then feed the
        // slot. Reading the rings and writing the slot are disjoint fields.
        let mut interleaved: Vec<f32> = Vec::new();
        match entry.rings.as_slice() {
            [] => {}
            [mono] => {
                while let Some(s) = mono.pop() {
                    interleaved.push(s);
                    interleaved.push(s);
                }
            }
            [l, r, ..] => loop {
                match (l.pop(), r.pop()) {
                    (Some(a), Some(b)) => {
                        interleaved.push(a);
                        interleaved.push(b);
                    }
                    _ => break,
                }
            },
        }
        for f in interleaved.chunks_exact(2) {
            let _ = entry.slot.tick([f[0], f[1]]);
        }
    }

    /// Install a saved loop (interleaved stereo) as the loop's single layer,
    /// landing it in Stopped (never auto-playing) — the project-open path.
    pub fn load(&mut self, chain: &ChainId, uid: u64, pcm: &[f32]) {
        let max = self.max_frames();
        if let Some(entry) = self.slots.get_mut(&(chain.clone(), uid)) {
            let frames = (pcm.len() / 2).min(max);
            let mut buffer = vec![0.0f32; max * 2].into_boxed_slice();
            buffer[..frames * 2].copy_from_slice(&pcm[..frames * 2]);
            entry.slot.load_layer(buffer, frames);
        }
    }

    pub fn stop(&mut self, chain: &ChainId, uid: u64) {
        if let Some(e) = self.slots.get_mut(&(chain.clone(), uid)) {
            e.slot.stop();
            e.rings.clear();
        }
    }
    pub fn play(&mut self, chain: &ChainId, uid: u64) {
        self.with_slot(chain, uid, |s| s.play());
    }
    pub fn clear(&mut self, chain: &ChainId, uid: u64) {
        if let Some(e) = self.slots.get_mut(&(chain.clone(), uid)) {
            e.slot.clear();
            drain_retired(&mut e.slot);
            e.rings.clear();
        }
    }
    /// Single-take looper (#323): there are no overdub layers, so undo/redo do
    /// nothing. Kept as no-ops so the `Command`/MCP surface stays unchanged and
    /// a footswitch or MCP call can never silence a loop by "undoing" it.
    pub fn undo(&mut self, _chain: &ChainId, _uid: u64) {}
    pub fn redo(&mut self, _chain: &ChainId, _uid: u64) {}
    pub fn set_mix(&mut self, chain: &ChainId, uid: u64, v: f32) {
        self.with_slot(chain, uid, |s| s.set_mix(v));
    }
    pub fn set_decay(&mut self, chain: &ChainId, uid: u64, v: f32) {
        self.with_slot(chain, uid, |s| s.set_decay(v));
    }
    pub fn set_speed(&mut self, chain: &ChainId, uid: u64, v: LooperSpeed) {
        self.with_slot(chain, uid, |s| s.set_speed(v));
    }
    pub fn set_reverse(&mut self, chain: &ChainId, uid: u64, v: bool) {
        self.with_slot(chain, uid, |s| s.set_reverse(v));
    }

    /// The chosen record-from input for `uid`, if set.
    pub fn input(&self, chain: &ChainId, uid: u64) -> Option<EndpointRef> {
        self.slots
            .get(&(chain.clone(), uid))
            .and_then(|e| e.input.clone())
    }
    /// Set the record-from input.
    pub fn set_input(&mut self, chain: &ChainId, uid: u64, input: Option<EndpointRef>) {
        if let Some(e) = self.slots.get_mut(&(chain.clone(), uid)) {
            e.input = input;
        }
    }
    /// The chosen play-to output for `uid`, if set.
    pub fn output(&self, chain: &ChainId, uid: u64) -> Option<EndpointRef> {
        self.slots
            .get(&(chain.clone(), uid))
            .and_then(|e| e.output.clone())
    }
    /// Set the play-to output.
    pub fn set_output(&mut self, chain: &ChainId, uid: u64, output: Option<EndpointRef>) {
        if let Some(e) = self.slots.get_mut(&(chain.clone(), uid)) {
            e.output = output;
        }
    }
    /// #323 phase 2: the effect blocks the loop plays through (its linked
    /// preset's blocks), if the adapter has resolved them.
    pub fn playback_blocks(&self, chain: &ChainId, uid: u64) -> Option<Vec<AudioBlock>> {
        self.slots
            .get(&(chain.clone(), uid))
            .and_then(|e| e.playback_blocks.clone())
    }
    /// #323 phase 2: install (or clear) the loop's linked-preset playback
    /// blocks. `None` restores playing through the chain's current blocks.
    /// Bumps `playback_rev` only on a real change so a steady loop never
    /// respawns its render on an idempotent tick.
    pub fn set_playback_blocks(
        &mut self,
        chain: &ChainId,
        uid: u64,
        blocks: Option<Vec<AudioBlock>>,
    ) {
        if let Some(e) = self.slots.get_mut(&(chain.clone(), uid)) {
            if e.playback_blocks != blocks {
                e.playback_blocks = blocks;
                e.playback_rev = e.playback_rev.wrapping_add(1);
            }
        }
    }
    /// #323 phase 2: the current playback-blocks generation (see
    /// [`Self::set_playback_blocks`]); folded into the isolated stream's
    /// re-arm key so a preset edit takes effect.
    pub fn playback_rev(&self, chain: &ChainId, uid: u64) -> u64 {
        self.slots
            .get(&(chain.clone(), uid))
            .map(|e| e.playback_rev)
            .unwrap_or(0)
    }

    /// The published reading of one looper.
    pub fn status(&self, chain: &ChainId, uid: u64) -> Option<LooperStatus> {
        self.slots
            .get(&(chain.clone(), uid))
            .map(|e| status_of(uid, &e.slot))
    }

    /// Every looper of `chain`, in uid order.
    pub fn statuses(&self, chain: &ChainId) -> Vec<LooperStatus> {
        let mut out: Vec<LooperStatus> = self
            .slots
            .iter()
            .filter(|((cid, _), _)| cid == chain)
            .map(|((_, uid), e)| status_of(*uid, &e.slot))
            .collect();
        out.sort_by_key(|s| s.uid);
        out
    }

    /// The recorded mixdown (interleaved stereo), or `None` when empty.
    pub fn export(&self, chain: &ChainId, uid: u64) -> Option<Vec<f32>> {
        self.slots
            .get(&(chain.clone(), uid))
            .and_then(|e| e.slot.export_mixdown())
    }

    fn with_slot(&mut self, chain: &ChainId, uid: u64, f: impl FnOnce(&mut LooperSlot)) {
        if let Some(e) = self.slots.get_mut(&(chain.clone(), uid)) {
            f(&mut e.slot);
        }
    }
}

fn status_of(uid: u64, slot: &LooperSlot) -> LooperStatus {
    LooperStatus {
        uid,
        state: slot.state(),
        position_frames: slot.position_frames(),
        len_frames: slot.len_frames(),
        layers: slot.active_layers(),
        content_rev: slot.content_revision(),
    }
}

/// Drop the layer buffers the slot retired (undo/clear/close). Off the audio
/// thread this is a plain drop — no return-queue dance is needed anymore.
fn drain_retired(slot: &mut LooperSlot) {
    while slot.take_retired().is_some() {}
}

#[cfg(test)]
#[path = "looper_store_tests.rs"]
mod tests;
