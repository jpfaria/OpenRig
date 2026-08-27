//! Responsibility: fronts the controller's loopers to the application.
//! Issue #323 — the controller's looper facade (redesigned).
//!
//! Looper state lives in the controller-owned [`crate::looper_store::LooperStore`],
//! NOT inside the volatile `ChainRuntimeState`. Recording drains the chain's
//! lock-free input tap into the store off the audio thread; control ops mutate
//! the store directly, so stop/clear/remove are deterministic and a loop
//! survives a chain rebuild or enable toggle. Playback stays the isolated
//! stream (`IsolatedSource::Looper`), sourcing the store's exported mixdown.

use std::collections::HashSet;
use std::sync::Arc;

use domain::ids::ChainId;
use engine::runtime::ChainRuntimeState;
use engine::{DiPcm, LooperState, LooperStatus};
use project::binding_discovery::{resolve_input_segment, resolve_output_segment};
use project::chain::{Chain, EndpointRef, LooperSpeed};

use crate::controller::ProjectRuntimeController;
use crate::looper_store::LooperEditRefused;
use engine::loop_edit::LoopEditOp;

/// Record-tap ring capacity per channel — ~1 s at 48 kHz, so a delayed meter
/// tick never drops recorded samples before the drain runs.
const RECORD_RING_CAP: usize = 48_000;

/// #323 phase 2: the chain the loop's isolated stream plays through. With a
/// linked preset the adapter has resolved (`Some(blocks)`), it is the chain
/// with its processing blocks swapped for the preset's — same id and
/// `io_binding_ids`, so routing, output resolution and stream isolation
/// (invariant #4) are untouched and only the tone differs. Without one, the
/// chain plays through its own current blocks (pre-phase-2 behaviour).
pub(crate) fn looper_playback_chain(
    chain: &Chain,
    linked_blocks: Option<Vec<project::block::AudioBlock>>,
) -> Chain {
    let mut c = chain.clone();
    if let Some(blocks) = linked_blocks {
        c.blocks = blocks;
    }
    c
}

#[cfg(test)]
#[path = "controller_loopers_tests.rs"]
mod tests;

impl ProjectRuntimeController {
    /// Every runtime serving `chain_id`, as the audio threads see them (the live
    /// slot wins over the graph, #672). Still used for the record tap and the DI.
    pub fn runtimes_for_chain(&self, chain_id: &ChainId) -> Vec<Arc<ChainRuntimeState>> {
        let mut live: Vec<(usize, Arc<ChainRuntimeState>)> = self
            .chain_slots
            .iter()
            .filter(|((cid, _), _)| cid == chain_id)
            .map(|((_, group), slot)| (*group, slot.load()))
            .collect();
        if live.is_empty() {
            return self.runtime_graph.runtimes_for(chain_id);
        }
        live.sort_by_key(|(group, _)| *group);
        live.into_iter().map(|(_, runtime)| runtime).collect()
    }

    // ── read (from the store — the single source of truth) ────────────────

    pub fn chain_looper_status(&self, chain_id: &ChainId, uid: u64) -> Option<LooperStatus> {
        let mut status = self.looper_store.borrow().status(chain_id, uid)?;
        self.overlay_playback_position(chain_id, &mut status);
        Some(status)
    }

    pub fn chain_looper_statuses(&self, chain_id: &ChainId) -> Vec<LooperStatus> {
        let mut statuses = self.looper_store.borrow().statuses(chain_id);
        for status in &mut statuses {
            self.overlay_playback_position(chain_id, status);
        }
        statuses
    }

    /// During playback the store's slot is idle — the loop sounds on the
    /// isolated stream (#323 redesign), so its audible position lives on THAT
    /// stream's cursor, not the frozen slot. Overlay it so the UI timer runs;
    /// otherwise it sits at 0:00 the whole time the loop plays.
    fn overlay_playback_position(&self, chain_id: &ChainId, status: &mut LooperStatus) {
        if !matches!(
            status.state,
            LooperState::Playing | LooperState::Overdubbing
        ) {
            return;
        }
        if let Some(pos) = self.looper_stream_position(chain_id, status.uid) {
            status.position_frames = pos;
        }
    }

