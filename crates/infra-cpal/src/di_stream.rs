//! Issue #771 — an armed DI loop plays on its own isolated, STREAMED
//! runtime, output-clocked via ring backpressure, never injected into the
//! guitar's runtime.
//!
//! This module owns the controller-side lifecycle: arm, disarm, re-arm and the
//! queries the UI reads. The render thread itself lives in `di_stream_worker`.
//!
//! Arming resolves the chain's persisted output choice (`Chain.di_output`),
//! builds a fresh routed copy of the chain's block graph on a `di-stream`
//! worker and parks a ring-backed playback in the CHOSEN output's
//! [`DiPlaybackCell`] (a 75 s loop starts in milliseconds — the full pre-render
//! tried first took minutes before the first sample).
//!
//! Lifecycle rules (review findings, #771; hand-off, #785):
//! - The render thread parks ONLY under an `armed` lock, and a disarm flips it
//!   under the same lock — a disarm ALWAYS wins over an in-flight render (no
//!   zombie playback). The lock lives entirely off the audio thread.
//! - A retired playback is never dropped by the audio callback: it is parked in
//!   `di_retired` and freed on a LATER cycle, long after any in-flight callback
//!   guard is gone.
//! - A failed render flips the handle's `failed` flag; `di_stream_active` then
//!   reports NOT playing (the meter poll resets the UI) instead of an eternal
//!   silent "playing".
//! - A live edit re-arms GAPLESSLY: the outgoing playback keeps sounding while
//!   the incoming render builds and pre-rolls, and the incoming worker takes
//!   the cell over mid-loop. The handle therefore tracks EVERY live worker of
//!   the chain, so the one that takes over stops all it supersedes and a disarm
//!   stops whatever is left.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::Result;

use domain::ids::ChainId;
use engine::di_output_resolve::resolve_di_output_index;
use engine::runtime_endpoints::resolve_chain_io;
use engine::DiPcm;
use project::chain::Chain;

use crate::di_playback::{DiPlayback, DiPlaybackCell};
use crate::di_stream_worker::{self, DiHandoff, DiWorkerSpec};
use crate::ProjectRuntimeController;

/// Bookkeeping for one armed DI. Dropping the handle disarms (safety net);
/// the controller's `disarm_di_stream` is the primary path because it also
/// retires the parked playback off the audio thread.
pub(crate) struct DiStreamHandle {
    output_index: usize,
    cell: DiPlaybackCell,
    /// #785: the arm flags of EVERY render thread still alive for this chain —
    /// this arm's (last) plus any it superseded. Each thread runs while its own
    /// flag is `true`. Edits arrive faster than a render builds, so a hand-off
    /// can find several workers in flight; the one that takes over stops all of
    /// them, and a disarm stops the survivors. Tracking only the latest left the
    /// worker feeding the playback the listener was hearing running forever.
    workers: Vec<Arc<Mutex<bool>>>,
    /// The source, kept so the controller can re-arm after a rebuild
    /// without a dispatcher round-trip.
    pcm: Arc<DiPcm>,
    /// Set by the render thread on failure; surfaces through
    /// [`ProjectRuntimeController::di_stream_active`].
    failed: Arc<AtomicBool>,
    /// #785: a gapless re-arm supersedes this handle while its playback is
    /// still sounding — the incoming worker owns stopping it. Dropping a
    /// superseded handle must NOT empty the cell; that is exactly the teardown
    /// the listener heard as a cut.
    superseded: bool,
    /// #808: the DI's OWN output stream — a fully isolated cpal output on the
    /// chain's chosen output device that drains this cell (invariant #4: the DI
    /// never shares the chain's output stream; the backend sums them on the
    /// device). Present whenever the DI is armed, so it plays with or without
    /// an active guitar stream. `None` on the JACK build (Orange Pi keeps the
    /// port-mix path) and when a re-arm hands the live stream to a new handle.
    /// Carries the stream's sample rate + the interface output buffer (frames)
    /// so a re-arm renders at the rate the stream consumes and the worker leads
    /// by the interface buffer, without re-querying the device.
    output_stream: Option<(cpal::Stream, u32, u32)>,
    /// #808: the live runtime the worker steps. A param edit rebuilds the routed
    /// runtime off-thread and swaps it in here (`update_di_runtime`), so the tone
    /// changes gaplessly with NO worker/stream respawn — the same wait-free swap
    /// the guitar path uses. Kept in the handle so re-arms carry it forward.
    live_runtime: Arc<arc_swap::ArcSwapOption<engine::runtime::ChainRuntimeState>>,
    /// #808: runtimes the render thread retired on a live swap. A (NAM) runtime
    /// must NEVER be dropped on the render thread (its C++ destructor there
    /// stalls/kills the DI — invariant #8; the guitar rebuild drops off-thread
    /// too). The render thread pushes the outgoing runtime here; the control
    /// worker drains and frees it.
    graveyard: Arc<Mutex<Vec<Arc<engine::runtime::ChainRuntimeState>>>>,
}

