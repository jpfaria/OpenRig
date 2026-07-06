//! Issue #717 — an armed DI loop plays on its own dedicated, isolated runtime
//! (a copy of the chain's block graph), never injected into the guitar's
//! runtime. `arm_di_stream` builds that separate runtime and holds it; the
//! guitar runtime is left untouched, so guitar and DI coexist fully isolated
//! (invariant #4).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::Result;

use domain::ids::ChainId;
use engine::runtime::build_chain_runtime_state;
use engine::spsc::SpscRing;
use engine::DiPcm;
use project::chain::Chain;

use crate::{LiveRuntimeSlot, ProjectRuntimeController};

/// Buffer size the DI worker clocks the runtime at. Meters only need a steady
/// tick; this paces ~one buffer every `frames / rate` seconds.
const DI_WORKER_FRAMES: usize = 256;

/// Self-clocked thread that steps the DI runtime buffer by buffer (the driver
/// Candidate B calls for). The armed loop substitutes the silent device input,
/// so each step fills the runtime's meter taps + output route. Runs off the
/// audio callback (like `dsp_worker`), so pacing by sleep is fine. Dropping the
/// worker stops and joins the thread.
struct DiWorker {
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl DiWorker {
    fn spawn(slot: LiveRuntimeSlot, sample_rate: u32) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_flag = Arc::clone(&stop);
        let period = Duration::from_secs_f64(DI_WORKER_FRAMES as f64 / sample_rate.max(1) as f64);
        let join = std::thread::Builder::new()
            .name("di-worker".into())
            .spawn(move || {
                // Silent device input; the loop provides the real signal.
                let silence = vec![0.0f32; DI_WORKER_FRAMES];
                while !stop_flag.load(Ordering::Relaxed) {
                    crate::slot_processing::process_input_buffer(&slot, 0, &silence, 1);
                    std::thread::sleep(period);
                }
            })
            .expect("spawn DI worker thread");
        Self {
            stop,
            join: Some(join),
        }
    }
}

impl Drop for DiWorker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

/// A live dedicated DI runtime for one chain, alive only while the DI is armed.
/// Holds the isolated runtime via its slot + the worker that clocks it; dropping
/// the handle stops the worker and tears the runtime down.
pub(crate) struct DiStreamHandle {
    pub(crate) slot: LiveRuntimeSlot,
    _worker: DiWorker,
}

impl ProjectRuntimeController {
    /// Build a fresh, independent runtime from `chain`'s block graph, feed it
    /// the loop, and hold it — NEVER the guitar runtime (#717, invariant #4).
    /// The engine defaults every route's elastic cushion here; Task 4 sizes it
    /// to the chain's chosen output once that output is resolved.
    pub fn arm_di_stream(&self, chain: &Chain, pcm: Arc<DiPcm>) -> Result<()> {
        let runtime = Arc::new(build_chain_runtime_state(
            chain,
            self.sample_rate as f32,
            &[],
            &self.io_bindings,
        )?);
        let rate = runtime.sample_rate() as u32;
        runtime.set_di_loop(Some(Arc::new(pcm.to_loop_at(rate))));
        let slot = LiveRuntimeSlot::new(runtime);
        let worker = DiWorker::spawn(slot.handle(), rate);
        // NOTE: the dedicated runtime drives the DI GRAPH/METERS only. Draining it
        // straight onto the output device (a free-running worker clock vs the
        // device clock) drifts and dropouts — the sound must instead come from a
        // pre-rendered, output-clocked player (follow-up). So no output routing
        // here; the chain's normal path carries the DI audio.
        self.di_streams.borrow_mut().insert(
            chain.id.clone(),
            DiStreamHandle {
                slot,
                _worker: worker,
            },
        );
        Ok(())
    }

    /// Tear the chain's dedicated DI runtime down: stop its worker, drop the
    /// runtime + loop.
    pub fn disarm_di_stream(&self, chain_id: &ChainId) {
        // Dropping the handle stops+joins the worker and drops the runtime.
        self.di_streams.borrow_mut().remove(chain_id);
    }

    /// Whether a dedicated DI runtime is currently armed for the chain.
    pub fn di_stream_active(&self, chain_id: &ChainId) -> bool {
        self.di_streams.borrow().contains_key(chain_id)
    }

    /// Length of the loop carried by the chain's dedicated DI runtime, if
    /// armed. Mirrors [`Self::chain_di_loop_len`] but reads the DI runtime, not
    /// the guitar — proving the loop rides the separate stream.
    pub fn di_stream_loop_len(&self, chain_id: &ChainId) -> Option<usize> {
        self.di_streams
            .borrow()
            .get(chain_id)
            .and_then(|h| h.slot.load().di_loop_len())
    }

    /// Subscribe the DI runtime's per-stream OUTPUT tap (post-FX stereo), for
    /// the dedicated DI graph's meters. Mirrors [`Self::subscribe_stream_tap`]
    /// but reads the isolated DI runtime, not the guitar. `None` if not armed.
    pub fn di_subscribe_stream_tap(
        &self,
        chain_id: &ChainId,
        stream_index: usize,
        capacity_per_channel: usize,
    ) -> Option<[Arc<SpscRing<f32>>; 2]> {
        self.di_streams
            .borrow()
            .get(chain_id)
            .map(|h| h.slot.load().subscribe_stream_tap(stream_index, capacity_per_channel))
    }

    /// How many streams the chain's DI runtime runs (0 if not armed).
    pub fn di_stream_count(&self, chain_id: &ChainId) -> usize {
        self.di_streams
            .borrow()
            .get(chain_id)
            .map(|h| h.slot.load().stream_count())
            .unwrap_or(0)
    }
}