    /// The recorded mixdown (interleaved stereo), or `None` when empty.
    pub fn export_chain_looper(&self, chain_id: &ChainId, uid: u64) -> Option<Vec<f32>> {
        self.looper_store.borrow().export(chain_id, uid)
    }

    // ── control (mutate the store; deterministic, no runtime needed) ──────

    /// Claim a slot for `uid` (idempotent). Routing is `None` (first input /
    /// main output) until a pick or a project restore sets it via
    /// [`Self::looper_set_input`] / [`Self::looper_set_output`].
    pub fn looper_create(&self, chain_id: &ChainId, uid: u64) {
        let mut store = self.looper_store.borrow_mut();
        store.set_sample_rate(self.sample_rate);
        store.create(chain_id, uid);
    }

    /// Install a saved loop (project-open path). Sizes the slot at the current
    /// rate; the loop lands Stopped.
    pub fn looper_load(&self, chain_id: &ChainId, uid: u64, pcm: &[f32]) {
        let mut store = self.looper_store.borrow_mut();
        store.set_sample_rate(self.sample_rate);
        store.create(chain_id, uid);
        store.load(chain_id, uid, pcm);
    }

    pub fn looper_remove(&self, chain_id: &ChainId, uid: u64) {
        self.looper_store.borrow_mut().remove(chain_id, uid);
        self.looper_armed
            .borrow_mut()
            .remove(&(chain_id.clone(), uid));
        self.disarm_looper_stream(chain_id, uid);
    }

    /// The record/overdub footswitch tap.
    pub fn looper_tap_record(&self, chain_id: &ChainId, uid: u64) {
        self.looper_store.borrow_mut().tap_record(chain_id, uid);
    }

    pub fn looper_stop(&self, chain_id: &ChainId, uid: u64) {
        self.looper_store.borrow_mut().stop(chain_id, uid);
    }

    pub fn looper_play(&self, chain_id: &ChainId, uid: u64) {
        self.looper_store.borrow_mut().play(chain_id, uid);
    }

    pub fn looper_clear(&self, chain_id: &ChainId, uid: u64) {
        self.looper_store.borrow_mut().clear(chain_id, uid);
    }

    pub fn looper_undo(&self, chain_id: &ChainId, uid: u64) {
        self.looper_store.borrow_mut().undo(chain_id, uid);
    }

    pub fn looper_redo(&self, chain_id: &ChainId, uid: u64) {
        self.looper_store.borrow_mut().redo(chain_id, uid);
    }

    /// #826: the loop's raw material — no `mix`/`decay`/`reverse` baked in —
    /// for the waveform editor to draw and reshape.
    pub fn export_chain_looper_raw(&self, chain_id: &ChainId, uid: u64) -> Option<Vec<f32>> {
        self.looper_store.borrow().export_raw(chain_id, uid)
    }

    /// #826: reshape a stopped loop; the new length in frames on success.
    pub fn looper_apply_edit(
        &self,
        chain_id: &ChainId,
        uid: u64,
        op: LoopEditOp,
        start: usize,
        end: usize,
    ) -> Result<usize, LooperEditRefused> {
        self.looper_store
            .borrow_mut()
            .apply_edit(chain_id, uid, op, start, end)
    }

    /// #826: step back one waveform edit; `false` when there is nothing to undo.
    pub fn looper_undo_edit(&self, chain_id: &ChainId, uid: u64) -> bool {
        self.looper_store.borrow_mut().undo_edit(chain_id, uid)
    }

    /// #826: step forward one undone waveform edit.
    pub fn looper_redo_edit(&self, chain_id: &ChainId, uid: u64) -> bool {
        self.looper_store.borrow_mut().redo_edit(chain_id, uid)
    }

    /// #826: (undo depth, redo depth) — what the editor's buttons enable on.
    pub fn looper_edit_history_depth(&self, chain_id: &ChainId, uid: u64) -> (usize, usize) {
        self.looper_store.borrow().edit_history_depth(chain_id, uid)
    }

