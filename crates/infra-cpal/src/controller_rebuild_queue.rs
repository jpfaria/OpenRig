//! Responsibility: applies the chain rebuilds the control worker finished off-thread.
//!
//! Issue #672: heavy builds (NAM loads, route assembly) run on the control
//! worker; the frontend tick calls `poll_pending_rebuilds`, which swaps the
//! live slot and the `runtime_graph` entry in lock-step so the audio path
//! and every other reader stay consistent.

use std::sync::Arc;

use project::chain::Chain;

use crate::controller::ProjectRuntimeController;
use crate::{build_chain_runtime, BuildRequest, LiveRuntimeSlot};

impl ProjectRuntimeController {
    /// Issue #672 — enqueue an off-thread rebuild of a chain's runtime.
    ///
    /// The heavy build (NAM loads, route assembly) runs on the control worker;
    /// the caller (frontend thread) returns immediately. The finished build is
    /// applied later by [`ProjectRuntimeController::poll_pending_rebuilds`].
    /// Applies to chains that already have a runtime (the freeze case — editing
    /// a live chain whose IO is unchanged); a brand-new chain or an IO-topology
    /// change is still built synchronously by `upsert_chain`.
    pub fn schedule_chain_rebuild(
        &mut self,
        chain: &Chain,
        sample_rate: f32,
        device_sample_rates: std::collections::HashMap<domain::ids::DeviceId, f32>,
        buffer_sizes: Vec<usize>,
    ) {
        // Seed the slots from the current graph runtimes if they have not
        // been created yet (chain built before its slots were wired). Issue
        // #703: a chain owns one slot per input-entry group.
        let groups: Vec<usize> = self
            .runtime_graph
            .chains
            .keys()
            .filter(|(cid, _)| cid == &chain.id)
            .map(|(_, g)| *g)
            .collect();
        for group in groups {
            let key = (chain.id.clone(), group);
            if !self.chain_slots.contains_key(&key) {
                if let Some(runtime) = self.runtime_graph.chains.get(&key).map(Arc::clone) {
                    self.chain_slots.insert(key, LiveRuntimeSlot::new(runtime));
                }
            }
        }

        let request = BuildRequest {
            chain: chain.clone(),
            sample_rate,
            device_sample_rates,
            buffer_sizes,
            io_bindings: self.io_bindings.clone(),
        };
        let rx = self.worker.submit(move || build_chain_runtime(&request));
        self.pending_rebuilds.push((chain.id.clone(), rx));
    }

    /// Issue #672 — apply any finished off-thread rebuilds (call on the frontend
    /// tick). For each completed build, swap the live slot AND the
    /// `runtime_graph` entry in lock-step so the audio path and every other
    /// reader stay consistent, and drop the superseded runtime back on the
    /// worker (its NAM C++ destructors never run on the audio/frontend thread).
    ///
    /// Returns the number of rebuilds applied this tick.
    pub fn poll_pending_rebuilds(&mut self) -> usize {
        let mut applied = 0;
        let mut still_pending = Vec::new();
        for (chain_id, rx) in std::mem::take(&mut self.pending_rebuilds) {
            match rx.try_recv() {
                Ok(Ok(runtimes)) => {
                    // Issue #703: publish each per-entry runtime into ITS
                    // OWN (chain, group) slot. Publishing a single runtime
                    // into group 0 (the old shape) would leave sibling
                    // entries stale — or, on a shared device, double-process
                    // the buffer (the stream feeds every bound slot).
                    for (group, runtime) in runtimes {
                        let key = (chain_id.clone(), group);
                        if let Some(slot) = self.chain_slots.get(&key) {
                            // #740: carry the live meter/spectrum/tuner taps over
                            // to the rebuilt runtime BEFORE it goes live, or the
                            // graph freezes after a preset switch / live edit (the
                            // UI's tap rings were subscribed on the old runtime).
                            runtime.adopt_taps_from(&slot.load());
                            let graph_runtime = Arc::clone(&runtime);
                            let superseded = slot.publish(runtime);
                            self.runtime_graph.chains.insert(key, graph_runtime);
                            // Drop the old runtime off the audio/frontend thread.
                            let _ = self.worker.submit(move || drop(superseded));
                            applied += 1;
                        } else {
                            log::error!(
                                "chain '{}' rebuild produced entry group {} with no live \
                                 slot — the edit needs a stream rebuild to be heard",
                                chain_id.0,
                                group
                            );
                        }
                    }
                }
                Ok(Err(e)) => {
                    log::error!("chain '{}' off-thread rebuild failed: {e}", chain_id.0);
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => still_pending.push((chain_id, rx)),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    log::error!("chain '{}' rebuild worker disconnected", chain_id.0);
                }
            }
        }
        self.pending_rebuilds = still_pending;

        // Cold activations: once the runtime is built off-thread, create the
        // cpal streams on THIS (frontend) thread — cpal `Stream` is `!Send` — and
        // install the chain.
        let mut still_activating = Vec::new();
        for (chain_id, chain, rx) in std::mem::take(&mut self.pending_activations) {
            match rx.try_recv() {
                Ok(Ok((runtimes, resolved))) => {
                    // Issue #703: install every per-entry runtime — a
                    // single-device chain may own N isolated runtimes (one
                    // per input entry) all fed by the one device stream.
                    for (group, runtime) in &runtimes {
                        self.runtime_graph
                            .chains
                            .insert((chain_id.clone(), *group), Arc::clone(runtime));
                    }
                    let slots = crate::build_chain_slots(&runtimes);
                    for (group, slot) in &slots {
                        self.chain_slots
                            .insert((chain_id.clone(), *group), slot.handle());
                    }
                    // #669/#693: the worker resolved the real device rate —
                    // mirror it like the synchronous upsert path does, so DI
                    // loops resample to the live rate.
                    self.sample_rate = resolved.sample_rate as u32;
                    // #771: one DI playback cell per output stream, shared
                    // with arm_di_stream through the controller map.
                    let di_cells: Vec<_> = (0..resolved.outputs.len())
                        .map(|j| self.di_playback_cell(&chain_id, j))
                        .collect();
                    match crate::build_active_chain_runtime(
                        &chain_id,
                        &chain,
                        resolved,
                        slots,
                        &self.io_bindings,
                        &di_cells,
                    ) {
                        Ok(active) => {
                            self.active_chains.insert(chain_id, active);
                            // #771: an armed DI re-renders against the fresh
                            // streams (output index/rate/dest may have moved).
                            self.rearm_di_stream_after_rebuild(&chain);
                            applied += 1;
                        }
                        Err(e) => {
                            log::error!("chain '{}' stream build failed: {e}", chain_id.0);
                            self.runtime_graph.remove_chain(&chain_id);
                        }
                    }
                }
                Ok(Err(e)) => log::error!("chain '{}' activation build failed: {e}", chain_id.0),
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    still_activating.push((chain_id, chain, rx))
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    log::error!("chain '{}' activation worker disconnected", chain_id.0)
                }
            }
        }
        self.pending_activations = still_activating;
        applied
    }
}
