//! #831: the GUI's live readings, as [`LiveSource`].
//!
//! Everything here is state that only exists inside the running window —
//! the meter rows, the tuner/spectrum sessions, the audio runtime. The
//! resolver in `application::read` owns the wire shape for all of it; this
//! module only hands over the DATA the GUI already has, never JSON.
//!
//! A `None` means "the GUI is not hosting this right now" (the tuner window
//! is closed, the project is not started) — never "the reading failed" and
//! never a fabricated row. The resolver answers those with the documented
//! empty shape, so a client reads the same fields whichever frontend and
//! whichever transport served it.

use std::cell::RefCell;
use std::rc::Rc;

use application::live_source::{
    AudioHealthReading, BlockErrorReading, ChainMeterReading, ChainRuntimeReading, LiveSource,
    MetronomeReading,
};
use application::query_analyzers::{SpectrumReading, TunerReading};
use application::looper_edit::LoopEditReading;
use application::query_di::DiLoopReading;
use engine::loop_edit;
use domain::ids::{BlockId, ChainId};
use domain::io_binding::IoBinding;
use engine::LooperStatus;
use infra_cpal::ProjectRuntimeController;
use project::project::Project;
use slint::{Model, VecModel};

use crate::spectrum_session::SpectrumSession;
use crate::state::ProjectSession;
use crate::tuner_session::TunerSession;
use crate::ProjectChainItem;

/// Live GUI handles, borrowed for the length of one read. Nothing is
/// cached, so a reply always reflects the frame the user is looking at.
pub(crate) struct GuiLiveSource<'a> {
    /// The project the rows are aligned with — the rows carry display
    /// values only, so the chain identity of row `i` is chain `i`.
    pub(crate) project: &'a Project,
    /// Chain rows the GUI meters write into (`meter_in_dbfs` / `meter_out_dbfs`).
    pub(crate) chain_rows: &'a Rc<VecModel<ProjectChainItem>>,
    /// #716: the per-machine binding registry a chain's device endpoints —
    /// and therefore its real sample rate — resolve against when no runtime
    /// is up to report one.
    pub(crate) io_bindings: &'a [IoBinding],
    pub(crate) tuner: &'a Rc<RefCell<Option<TunerSession>>>,
    pub(crate) spectrum: &'a Rc<RefCell<Option<SpectrumSession>>>,
    /// Live runtime — DI playback state, DI peaks and looper transport
    /// state come from it, per chain.
    pub(crate) runtime: &'a Rc<RefCell<Option<ProjectRuntimeController>>>,
}

