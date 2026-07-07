//! Issue #771 — an armed DI loop plays on its own isolated, STREAMED
//! runtime, output-clocked via ring backpressure, never injected into the
//! guitar's runtime.
//!
//! Arming resolves the chain's persisted output choice (`Chain.di_output`),
//! builds a fresh routed copy of the chain's block graph on a `di-stream`
//! worker and parks a ring-backed playback in the CHOSEN output's
//! [`DiPlaybackCell`] immediately (a 75 s loop starts in milliseconds — the
//! full pre-render tried first took minutes before the first sample). The
//! worker only produces what the output callback consumed (ring
//! backpressure), so the output device clock IS the DI clock: no drift by
//! construction (the sleep-paced worker tried in #717 drifted and was
//! reverted, `f1131725e`), and no DSP ever runs in the callback. The guitar
//! runtime is never touched (invariant #4).
//!
//! Lifecycle rules (review findings, #771):
//! - The render thread parks ONLY under the handle's `armed` lock, and
//!   disarm flips it under the same lock — a disarm ALWAYS wins over an
//!   in-flight render (no zombie playback). The lock lives entirely off the
//!   audio thread.
//! - A retired playback is never dropped by the audio callback: disarm swaps
//!   it out and parks it in `di_retired`; the entry is freed on a LATER
//!   disarm/arm cycle, long after any in-flight callback guard is gone.
//! - A failed render flips the handle's `failed` flag; `di_stream_active`
//!   then reports NOT playing (the meter poll resets the UI) instead of an
//!   eternal silent "playing".

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::Result;

use domain::ids::ChainId;
use engine::di_output_resolve::resolve_di_output_index;
use engine::di_render::build_routed_di_runtime;
use engine::runtime::{process_input_f32, process_output_f32};
use engine::runtime_endpoints::resolve_chain_io;
use engine::DiPcm;
use project::chain::Chain;

use crate::di_playback::{DiPlayback, DiPlaybackCell};
use crate::ProjectRuntimeController;

/// Bookkeeping for one armed DI. Dropping the handle disarms (safety net);
/// the controller's `disarm_di_stream` is the primary path because it also
/// retires the parked playback off the audio thread.
pub(crate) struct DiStreamHandle {
    output_index: usize,
    cell: DiPlaybackCell,
    /// `true` while this arm owns the cell. The render thread parks only
    /// under this lock; disarm flips it under the same lock.
    armed: Arc<Mutex<bool>>,
    /// The source, kept so the controller can re-arm after a rebuild
    /// without a dispatcher round-trip.
    pcm: Arc<DiPcm>,
    /// Set by the render thread on failure; surfaces through
    /// [`ProjectRuntimeController::di_stream_active`].
    failed: Arc<AtomicBool>,
}

impl DiStreamHandle {
    fn disarm(&self) -> Option<Arc<DiPlayback>> {
        let mut armed = self.armed.lock().unwrap_or_else(|e| e.into_inner());
        *armed = false;
        self.cell.swap(None)
    }
}

impl Drop for DiStreamHandle {
    fn drop(&mut self) {
        // Safety net for non-controller drops (controller teardown). The
        // swapped-out playback drops here, on a non-audio thread.
        let _ = self.disarm();
    }
}

impl ProjectRuntimeController {
    /// The playback cell for `(chain, flat output index)`, created on demand.
    /// Stream builds and arming both resolve through here, so the SAME cell is
    /// shared regardless of which side runs first, and it survives rebuilds.
    pub(crate) fn di_playback_cell(
        &self,
        chain_id: &ChainId,
        output_index: usize,
    ) -> DiPlaybackCell {
        self.di_playback_cells
            .borrow_mut()
            .entry((chain_id.clone(), output_index))
            .or_insert_with(|| Arc::new(arc_swap::ArcSwapOption::from(None)))
            .clone()
    }

    /// The rate the chain's `output_index` stream actually runs at: the live
    /// stream signature when the chain is active, else the controller's last
    /// resolved rate (never a hardcoded constant — #669/#723).
    fn di_output_rate(&self, chain_id: &ChainId, output_index: usize) -> u32 {
        self.active_chains
            .get(chain_id)
            .and_then(|active| active.stream_signature.outputs.get(output_index))
            .map(|sig| sig.sample_rate)
            .unwrap_or(self.sample_rate)
    }