    /// Whether the loop is currently sounding — the `PlayStop` toggle reads this.
    pub fn looper_is_playing(&self, chain_id: &ChainId, uid: u64) -> bool {
        matches!(
            self.looper_store
                .borrow()
                .status(chain_id, uid)
                .map(|s| s.state),
            Some(LooperState::Playing | LooperState::Overdubbing)
        )
    }

    pub fn looper_set_mix(&self, chain_id: &ChainId, uid: u64, v: f32) {
        self.looper_store.borrow_mut().set_mix(chain_id, uid, v);
    }
    pub fn looper_set_decay(&self, chain_id: &ChainId, uid: u64, v: f32) {
        self.looper_store.borrow_mut().set_decay(chain_id, uid, v);
    }
    pub fn looper_set_speed(&self, chain_id: &ChainId, uid: u64, v: LooperSpeed) {
        self.looper_store.borrow_mut().set_speed(chain_id, uid, v);
    }
    pub fn looper_set_reverse(&self, chain_id: &ChainId, uid: u64, v: bool) {
        self.looper_store.borrow_mut().set_reverse(chain_id, uid, v);
    }
    pub fn looper_set_input(&self, chain_id: &ChainId, uid: u64, input: Option<EndpointRef>) {
        // A new input means re-subscribing the record tap; drop the current
        // rings so the next drain re-arms from the chosen segment.
        let mut store = self.looper_store.borrow_mut();
        store.set_input(chain_id, uid, input);
        store.set_recording_rings(chain_id, uid, Vec::new());
    }
    pub fn looper_set_output(&self, chain_id: &ChainId, uid: u64, output: Option<EndpointRef>) {
        self.looper_store
            .borrow_mut()
            .set_output(chain_id, uid, output);
    }
    /// #323 phase 2: install the effect blocks the loop plays through — its
    /// LINKED preset's blocks, resolved by the adapter (which owns the rig).
    /// `None` restores playing through the chain's current blocks. Idempotent:
    /// the store bumps its re-arm generation only on a real change.
    pub fn looper_set_playback_blocks(
        &self,
        chain_id: &ChainId,
        uid: u64,
        blocks: Option<Vec<project::block::AudioBlock>>,
    ) {
        self.looper_store
            .borrow_mut()
            .set_playback_blocks(chain_id, uid, blocks);
    }

    /// Make sure every looper the PROJECT carries has a store entry — so a
    /// looper added by ANY transport (GUI, MCP, MIDI) or loaded from disk is
    /// readable and controllable, not only the ones routed through
    /// `apply_looper_event`. Idempotent: an existing entry keeps its material
    /// and rings; only a brand-new one is created and seeded with its routing.
    /// Called on the meter tick.
    pub fn sync_looper_slots(&self, chain: &Chain) {
        let mut store = self.looper_store.borrow_mut();
        store.set_sample_rate(self.sample_rate);
        for cfg in &chain.loopers {
            if store.status(&chain.id, cfg.uid).is_none() {
                store.create(&chain.id, cfg.uid);
                store.set_input(&chain.id, cfg.uid, cfg.input.clone());
                store.set_output(&chain.id, cfg.uid, cfg.output.clone());
            }
        }
    }

    // ── recording: subscribe the input tap + drain, off the audio thread ──

    /// Feed each Recording loop from its input tap. Called on the meter tick.
    /// Subscribes the chosen input segment's tap once per recording, then drains
    /// whatever the audio thread pushed into the loop's buffer.
    pub fn drain_looper_recording(&self, chain: &Chain) {
        // Snapshot which loops need arming, WITHOUT holding the store borrow
        // across the tap subscription (which borrows other controller state).
        let to_arm: Vec<(u64, Option<EndpointRef>)> = {
            let store = self.looper_store.borrow();
            chain
                .loopers
                .iter()
                .filter_map(|cfg| {
                    let uid = cfg.uid;
                    let recording = matches!(
                        store.status(&chain.id, uid).map(|s| s.state),
                        Some(LooperState::Recording | LooperState::Overdubbing)
                    );
                    if recording && !store.is_recording_armed(&chain.id, uid) {
                        Some((uid, store.input(&chain.id, uid)))
                    } else {
                        None
                    }
                })
                .collect()
        };
        for (uid, input) in to_arm {
            let seg = resolve_input_segment(chain, &self.io_bindings, input.as_ref());
            if let Some(ring) = self.subscribe_stream_input_tap(&chain.id, seg, RECORD_RING_CAP) {
                self.looper_store
                    .borrow_mut()
                    .set_recording_rings(&chain.id, uid, vec![ring]);
            }
        }
        // Drain every recording loop.
        let mut store = self.looper_store.borrow_mut();
        for cfg in &chain.loopers {
            store.drain_recording(&chain.id, cfg.uid);
        }
    }

