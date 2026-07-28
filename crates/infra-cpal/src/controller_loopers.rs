//! Issue #323 — the controller's looper facade.
//!
//! A chain can be served by SEVERAL parallel runtimes (one per input entry,
//! #703). Each of them owns its own looper slots, records its own input and
//! plays its own material back into its own pipeline — the stream-isolation
//! law: nothing is shared, nothing is mixed across runtimes in our code.
//!
//! So a chain-level op is applied to every runtime of the chain, each with its
//! OWN layer buffer, and a chain-level status is the reading of whichever
//! runtime actually holds material (the input the user played into). Reads are
//! wait-free atomic loads — never the `processing` lock the audio thread
//! try-locks (#580).

use std::sync::Arc;

use domain::ids::ChainId;
use engine::runtime::ChainRuntimeState;
use engine::{DiPcm, LooperOp, LooperState, LooperStatus};
use project::binding_discovery::resolve_output_segment;
use project::chain::Chain;

use crate::controller::ProjectRuntimeController;

impl ProjectRuntimeController {
    /// Every runtime serving `chain_id`, as the audio threads see them.
    ///
    /// The LIVE runtime of a chain lives in its `chain_slots` cell (#672): an
    /// off-thread rebuild publishes a new `ChainRuntimeState` there while the
    /// graph still holds the superseded one. Reading the graph would address a
    /// runtime that no longer processes audio — the looper would answer
    /// "no such looper" after any live edit — so the slots win and the graph
    /// is only the fallback for runtimes that have no slot yet.
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

    /// Queue an op on every runtime of the chain. `make` is called once per
    /// runtime so an op carrying a layer buffer allocates one buffer PER
    /// runtime — two audio threads must never write the same memory.
    /// Returns how many runtimes accepted the op.
    pub fn push_chain_looper_op(
        &self,
        chain_id: &ChainId,
        make: impl Fn(&Arc<ChainRuntimeState>) -> Option<LooperOp>,
    ) -> usize {
        let mut queued = 0;
        for runtime in self.runtimes_for_chain(chain_id) {
            if let Some(op) = make(&runtime) {
                if runtime.push_looper_op(op).is_ok() {
                    queued += 1;
                }
            }
        }
        queued
    }

    /// The chain-level reading of one looper: the runtime that holds the most
    /// material wins, so a chain whose second input never recorded still
    /// reports the loop the user actually played.
    pub fn chain_looper_status(&self, chain_id: &ChainId, uid: u64) -> Option<LooperStatus> {
        self.runtimes_for_chain(chain_id)
            .iter()
            .filter_map(|rt| rt.looper_status(uid))
            .max_by_key(|s| (s.len_frames, s.layers))
    }

    /// Chain-level reading of every looper, in slot order.
    pub fn chain_looper_statuses(&self, chain_id: &ChainId) -> Vec<LooperStatus> {
        let mut out: Vec<LooperStatus> = Vec::new();
        for runtime in self.runtimes_for_chain(chain_id) {
            for status in runtime.looper_statuses() {
                match out.iter_mut().find(|s| s.uid == status.uid) {
                    Some(existing) => {
                        if (status.len_frames, status.layers)
                            > (existing.len_frames, existing.layers)
                        {
                            *existing = status;
                        }
                    }
                    None => out.push(status),
                }
            }
        }
        out
    }

    /// Collect and drop the layer buffers the audio threads handed back.
    /// Called from the GUI tick; freeing memory is forbidden on the audio
    /// thread (invariant #8). Returns how many buffers were dropped.
    pub fn drain_chain_looper_layers(&self, chain_id: &ChainId) -> usize {
        self.runtimes_for_chain(chain_id)
            .iter()
            .map(|rt| rt.drain_retired_layers().len())
            .sum()
    }

    /// The recorded mixdown of one looper (interleaved stereo), from whichever
    /// runtime holds it. `None` when the looper is unknown or empty.
    pub fn export_chain_looper(&self, chain_id: &ChainId, uid: u64) -> Option<Vec<f32>> {
        self.runtimes_for_chain(chain_id)
            .iter()
            .find_map(|rt| rt.export_looper(uid))
    }