impl LiveSource for GuiLiveSource<'_> {
    /// The numbers the IN/OUT bars are drawing, read from the rows they are
    /// bound to. Deliberately NOT a second poll of the audio taps: a second
    /// read would let the screen and the transport disagree, and would put
    /// extra work on the audio path.
    fn chain_meters(&self) -> Option<Vec<ChainMeterReading>> {
        Some(
            self.project
                .chains
                .iter()
                .enumerate()
                .filter_map(|(idx, chain)| {
                    self.chain_rows.row_data(idx).map(|row| ChainMeterReading {
                        chain: chain.id.clone(),
                        in_dbfs: row.meter_in_dbfs,
                        out_dbfs: row.meter_out_dbfs,
                    })
                })
                .collect(),
        )
    }

    /// #829: the same rows the Tuner window renders. No session (window
    /// closed / tuner powered off) ⇒ not hosted.
    fn tuner(&self) -> Option<Vec<TunerReading>> {
        self.tuner.borrow().as_ref().map(TunerSession::readings)
    }

    fn spectrum(&self) -> Option<Vec<SpectrumReading>> {
        self.spectrum
            .borrow()
            .as_ref()
            .map(SpectrumSession::readings)
    }

    /// Per-chain DI loop state from the live controller — the same
    /// `di_stream_active` / `di_playback_peaks` the chain tile shows.
    ///
    /// `source` is filled by the resolver from the dispatcher (the only
    /// owner of that state) and whatever is set here is discarded, so it
    /// cannot drift between transports.
    fn di_loop(&self) -> Option<Vec<DiLoopReading>> {
        let runtime = self.runtime.borrow();
        let controller = runtime.as_ref()?;
        Some(
            self.project
                .chains
                .iter()
                .filter_map(|chain| di_reading(controller, &chain.id))
                .collect(),
        )
    }

    /// The per-chain half of [`Self::di_loop`], through the same helper.
    fn chain_di_loop(&self, chain: &ChainId) -> Option<DiLoopReading> {
        let runtime = self.runtime.borrow();
        di_reading(runtime.as_ref()?, chain)
    }

    /// #127: the rate THIS chain runs at, or would be opened at with the rig
    /// stopped — the one the latency probe measures against.
    ///
    /// Same resolution as [`Self::chain_loopers`], and for the same reason: the
    /// GUI owns the binding registry and the audio host, so a stopped rig still
    /// has a real, measurable rate for the chain's own device. `None` when it
    /// cannot be resolved — the caller falls back to the dispatcher's tracked
    /// rate rather than being handed a guess (#723).
    fn chain_sample_rate(&self, chain: &ChainId) -> Option<f32> {
        resolve_chain_rate(self.runtime, self.project, self.io_bindings, chain)
    }

    /// #323: the chain's live looper transport state, at THIS chain's real
    /// rate — never a constant (issue #723).
    ///
    /// Running: the statuses and the rate the streams are actually running
    /// at, both from the controller. Stopped: there is no transport state,
    /// but the rate is still a real, measurable property of the chain's own
    /// device — the GUI owns the binding registry and the audio host, so it
    /// resolves that rate the same way `build_streams` does, keyed by this
    /// chain's id (a sibling chain never leaks into the answer). A rate that
    /// cannot be resolved is a real failure and propagates as one; never
    /// falls through to a tracked/default engine rate.
    fn chain_loopers(&self, chain: &ChainId) -> Option<Result<(Vec<LooperStatus>, u32), String>> {
        let runtime = self.runtime.borrow();
        if let Some(controller) = runtime.as_ref() {
            return Some(Ok((
                controller.chain_looper_statuses(chain),
                controller.sample_rate(),
            )));
        }
        let rate = infra_cpal::resolve_project_chain_sample_rates(self.project, self.io_bindings)
            .ok()
            .and_then(|rates| rates.get(chain).copied())
            .ok_or_else(|| format!("no resolved sample rate for chain {}", chain.0));
        Some(rate.map(|rate| (Vec::new(), rate.round() as u32)))
    }

    /// The GUI owns an audio host, so it always answers — an enumeration
    /// that FAILED is a real failure (a dead host, a JACK server that is
    /// down) and propagates as one, not as an empty listing.
    fn devices(&self) -> Option<Result<Vec<String>, String>> {
        Some(infra_cpal::list_devices().map_err(|e| e.to_string()))
    }

    fn metronome(&self) -> Option<MetronomeReading> {
        metronome_reading(self.runtime)
    }
}

/// #127: the metronome's live reading, on its own.
///
/// The click is an independent pipeline (invariant #4): its position depends
/// on no chain, no project row and no analyzer session, so this carries none
/// of the handles [`GuiLiveSource`] needs. It exists so `metronome_wiring` can
/// read the beat through the SEAM — the same one MCP reads — instead of
/// holding the audio backend itself.
pub(crate) struct MetronomeLiveSource {
    runtime: Rc<RefCell<Option<ProjectRuntimeController>>>,
}

impl LiveSource for MetronomeLiveSource {
    fn metronome(&self) -> Option<MetronomeReading> {
        metronome_reading(&self.runtime)
    }
}

/// Build the metronome's read seam. Called by `desktop_app`, the module that
/// allocates the shared runtime handle in the first place.
pub(crate) fn metronome_live_source(
    runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>,
) -> Rc<dyn LiveSource> {
    Rc::new(MetronomeLiveSource {
        runtime: Rc::clone(runtime),
    })
}