    /// Arm the chain's DI: resolve the chosen output, build the routed
    /// runtime off-thread and stream the loop into that output's cell. The
    /// guitar runtime is NEVER touched.
    pub fn arm_di_stream(&self, chain: &Chain, pcm: Arc<DiPcm>) -> Result<()> {
        // Re-arm replaces any previous playback (and retires it off the
        // audio thread).
        self.disarm_di_stream(&chain.id);

        let output_index =
            resolve_di_output_index(chain, &self.io_bindings, chain.di_output.as_ref());
        let output_rate = self.di_output_rate(&chain.id, output_index);
        let (_, outputs) = resolve_chain_io(chain, &self.io_bindings);
        let dest = outputs
            .get(output_index)
            .map(|o| o.channels.clone())
            .unwrap_or_default();
        let dest_left = dest.first().copied().unwrap_or(0);
        let dest_right = dest.get(1).copied().unwrap_or(dest_left);

        let cell = self.di_playback_cell(&chain.id, output_index);
        let armed = Arc::new(Mutex::new(true));
        let failed = Arc::new(AtomicBool::new(false));
        {
            let cell = cell.clone();
            let armed = Arc::clone(&armed);
            let failed = Arc::clone(&failed);
            let chain = chain.clone();
            let registry = self.io_bindings.clone();
            let pcm = Arc::clone(&pcm);
            std::thread::Builder::new()
                .name("di-stream".into())
                .spawn(move || {
                    // Build the routed isolated runtime (heavy: NAM/IR loads)
                    // OFF the frontend; the loop is fed through the CHOSEN
                    // output's own binding (#716/#699 — a flat render on a
                    // multi-binding chain was silent for any binding but the
                    // first: the owner's "no sound").
                    let routed = match build_routed_di_runtime(
                        &chain,
                        &registry,
                        chain.di_output.as_ref(),
                        output_rate,
                        &pcm,
                    ) {
                        Ok(r) => r,
                        Err(e) => {
                            failed.store(true, Ordering::Relaxed);
                            log::error!("di-stream build failed for chain '{}': {e:#}", chain.id.0);
                            return;
                        }
                    };
                    // Raw (pre-chain) loop for the DI IN meter.
                    let raw = pcm.to_loop_at(output_rate);
                    let raw_len = raw.len().max(1);

                    // Park the playback IMMEDIATELY — frames start flowing
                    // within one block, regardless of loop length (a 75 s
                    // loop used to pre-render for minutes before the first
                    // sample). Park ONLY while this arm still owns the cell.
                    let playback =
                        Arc::new(DiPlayback::new(dest_left, dest_right, routed.loop_len));
                    let ring = playback.ring();
                    {
                        let armed = armed.lock().unwrap_or_else(|e| e.into_inner());
                        if !*armed {
                            return;
                        }
                        cell.store(Some(Arc::clone(&playback)));
                    }

                    // Stream the DI: paced by RING BACKPRESSURE — the worker
                    // only produces what the output consumed, so the output
                    // device clock IS the DI clock (no drift by construction;
                    // the sleep-paced worker tried in #717 drifted and was
                    // reverted, f1131725e). All DSP runs HERE, never in the
                    // callback (invariant #8).
                    const BLOCK: usize = 256;
                    let silence = vec![0.0f32; BLOCK];
                    let mut drain = vec![0.0f32; BLOCK * routed.drain_width];
                    let mut pos: usize = 0;
                    loop {
                        {
                            let armed = armed.lock().unwrap_or_else(|e| e.into_inner());
                            if !*armed {
                                return;
                            }
                        }
                        let free = ring.capacity() - ring.len();
                        if free < BLOCK * 2 {
                            std::thread::sleep(std::time::Duration::from_millis(2));
                            continue;
                        }
                        process_input_f32(&routed.runtime, 0, &silence, 1);
                        process_output_f32(
                            &routed.runtime,
                            routed.output_index,
                            &mut drain,
                            routed.drain_width,
                        );
                        for frame in drain.chunks(routed.drain_width) {
                            let _ = ring.push(frame[routed.drain_left]);
                            let _ = ring.push(frame[routed.drain_right]);
                        }
                        let mut in_peak = 0.0f32;
                        for i in 0..BLOCK {
                            let f = match raw.frame_at((pos + i) % raw_len) {
                                engine::DiFrame::Mono(s) => s.abs(),
                                engine::DiFrame::Stereo([a, b]) => a.abs().max(b.abs()),
                            };
                            in_peak = in_peak.max(f);
                        }
                        pos = (pos + BLOCK) % raw_len;
                        playback.set_in_peak(in_peak);
                    }
                })
                .expect("spawn di-stream thread");
        }
        self.di_streams.borrow_mut().insert(
            chain.id.clone(),
            DiStreamHandle {
                output_index,
                cell,
                armed,
                pcm,
                failed,
            },
        );
        Ok(())
    }