    /// #323: reconcile each looper's ISOLATED playback stream with its recorded
    /// state — the same pipeline the DI uses (`arm_looper_stream`), so a loop
    /// plays out its chosen output, through a routed copy of the chain,
    /// independent of the input it was recorded from.
    ///
    /// A looper that is Playing/Overdubbing arms (or re-arms, when its content
    /// `(len, layers)` changed since the last arm) its stream on
    /// `LooperConfig.output`; anything else disarms. Re-arm is gated on the
    /// content signature so this runs every meter tick without respawning a
    /// render each time. Called off the audio thread (the GUI tick).
    pub fn sync_looper_streams(&self, chain: &Chain) {
        // A looper the user removed is no longer in `chain.loopers`, so the
        // per-looper loop below never visits it — its armed stream would play
        // on forever ("deleting the looper doesn't stop it"). Disarm every
        // armed stream of this chain whose looper is gone, first.
        let live: std::collections::HashSet<u64> =
            chain.loopers.iter().map(|c| c.uid).collect();
        let stale: Vec<u64> = self
            .looper_armed
            .borrow()
            .keys()
            .filter(|(cid, uid)| cid == &chain.id && !live.contains(uid))
            .map(|(_, uid)| *uid)
            .collect();
        for uid in stale {
            let key = (chain.id.clone(), uid);
            self.looper_armed.borrow_mut().remove(&key);
            self.looper_suppressed.borrow_mut().remove(&key);
            self.disarm_looper_stream(&chain.id, uid);
        }

        for cfg in &chain.loopers {
            let uid = cfg.uid;
            let key = (chain.id.clone(), uid);
            let state = self
                .chain_looper_status(&chain.id, uid)
                .map(|s| s.state)
                .unwrap_or(LooperState::Empty);
            let playing = matches!(state, LooperState::Playing | LooperState::Overdubbing);

            if !playing {
                // The bank now agrees the looper is not playing: the intent and
                // the status match, so drop any suppression and disarm.
                self.looper_suppressed.borrow_mut().remove(&key);
                if self.looper_armed.borrow_mut().remove(&key).is_some() {
                    self.disarm_looper_stream(&chain.id, uid);
                }
                continue;
            }

            // The user stopped/cleared this looper but the bank has not caught
            // up yet (its callback is not running) — do NOT re-arm from the
            // stale Playing status.
            if self.looper_suppressed.borrow().contains(&key) {
                continue;
            }

            let status = match self.chain_looper_status(&chain.id, uid) {
                Some(s) => s,
                None => continue,
            };
            // The content revision moves on any change that alters the exported
            // mixdown (record close, overdub, undo/redo, level, decay, reverse),
            // so the level/reverse controls take effect via a re-arm — but a
            // steady loop never respawns a render. `len_frames` is kept in the
            // signature so a rebuild that changed the loop length also re-arms.
            let content = (status.len_frames as u64, status.content_rev);
            if self.looper_armed.borrow().get(&key) == Some(&content) {
                continue; // already streaming this exact take
            }
            let Some(samples) = self.export_chain_looper(&chain.id, uid) else {
                continue;
            };
            let output_index =
                resolve_output_segment(chain, &self.io_bindings, cfg.output.as_ref());
            let pcm = Arc::new(DiPcm::new(samples, self.sample_rate, 2));
            if self
                .arm_looper_stream(chain, uid, output_index, pcm)
                .is_ok()
            {
                self.looper_armed.borrow_mut().insert(key, content);
            }
        }
    }

    /// #323: silence one looper's isolated playback NOW, on the user's intent
    /// (stop / clear / remove) — not on the bank's next published status.
    ///
    /// The loop plays on its own worker thread, but the bank ops that change a
    /// looper's state are applied inside the CHAIN's audio callback. When that
    /// callback is not running (the chain is not actively streaming), the Stop
    /// op never drains, the published status stays Playing, and the status-
    /// driven reconcile keeps the stream armed forever — the loop never stops.
    /// Disarming here breaks that dependency: the sound stops immediately, and
    /// the bank state settles whenever the callback next runs.
    pub fn disarm_looper_playback(&self, chain_id: &ChainId, uid: u64) {
        let key = (chain_id.clone(), uid);
        self.looper_armed.borrow_mut().remove(&key);
        // Suppress re-arm until the bank's published status agrees the looper
        // stopped — otherwise the very next reconcile re-arms it from the stale
        // Playing status and the loop never goes silent.
        self.looper_suppressed.borrow_mut().insert(key);
        self.disarm_looper_stream(chain_id, uid);
    }

    /// #323: lift the stop suppression on a looper — a fresh Play/Record intent
    /// means the user wants it sounding again, so the next reconcile may re-arm.
    pub fn allow_looper_playback(&self, chain_id: &ChainId, uid: u64) {
        self.looper_suppressed
            .borrow_mut()
            .remove(&(chain_id.clone(), uid));
    }

    /// Drop the looper stream bookkeeping for a chain (chain removed / project
    /// closed); the streams themselves are torn down by `drop_di_state_for_chain`.
    pub fn forget_chain_looper_streams(&self, chain_id: &ChainId) {
        self.looper_armed
            .borrow_mut()
            .retain(|(cid, _), _| cid != chain_id);
        self.looper_suppressed
            .borrow_mut()
            .retain(|(cid, _)| cid != chain_id);
    }
}