    // ── reconcile the isolated playback stream from store state ───────────

    /// Arm a Playing/Overdubbing loop's isolated stream (re-arm only when its
    /// content changed) and disarm anything else — including a looper the user
    /// removed. Reads the store, which is authoritative and updated on the same
    /// thread, so there is no stale-status race and no suppression is needed.
    pub fn sync_looper_streams(&self, chain: &Chain) {
        let live: HashSet<u64> = chain.loopers.iter().map(|c| c.uid).collect();
        let stale: Vec<u64> = self
            .looper_armed
            .borrow()
            .keys()
            .filter(|(cid, uid)| cid == &chain.id && !live.contains(uid))
            .map(|(_, uid)| *uid)
            .collect();
        for uid in stale {
            self.looper_armed
                .borrow_mut()
                .remove(&(chain.id.clone(), uid));
            self.disarm_looper_stream(&chain.id, uid);
        }

        for cfg in &chain.loopers {
            let uid = cfg.uid;
            let key = (chain.id.clone(), uid);
            let status = self.looper_store.borrow().status(&chain.id, uid);
            let playing = matches!(
                status.map(|s| s.state),
                Some(LooperState::Playing | LooperState::Overdubbing)
            );
            if !playing {
                if self.looper_armed.borrow_mut().remove(&key).is_some() {
                    self.disarm_looper_stream(&chain.id, uid);
                }
                continue;
            }
            let status = match status {
                Some(s) => s,
                None => continue,
            };
            // Content moves on any mixdown-altering change (close, overdub,
            // undo/redo, level/decay/reverse) so those take effect via a re-arm;
            // a steady loop never respawns a render. The playback-blocks
            // generation is folded in so editing/reassigning the linked preset
            // (#323 phase 2) also re-renders the loop through the new tone.
            let playback_rev = self.looper_store.borrow().playback_rev(&chain.id, uid);
            let content = (status.len_frames as u64, status.content_rev, playback_rev);
            if self.looper_armed.borrow().get(&key) == Some(&content) {
                continue;
            }
            let samples = match self.looper_store.borrow().export(&chain.id, uid) {
                Some(s) => s,
                None => continue,
            };
            let output_index =
                resolve_output_segment(chain, &self.io_bindings, cfg.output.as_ref());
            let pcm = Arc::new(DiPcm::new(samples, self.sample_rate, 2));
            // #323 phase 2: play through the loop's LINKED preset when the
            // adapter has resolved its blocks — a routed copy of the chain with
            // its processing swapped, same id/I/O so isolation is unchanged
            // (invariant #4). No linked preset ⇒ the chain's current blocks.
            let linked = self.looper_store.borrow().playback_blocks(&chain.id, uid);
            let playback_chain = looper_playback_chain(chain, linked);
            if self
                .arm_looper_stream(&playback_chain, uid, output_index, pcm)
                .is_ok()
            {
                self.looper_armed.borrow_mut().insert(key, content);
            }
        }
    }

    /// Drop the looper stream bookkeeping for a chain (chain removed / project
    /// closed); the streams themselves are torn down by `drop_di_state_for_chain`.
    pub fn forget_chain_looper_streams(&self, chain_id: &ChainId) {
        self.looper_armed
            .borrow_mut()
            .retain(|(cid, _), _| cid != chain_id);
    }
}