/// #127/#323: the loopers' live reading, on its own.
///
/// The looper panel redraws a chain's rows right after every dispatch, and it
/// needs exactly one thing the project cannot tell it: what each loop is
/// DOING, and at what rate it is counting. That is a finished reading — never
/// PCM — so it comes through the same seam MCP reads, and `looper_callbacks`
/// no longer holds the audio backend to get it.
///
/// Unlike [`GuiLiveSource::chain_loopers`] this answers `None` with no
/// controller instead of resolving a rate off the binding registry: the panel
/// draws its rows from the persisted config in that case, and a rate for a
/// device nobody opened would be a fiction (#723). MCP asks a different
/// question — "what rate would this chain run at?" — and gets the resolved
/// answer there.
pub(crate) struct LooperLiveSource {
    runtime: Rc<RefCell<Option<ProjectRuntimeController>>>,
}

impl LiveSource for LooperLiveSource {
    fn chain_loopers(&self, chain: &ChainId) -> Option<Result<(Vec<LooperStatus>, u32), String>> {
        let borrow = self.runtime.borrow();
        let controller = borrow.as_ref()?;
        Some(Ok((
            controller.chain_looper_statuses(chain),
            controller.sample_rate(),
        )))
    }

    /// #826: the loop's envelope for the waveform editor. The peaks are
    /// reduced HERE, on the side that owns the samples — the buffer itself
    /// never crosses the seam (the module's PCM rule).
    fn chain_loop_edit(
        &self,
        chain: &ChainId,
        looper: u64,
        buckets: usize,
    ) -> Option<LoopEditReading> {
        let borrow = self.runtime.borrow();
        let controller = borrow.as_ref()?;
        let pcm = controller.export_chain_looper_raw(chain, looper)?;
        let (can_undo, can_redo) = controller.looper_edit_history_depth(chain, looper);
        let len_frames = pcm.len() / 2;
        Some(LoopEditReading {
            peaks: loop_edit::peaks(&pcm, buckets),
            len_frames,
            length_label: crate::looper_view::clock_label(len_frames, controller.sample_rate()),
            can_undo: can_undo > 0,
            can_redo: can_redo > 0,
        })
    }
}

/// The rate one chain's streams run at (live controller) or would be opened at
/// (resolved from the project + this machine's bindings, exactly as
/// `build_streams` does). Keyed by the chain's own id, so a sibling chain never
/// leaks into the answer (`CLAUDE.md` LAW).
pub(crate) fn resolve_chain_rate(
    runtime: &RefCell<Option<ProjectRuntimeController>>,
    project: &Project,
    io_bindings: &[IoBinding],
    chain: &ChainId,
) -> Option<f32> {
    if let Some(controller) = runtime.borrow().as_ref() {
        return Some(controller.sample_rate() as f32);
    }
    infra_cpal::resolve_project_chain_sample_rates(project, io_bindings)
        .ok()
        .and_then(|rates| rates.get(chain).copied())
}

/// #127: the latency badge's read seam — the rate the probe must run at.
///
/// Its own `LiveSource` because it is asked from the chains screen, where the
/// project is behind the app's session cell rather than borrowed for the call
/// (the shape [`GuiLiveSource`] takes). Answers only `chain_sample_rate`; every
/// other reading stays at the trait's default.
pub(crate) struct ChainRateLiveSource {
    runtime: Rc<RefCell<Option<ProjectRuntimeController>>>,
    project_session: Rc<RefCell<Option<ProjectSession>>>,
}

impl LiveSource for ChainRateLiveSource {
    fn chain_sample_rate(&self, chain: &ChainId) -> Option<f32> {
        let borrow = self.project_session.borrow();
        let session = borrow.as_ref()?;
        let project = session.project.borrow();
        let bindings = session.io_bindings.borrow();
        resolve_chain_rate(&self.runtime, &project, &bindings, chain)
    }
}

