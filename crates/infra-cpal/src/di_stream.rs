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
    /// #785: a gapless re-arm hands this handle's playback over to the
    /// incoming render, which stops the old worker and retires the old
    /// playback itself. Dropping the superseded handle must NOT empty the
    /// cell — that is exactly the teardown the listener heard as a cut.
    superseded: bool,
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
        if self.superseded {
            return;
        }
        // Safety net for non-controller drops (controller teardown). The
        // swapped-out playback drops here, on a non-audio thread.
        let _ = self.disarm();
    }
}

/// What a gapless re-arm (#785) hands to the incoming render: the playback the
/// listener is hearing, and the outgoing worker's arm flag, so the incoming
/// render can line itself up with it and stop it at the hand-off.
struct DiHandoff {
    prev: Arc<DiPlayback>,
    prev_armed: Arc<Mutex<bool>>,
}

/// Frames the incoming render pre-rolls before it can take over. The outgoing
/// playback keeps sounding until the listener reaches this position, so the
/// hand-off is both gapless and continuous in the loop — no restart, no jump.
const HANDOFF_PREROLL_FRAMES: usize = 8192;

/// A stalled output (no callback consuming) would never reach the hand-off
/// position. After this long, the incoming render takes over anyway.
const HANDOFF_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

/// Has the outgoing playback reached the loop position the incoming render
/// starts from (within one worker block)? `tol` also absorbs the callback
/// overshooting the exact frame — the position may land just PAST `start_pos`.
fn handoff_reached(prev: &DiPlayback, start_pos: usize, loop_len: usize, tol: usize) -> bool {
    let loop_len = loop_len.max(1);
    let ahead = (start_pos + loop_len - prev.play_pos() % loop_len) % loop_len;
    ahead <= tol || ahead >= loop_len.saturating_sub(tol)
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
        self.spawn_di_stream(chain, pcm, None)
    }

    /// Spawn the render worker for `chain`. With a `handoff`, the incoming
    /// render takes over from the playback that is sounding (#785) instead of
    /// the cell being emptied first.
    fn spawn_di_stream(
        &self,
        chain: &Chain,
        pcm: Arc<DiPcm>,
        handoff: Option<DiHandoff>,
    ) -> Result<()> {
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
        let handoff_pending = handoff.is_some();
        {
            let cell = cell.clone();
            let armed = Arc::clone(&armed);
            let failed = Arc::clone(&failed);
            let chain = chain.clone();
            let registry = self.io_bindings.clone();
            let pcm = Arc::clone(&pcm);
            let retired = Arc::clone(&self.di_retired);
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

                    // Parked below only after a ~100 ms pre-buffer, so
                    // playback starts with a cushion instead of racing the
                    // worker from frame one (still well under a second for a
                    // 75 s loop — the pre-render took minutes).
                    //
                    // #785: on a gapless re-arm the outgoing playback is still
                    // sounding, so this render starts where the listener WILL
                    // be once the pre-roll is ready — not at the top of the
                    // loop. It then waits for the listener to reach exactly
                    // that position before taking the cell over, so the edit
                    // lands with neither a silent gap nor a jump in the loop.
                    let loop_len = routed.loop_len.max(1);
                    let start_pos = handoff
                        .as_ref()
                        .map(|h| (h.prev.play_pos() + HANDOFF_PREROLL_FRAMES) % loop_len)
                        .unwrap_or(0);
                    let playback = Arc::new(DiPlayback::starting_at(
                        dest_left,
                        dest_right,
                        routed.loop_len,
                        start_pos,
                    ));
                    let ring = playback.ring();
                    let mut parked = false;
                    let handoff_deadline = std::time::Instant::now() + HANDOFF_TIMEOUT;
                    routed.runtime.set_di_loop_pos(start_pos);

                    // Stream the DI: paced by RING BACKPRESSURE — the worker
                    // only produces what the output consumed, so the output
                    // device clock IS the DI clock (no drift by construction;
                    // the sleep-paced worker tried in #717 drifted and was
                    // reverted, f1131725e). All DSP runs HERE, never in the
                    // callback (invariant #8).
                    const BLOCK: usize = 256;
                    // Scheduling shape matters more than raw priority here
                    // (#698 lesson, re-measured live on the owner's rig):
                    // - normal priority + continuous burn → 71-88% fill
                    //   (preempted by the GUI + the guitar's RT worker);
                    // - RT class + continuous burn → 38% fill (the kernel
                    //   demotes a time-constraint thread that blows through
                    //   its declared computation budget for seconds).
                    // The guitar's dsp_worker sustains the SAME chain cost
                    // in debug because it works in SHORT BURSTS. Mirror it:
                    // one block per iteration, a breath every few blocks,
                    // and an honest RT declaration sized to that cadence.
                    let period_ns = (BLOCK as u64) * 1_000_000_000 / (output_rate.max(1) as u64);
                    crate::dsp_worker::promote_to_audio_rt(period_ns, period_ns * 3 / 5);
                    let silence = vec![0.0f32; BLOCK];
                    let mut drain = vec![0.0f32; BLOCK * routed.drain_width];
                    let mut pos: usize = start_pos;
                    // Catch-up bursts are capped: after every few back-to-back
                    // blocks the worker yields, so it never presents the
                    // scheduler with a multi-second monolithic burn (that is
                    // what collapsed throughput to 38%).
                    let mut burst: u32 = 0;
                    loop {
                        {
                            let armed = armed.lock().unwrap_or_else(|e| e.into_inner());
                            if !*armed {
                                return;
                            }
                        }
                        // Park BEFORE the backpressure check: a gapless re-arm
                        // waits, with a full ring, for the listener to reach
                        // the hand-off position — the take-over must still be
                        // evaluated while the worker is resting.
                        let park_fill = match handoff {
                            Some(_) => HANDOFF_PREROLL_FRAMES * 2,
                            None => BLOCK * 2 * 16,
                        };
                        if !parked && ring.len() >= park_fill {
                            let armed = armed.lock().unwrap_or_else(|e| e.into_inner());
                            if !*armed {
                                return;
                            }
                            match handoff.as_ref() {
                                // Cold arm: nothing is sounding, park at once.
                                None => {
                                    cell.store(Some(Arc::clone(&playback)));
                                    parked = true;
                                }
                                // Gapless re-arm (#785): the outgoing playback
                                // keeps sounding until the listener reaches the
                                // position this render started from. Then, under
                                // the outgoing arm lock, stop the old worker and
                                // swap the cell — no teardown, no silence, no
                                // jump in the loop.
                                Some(h) => {
                                    if handoff_reached(&h.prev, start_pos, loop_len, BLOCK)
                                        || std::time::Instant::now() >= handoff_deadline
                                    {
                                        let mut prev_armed =
                                            h.prev_armed.lock().unwrap_or_else(|e| e.into_inner());
                                        *prev_armed = false;
                                        cell.store(Some(Arc::clone(&playback)));
                                        // An in-flight callback may still hold a
                                        // guard on the outgoing playback: retire
                                        // it instead of dropping it here, so the
                                        // audio thread never frees it (#8).
                                        retired
                                            .lock()
                                            .unwrap_or_else(|e| e.into_inner())
                                            .push(Arc::clone(&h.prev));
                                        parked = true;
                                    }
                                }
                            }
                        }
                        let free = ring.capacity() - ring.len();
                        if free < BLOCK * 2 {
                            // Ring topped up: rest half a block period.
                            burst = 0;
                            std::thread::sleep(std::time::Duration::from_nanos(period_ns / 2));
                            continue;
                        }
                        if burst >= 4 {
                            burst = 0;
                            std::thread::sleep(std::time::Duration::from_millis(1));
                            continue;
                        }
                        burst += 1;
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
        let superseded = self.di_streams.borrow_mut().insert(
            chain.id.clone(),
            DiStreamHandle {
                output_index,
                cell,
                armed,
                pcm,
                failed,
                superseded: false,
            },
        );
        // A hand-off replaces the handle while its playback is still sounding:
        // the incoming worker owns stopping it. Dropping it as-is would empty
        // the cell — the very cut #785 is about.
        if let Some(mut old) = superseded {
            old.superseded = handoff_pending;
        }
        Ok(())
    }

    /// Disarm the chain's DI: the in-flight render (if any) is neutralized
    /// under the handle's lock, and the parked playback is RETIRED — freed on
    /// a later cycle, never by the audio callback (invariant #8).
    pub fn disarm_di_stream(&self, chain_id: &ChainId) {
        // Free the previous cycle's retirees first: by now any callback
        // guard that referenced them (a µs-scale window) is long gone.
        self.retired().clear();
        if let Some(handle) = self.di_streams.borrow_mut().remove(chain_id) {
            if let Some(old) = handle.disarm() {
                self.retired().push(old);
            }
        }
    }

    fn retired(&self) -> std::sync::MutexGuard<'_, Vec<Arc<DiPlayback>>> {
        self.di_retired.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Controller-side re-arm after a stream/runtime rebuild: re-resolves the
    /// chosen output (index, rate, dest channels may all have changed) and
    /// re-renders from the source stored at arm time. No-op when not armed.
    ///
    /// #785: GAPLESS. The playback the listener is hearing keeps sounding while
    /// the new render is built and pre-rolled off-thread; the incoming worker
    /// takes the cell over mid-loop, at the position the listener reaches. The
    /// old teardown-then-rebuild cut the DI on EVERY live edit — a param change
    /// or a block toggle — while the guitar chain kept sounding.
    pub fn rearm_di_stream_after_rebuild(&self, chain: &Chain) {
        let armed_now = {
            let streams = self.di_streams.borrow();
            streams.get(&chain.id).map(|h| {
                (
                    Arc::clone(&h.pcm),
                    h.cell.load_full().map(|prev| DiHandoff {
                        prev,
                        prev_armed: Arc::clone(&h.armed),
                    }),
                )
            })
        };
        let Some((pcm, handoff)) = armed_now else {
            return; // not armed — nothing to re-render
        };
        match handoff {
            // Nothing parked yet (the first render is still building): there is
            // no playback to preserve, so a plain re-arm is already gapless.
            None => {
                let _ = self.arm_di_stream(chain, pcm);
            }
            Some(handoff) => {
                let _ = self.spawn_di_stream(chain, pcm, Some(handoff));
            }
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