    /// Disarm the chain's DI: the in-flight render (if any) is neutralized
    /// under the handle's lock, and the parked playback is RETIRED — freed on
    /// a later cycle, never by the audio callback (invariant #8).
    pub fn disarm_di_stream(&self, chain_id: &ChainId) {
        // Free the previous cycle's retirees first: by now any callback
        // guard that referenced them (a µs-scale window) is long gone.
        self.di_retired.borrow_mut().clear();
        if let Some(handle) = self.di_streams.borrow_mut().remove(chain_id) {
            if let Some(old) = handle.disarm() {
                self.di_retired.borrow_mut().push(old);
            }
        }
    }

    /// Controller-side re-arm after a stream/runtime rebuild: re-resolves the
    /// chosen output (index, rate, dest channels may all have changed) and
    /// re-renders from the source stored at arm time. No-op when not armed.
    pub fn rearm_di_stream_after_rebuild(&self, chain: &Chain) {
        let pcm = self
            .di_streams
            .borrow()
            .get(&chain.id)
            .map(|h| Arc::clone(&h.pcm));
        if let Some(pcm) = pcm {
            let _ = self.arm_di_stream(chain, pcm);
        }
    }

    /// Drop every DI resource of a removed chain (handle, cells, retirees) so
    /// deleting chains never leaks parked render buffers.
    pub(crate) fn drop_di_state_for_chain(&self, chain_id: &ChainId) {
        self.disarm_di_stream(chain_id);
        self.di_playback_cells
            .borrow_mut()
            .retain(|(cid, _), _| cid != chain_id);
    }

    /// Whether the chain's DI is playing or still rendering. A FAILED render
    /// reports `false`, so the UI resets instead of showing an eternal
    /// silent "playing".
    pub fn di_stream_active(&self, chain_id: &ChainId) -> bool {
        self.di_streams
            .borrow()
            .get(chain_id)
            .is_some_and(|h| !h.failed.load(Ordering::Relaxed))
    }

    /// Length (frames) of one loop period — `None` while the worker is still
    /// building the runtime or when not armed.
    pub fn di_stream_loop_len(&self, chain_id: &ChainId) -> Option<usize> {
        self.di_streams
            .borrow()
            .get(chain_id)
            .and_then(|h| h.cell.load().as_ref().map(|p| p.loop_len()))
    }

    /// #771: which flat output index currently has the chain's pre-rendered
    /// playback parked, if any. `None` while the render is still running or
    /// when the DI is not armed.
    pub fn di_playback_active_output(&self, chain_id: &ChainId) -> Option<usize> {
        self.di_streams
            .borrow()
            .get(chain_id)
            .and_then(|h| h.cell.load().is_some().then_some(h.output_index))
    }

    /// Linear `(in, out)` peaks of the DI playback's last mixed window — the
    /// DI meter row's source (the DI's OWN levels, not the chain's).
    pub fn di_playback_peaks(&self, chain_id: &ChainId) -> Option<(f32, f32)> {
        self.di_streams
            .borrow()
            .get(chain_id)
            .and_then(|h| h.cell.load().as_ref().map(|p| p.peaks()))
    }
}
