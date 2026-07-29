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

use application::live_source::{ChainMeterReading, LiveSource};
use application::query_analyzers::{SpectrumReading, TunerReading};
use application::query_di::DiLoopReading;
use domain::ids::ChainId;
use engine::LooperStatus;
use infra_cpal::ProjectRuntimeController;
use project::project::Project;
use slint::{Model, VecModel};

use crate::spectrum_session::SpectrumSession;
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
                .map(|chain| {
                    let playing = controller.di_stream_active(&chain.id);
                    let meter = crate::di_meter::di_meter_from_peaks(
                        controller.di_playback_peaks(&chain.id),
                        playing,
                    );
                    DiLoopReading {
                        chain: chain.id.0.clone(),
                        playing,
                        in_dbfs: meter.in_dbfs,
                        out_dbfs: meter.out_dbfs,
                        source: None,
                    }
                })
                .collect(),
        )
    }

    /// #323: the chain's live looper transport state, at the rate the
    /// streams are actually running at — the controller's own rate, read
    /// from the live stream, never a constant (issue #723).
    fn chain_loopers(&self, chain: &ChainId) -> Option<Result<(Vec<LooperStatus>, u32), String>> {
        let runtime = self.runtime.borrow();
        let controller = runtime.as_ref()?;
        Some(Ok((
            controller.chain_looper_statuses(chain),
            controller.sample_rate(),
        )))
    }

    /// The GUI owns an audio host, so it always answers — an enumeration
    /// that FAILED is a real failure (a dead host, a JACK server that is
    /// down) and propagates as one, not as an empty listing.
    fn devices(&self) -> Option<Result<Vec<String>, String>> {
        Some(infra_cpal::list_devices().map_err(|e| e.to_string()))
    }
}

#[cfg(test)]
#[path = "gui_live_source_tests.rs"]
mod tests;
