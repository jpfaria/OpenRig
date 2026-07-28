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

/// Record-tap ring capacity per channel — ~1 s at 48 kHz, so a delayed meter
/// tick never drops recorded samples before the drain runs.
const RECORD_RING_CAP: usize = 48_000;

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
        self.looper_store.borrow().status(chain_id, uid)
    }

    pub fn chain_looper_statuses(&self, chain_id: &ChainId) -> Vec<LooperStatus> {
        self.looper_store.borrow().statuses(chain_id)
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
        self.looper_armed.borrow_mut().remove(&(chain_id.clone(), uid));
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

    /// Whether the loop is currently sounding — the `PlayStop` toggle reads this.
    pub fn looper_is_playing(&self, chain_id: &ChainId, uid: u64) -> bool {
        matches!(
            self.looper_store.borrow().status(chain_id, uid).map(|s| s.state),
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
        self.looper_store.borrow_mut().set_output(chain_id, uid, output);
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
            self.looper_armed.borrow_mut().remove(&(chain.id.clone(), uid));
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
            // a steady loop never respawns a render.
            let content = (status.len_frames as u64, status.content_rev);
            if self.looper_armed.borrow().get(&key) == Some(&content) {
                continue;
            }
            let samples = match self.looper_store.borrow().export(&chain.id, uid) {
                Some(s) => s,
                None => continue,
            };
            let output_index = resolve_output_segment(chain, &self.io_bindings, cfg.output.as_ref());
            let pcm = Arc::new(DiPcm::new(samples, self.sample_rate, 2));
            if self.arm_looper_stream(chain, uid, output_index, pcm).is_ok() {
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