/// Build the latency badge's read seam over the app's shared handles.
pub(crate) fn chain_rate_live_source(
    runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>,
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
) -> Rc<dyn LiveSource> {
    Rc::new(ChainRateLiveSource {
        runtime: Rc::clone(runtime),
        project_session: Rc::clone(project_session),
    })
}

/// Build the looper panel's read seam over the app's shared runtime handle.
pub(crate) fn looper_live_source(
    runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>,
) -> Rc<dyn LiveSource> {
    Rc::new(LooperLiveSource {
        runtime: Rc::clone(runtime),
    })
}

/// #127: what the poll tick reads — the block errors the audio thread
/// reported, and whether the backend is still there.
///
/// Its own `LiveSource` for the same reason the metronome has one: neither
/// reading depends on a project row, an analyzer session or a chain, so
/// carrying [`GuiLiveSource`]'s handles would be a lie about what the tick
/// looks at. With this, `desktop_app_polling` reads the runtime through the
/// seam instead of holding the backend.
pub(crate) struct HealthLiveSource {
    runtime: Rc<RefCell<Option<ProjectRuntimeController>>>,
}

impl LiveSource for HealthLiveSource {
    /// Drain the failures the audio thread posted, each already tagged with
    /// the chain that raised it. `None` ⇒ nothing is hosted, which is not the
    /// same as a hosted runtime with nothing to report.
    fn block_errors(&self) -> Option<Vec<BlockErrorReading>> {
        let borrow = self.runtime.borrow();
        let controller = borrow.as_ref()?;
        Some(
            controller
                .poll_errors()
                .into_iter()
                .map(|(chain, error)| BlockErrorReading {
                    chain,
                    block: error.block_id,
                    message: error.message,
                })
                .collect(),
        )
    }

    /// Is anything sounding, and does the backend still answer.
    ///
    /// `is_healthy` needs the controller mutably (the JACK supervisor's
    /// health check is a probe, not a getter), which is why this borrows the
    /// cell mutably for the length of the read — the same borrow the poll
    /// tick used to take itself.
    fn audio_health(&self) -> Option<AudioHealthReading> {
        let mut borrow = self.runtime.borrow_mut();
        let controller = borrow.as_mut()?;
        Some(AudioHealthReading {
            running: controller.is_running(),
            healthy: controller.is_healthy(),
        })
    }
}

/// #127: what a chain ROW redraws itself from, on the meter tick.
///
/// Three per-chain readings the project cannot answer: what the loops are
/// doing (and at what rate), whether the DI is playing and how loud, and
/// whether the chain has a live runtime plus its xrun/underrun counters. All
/// three are finished readings, all three carry the chain's identity, and
/// together they are what let `meter_wiring_poll` stop holding the backend.
pub(crate) struct ChainRowLiveSource {
    runtime: Rc<RefCell<Option<ProjectRuntimeController>>>,
}

impl LiveSource for ChainRowLiveSource {
    /// Same reading, same helper, as the whole-project [`GuiLiveSource::di_loop`]
    /// MCP reads — so the tile and the transport cannot drift.
    fn chain_di_loop(&self, chain: &ChainId) -> Option<DiLoopReading> {
        let borrow = self.runtime.borrow();
        di_reading(borrow.as_ref()?, chain)
    }

    /// One borrow, three numbers, all this chain's own. `is_empty` on
    /// `runtimes_for_chain` is what gates REC: an enabled chain whose runtime
    /// is still cold-starting has nothing to capture into yet.
    ///
    /// The counters are plain atomic reads off the audio thread — never the
    /// `processing` lock (#580).
    fn chain_runtime(&self, chain: &ChainId) -> Option<ChainRuntimeReading> {
        let borrow = self.runtime.borrow();
        let controller = borrow.as_ref()?;
        Some(ChainRuntimeReading {
            live: !controller.runtimes_for_chain(chain).is_empty(),
            xruns: controller.chain_xrun_count(chain),
            underruns: controller.chain_underrun_count(chain),
        })
    }