/// Stop every render thread in `workers` (idempotent).
fn stop_workers(workers: &[Arc<Mutex<bool>>]) {
    for worker in workers {
        let mut armed = worker.lock().unwrap_or_else(|e| e.into_inner());
        *armed = false;
    }
}

impl DiStreamHandle {
    fn disarm(&self) -> Option<Arc<DiPlayback>> {
        stop_workers(&self.workers);
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

    /// #808: build the DI's OWN cpal output stream on the chain's chosen output
    /// device, draining `cell`. Fully isolated (invariant #4): the DI never
    /// shares the chain's output stream, so a chain rebuild/edit cannot chop it,
    /// and it plays with or without an active guitar stream. Best-effort — on a
    /// resolve/config failure the DI still renders (heard once an output exists).
    /// Returns the stream + its sample rate so the render matches the stream.
    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    fn build_di_output_stream(
        &self,
        chain: &Chain,
        output_index: usize,
        cell: &DiPlaybackCell,
    ) -> Option<(cpal::Stream, u32, u32)> {
        use cpal::traits::{DeviceTrait, StreamTrait};
        let (_, outputs) = resolve_chain_io(chain, &self.io_bindings);
        let out = outputs.get(output_index)?;
        let host = crate::host::get_host();
        let device = crate::find_output_device_by_id(host, &out.device_id.0).ok()??;
        let supported = device.default_output_config().ok()?;
        let rate = supported.sample_rate();
        let resolved = crate::resolved::ResolvedOutputDevice {
            device_id: out.device_id.0.clone(),
            settings: None,
            device,
            supported,
        };
        // #808: the DI buffers to the INTERFACE's output buffer (like a normal
        // stream), not a hardcoded ring — so its latency/resilience match the
        // guitar's and a live edit lands within a few interface buffers.
        let buffer_frames = crate::stream_config::resolved_output_buffer_size_frames(&resolved);
        let stream = crate::stream_builder::build_output_stream_for_output(
            &chain.id,
            output_index,
            resolved,
            Vec::new(), // no chain runtime slots — this stream plays ONLY the DI
            cell.clone(),
        )
        .ok()?;
        stream.play().ok()?;
        Some((stream, rate, buffer_frames))
    }

    /// JACK build (Orange Pi) keeps the port-mix DI path unchanged (#808 wires
    /// the dedicated cpal output first).
    #[cfg(all(target_os = "linux", feature = "jack"))]
    fn build_di_output_stream(
        &self,
        _chain: &Chain,
        _output_index: usize,
        _cell: &DiPlaybackCell,
    ) -> Option<(cpal::Stream, u32, u32)> {
        None
    }

    /// Arm the chain's DI: resolve the chosen output, build the routed
    /// runtime off-thread and stream the loop into that output's cell. The
    /// guitar runtime is NEVER touched.
    pub fn arm_di_stream(&self, chain: &Chain, pcm: Arc<DiPcm>) -> Result<()> {
        // A fresh arm replaces any previous playback (and retires it off the
        // audio thread) — the listener asked for this one.
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
        let (_, outputs) = resolve_chain_io(chain, &self.io_bindings);
        let dest = outputs
            .get(output_index)
            .map(|o| o.channels.clone())
            .unwrap_or_default();
        let dest_left = dest.first().copied().unwrap_or(0);
        let dest_right = dest.get(1).copied().unwrap_or(dest_left);

        let cell = self.di_playback_cell(&chain.id, output_index);

        // #808: the DI's OWN output stream (invariant #4 — never the chain's).
        // Take the previous handle out first so a gapless re-arm on the SAME
        // output REUSES the live stream (no restart, no re-open), and a moved
        // output rebuilds it. `remove` (not `insert`) so the reused stream is
        // out of the old handle before it drops.
        let mut prev = self.di_streams.borrow_mut().remove(&chain.id);
        let same_output = prev
            .as_ref()
            .and_then(|h| h.output_stream.as_ref().map(|_| h.output_index))
            == Some(output_index);
        let output_stream = if same_output {
            prev.as_mut().and_then(|h| h.output_stream.take())
        } else {
            if let Some(h) = prev.as_mut() {
                h.output_stream = None; // moved output: drop the old stream
            }
            self.build_di_output_stream(chain, output_index, &cell)
        };
        // Render at the rate the DI's own stream consumes; fall back to the
        // controller's resolved rate when there is no dedicated stream (JACK, or
        // a failed device resolve).
        let output_rate = output_stream
            .as_ref()
            .map(|(_, r, _)| *r)
            .unwrap_or_else(|| self.di_output_rate(&chain.id, output_index));
        // #808: the worker leads by the INTERFACE output buffer (a few of them),
        // so the DI buffers like a normal stream — low, interface-matched latency.
        let buffer_frames = output_stream.as_ref().map(|(_, _, b)| *b).unwrap_or(256);

        let armed = Arc::new(Mutex::new(true));
        let failed = Arc::new(AtomicBool::new(false));
        // #808: the worker publishes its runtime here after building; a param
        // edit swaps a fresh one in (update_di_runtime) with no respawn.
        let live_runtime = Arc::new(arc_swap::ArcSwapOption::from(None));
        let graveyard: Arc<Mutex<Vec<Arc<engine::runtime::ChainRuntimeState>>>> =
            Arc::new(Mutex::new(Vec::new()));
        let handoff_pending = handoff.is_some();
        // Every worker the incoming one supersedes, plus itself: whoever takes
        // the cell over stops the others, and a disarm stops the survivors.
        let mut workers: Vec<Arc<Mutex<bool>>> = handoff
            .as_ref()
            .map(|h| h.prev_workers.clone())
            .unwrap_or_default();
        workers.push(Arc::clone(&armed));

        di_stream_worker::spawn(DiWorkerSpec {
            chain: chain.clone(),
            registry: self.io_bindings.clone(),
            pcm: Arc::clone(&pcm),
            output_rate,
            dest_left,
            dest_right,
            cell: cell.clone(),
            armed: Arc::clone(&armed),
            failed: Arc::clone(&failed),
            retired: Arc::clone(&self.di_retired),
            handoff,
            live_runtime: Arc::clone(&live_runtime),
            buffer_frames,
            graveyard: Arc::clone(&graveyard),
        });

        self.di_streams.borrow_mut().insert(
            chain.id.clone(),
            DiStreamHandle {
                output_index,
                cell,
                workers,
                pcm,
                failed,
                superseded: false,
                output_stream,
                live_runtime,
                graveyard,
            },
        );
        // A hand-off replaces the handle while its playback is still sounding:
        // the incoming worker owns stopping it. Dropping `prev` as-is would empty
        // the cell — the cut #785 is about. Its output_stream was already taken
        // (reuse) or cleared (rebuild) above, so its Drop stops no live stream.
        if let Some(mut old) = prev {
            old.superseded = handoff_pending;
        }
        Ok(())
    }

    /// Disarm the chain's DI: every live render thread is stopped under its arm
    /// lock, and the parked playback is RETIRED — freed on a later cycle, never
    /// by the audio callback (invariant #8).
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
                        prev_workers: h.workers.clone(),
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

    /// #808: a live param/config edit — rebuild the routed runtime off-thread
    /// and SWAP it into the DI worker's live slot (wait-free). GAPLESS with NO
    /// worker or output-stream respawn — that respawn on every edit was the
    /// owner's "parou som"/"picotando". The DI keeps its own output stream and
    /// cell untouched; only the DSP the worker steps changes. No-op when the DI
    /// is not armed. Use this for a param/block edit; `rearm` (respawn) stays for
    /// arm/disarm and an output-device change.
    pub fn update_di_runtime(&self, chain: &Chain) {
        let armed = self.di_streams.borrow().get(&chain.id).map(|h| {
            (
                Arc::clone(&h.live_runtime),
                Arc::clone(&h.pcm),
                h.output_index,
                h.output_stream.as_ref().map(|(_, r, _)| *r),
                Arc::clone(&h.graveyard),
            )
        });
        let Some((live_runtime, pcm, output_index, stream_rate, graveyard)) = armed else {
            return; // not armed — nothing to update
        };
        let chain = chain.clone();
        let registry = self.io_bindings.clone();
        // Build at the rate the DI's OWN output stream consumes (the device
        // rate), not the fallback — a mismatch renders the loop at the wrong
        // speed/position (silence/garbage after the swap).
        let output_rate = stream_rate.unwrap_or_else(|| self.di_output_rate(&chain.id, output_index));
        // Heavy (NAM/IR) build off the frontend; the swap itself is wait-free.
        let _ = self.worker.submit(move || -> Result<()> {
            match engine::di_render::build_routed_di_runtime(
                &chain,
                &registry,
                chain.di_output.as_ref(),
                output_rate,
                &pcm,
            ) {
                Ok(routed) => live_runtime.store(Some(routed.runtime)),
                Err(e) => log::error!("di update build failed for '{}': {e:#}", chain.id.0),
            }
            // #808: free the runtimes the render thread retired on its swap —
            // here, on the control worker, NEVER on the render thread (NAM C++
            // destructor there stalls/kills the DI).
            let retired: Vec<_> = graveyard
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .drain(..)
                .collect();
            drop(retired);
            Ok(())
        });
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