    fn chain_loopers(&self, chain: &ChainId) -> Option<Result<(Vec<LooperStatus>, u32), String>> {
        let borrow = self.runtime.borrow();
        let controller = borrow.as_ref()?;
        Some(Ok((
            controller.chain_looper_statuses(chain),
            controller.sample_rate(),
        )))
    }
}

/// Build the chain row's read seam over the app's shared runtime handle.
pub(crate) fn chain_row_live_source(
    runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>,
) -> Rc<dyn LiveSource> {
    Rc::new(ChainRowLiveSource {
        runtime: Rc::clone(runtime),
    })
}

/// #127: what a block editor's stream panel reads.
///
/// A utility block may publish a small table of reduced entries (`key` /
/// `value` / `text` / `peak`) from its worker thread — the tuner block's note
/// readout was the original one. That is a finished READING, so the editor
/// window, the inline drawer and the compact view all take it through the seam
/// instead of holding the audio backend to poll it.
pub(crate) struct BlockStreamLiveSource {
    runtime: Rc<RefCell<Option<ProjectRuntimeController>>>,
}

impl LiveSource for BlockStreamLiveSource {
    /// The engine collapses "no such block" and "nothing published" into
    /// `None`; both mean the same thing to a panel (show nothing), so they
    /// become the empty table here. What must NOT collapse is "no runtime" —
    /// that stays `None`, so a panel can tell a quiet block from a stopped rig.
    fn block_stream(&self, block: &BlockId) -> Option<Vec<block_core::StreamEntry>> {
        let borrow = self.runtime.borrow();
        let controller = borrow.as_ref()?;
        Some(controller.poll_stream(block).unwrap_or_default())
    }
}

/// Build the block editor's read seam over the app's shared runtime handle.
pub(crate) fn block_stream_live_source(
    runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>,
) -> Rc<dyn LiveSource> {
    Rc::new(BlockStreamLiveSource {
        runtime: Rc::clone(runtime),
    })
}

/// Build the poll tick's read seam over the app's shared runtime handle.
pub(crate) fn health_live_source(
    runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>,
) -> Rc<dyn LiveSource> {
    Rc::new(HealthLiveSource {
        runtime: Rc::clone(runtime),
    })
}

/// One chain's DI loop state: is its dedicated stream playing (#614/#717 —
/// the DI has its own stream, so "playing" is `di_stream_active`, not an
/// injection into the guitar runtime), and that playback's OWN peaks (#771 —
/// never the chain's).
///
/// The single source for both the chain row and the `openrig://` reading, so
/// the tile and a remote client cannot disagree.
fn di_reading(controller: &ProjectRuntimeController, chain: &ChainId) -> Option<DiLoopReading> {
    let playing = controller.di_stream_active(chain);
    let meter = crate::di_meter::di_meter_from_peaks(controller.di_playback_peaks(chain), playing);
    Some(DiLoopReading {
        chain: chain.0.clone(),
        playing,
        in_dbfs: meter.in_dbfs,
        out_dbfs: meter.out_dbfs,
        source: None,
    })
}

/// Where the click is in the bar, from the generator's own lock-free cell.
///
/// `None` ⇒ no runtime is hosted (the rig is stopped), never a fabricated
/// beat. `running` is the flag the audio callback itself reads, so a control
/// mirror that disagrees is the one that is wrong.
fn metronome_reading(
    runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>,
) -> Option<MetronomeReading> {
    let borrow = runtime.borrow();
    let shared = borrow.as_ref()?.metronome_shared();
    let position = shared.position();
    Some(MetronomeReading {
        running: shared.enabled(),
        bar: position.bar,
        beat: position.beat,
        tick: position.tick,
        counting_in: position.counting_in,
    })
}

#[cfg(test)]
#[path = "gui_live_source_tests.rs"]
mod tests;
