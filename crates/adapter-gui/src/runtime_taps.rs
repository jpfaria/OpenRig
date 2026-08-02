//! #127: the GUI's implementation of the subscription seam
//! (`application::audio_taps`) — the ONE place that turns a `TapPoint` into a
//! `ProjectRuntimeController` subscription.
//!
//! An owner module, not a wiring module: nothing in here is reachable except
//! through `AudioTaps`. The meters, the tuner, the spectrum and the Tone Doctor
//! now take `&dyn AudioTaps` and never name the audio backend.
//!
//! The tap handed back wraps **the exact `Arc<SpscRing<f32>>` handles the
//! consumers used to hold directly** — same `subscribe_*_tap` call, same
//! capacity, same producer. The audio thread pushes into precisely the taps it
//! pushed into before this module existed, so "no extra work on the audio
//! thread" is true by construction and not by measurement.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use application::audio_taps::{AudioTap, AudioTaps, TapPoint};
use domain::ids::ChainId;
use engine::output_meter::pop_peak_dbfs;
use engine::spsc::SpscRing;
use infra_cpal::ProjectRuntimeController;

/// The same-process tap: a handful of SPSC rings the audio callback pushes
/// into, one per channel.
pub(crate) struct RingTap {
    rings: Vec<Arc<SpscRing<f32>>>,
}

impl AudioTap for RingTap {
    fn channels(&self) -> usize {
        self.rings.len()
    }

    /// The same reduction the meter did itself before the seam existed:
    /// `pop_peak_dbfs` over this tap's own rings, `SILENT_DBFS` when nothing
    /// arrived. Consumes the window.
    fn poll_peak_dbfs(&self) -> f32 {
        pop_peak_dbfs(&self.rings)
    }

    fn drain_channel(&self, channel: usize, max: usize, out: &mut Vec<f32>) -> usize {
        let Some(ring) = self.rings.get(channel) else {
            return 0;
        };
        let mut taken = 0;
        while taken < max {
            match ring.pop() {
                Some(s) => {
                    out.push(s);
                    taken += 1;
                }
                None => break,
            }
        }
        taken
    }
}

/// The GUI's subscription authority over the app's shared runtime handle.
///
/// Frontend-local by design (it holds an `Rc<RefCell<..>>`): only the thread
/// that owns the audio host may OPEN a subscription. The `Arc<dyn AudioTap>` it
/// hands back is `Send + Sync`, so a consumer may carry it to a worker.
pub(crate) struct GuiAudioTaps {
    runtime: Rc<RefCell<Option<ProjectRuntimeController>>>,
}

impl AudioTaps for GuiAudioTaps {
    fn is_hosted(&self) -> bool {
        self.runtime.borrow().is_some()
    }

    fn live_sample_rate(&self) -> u32 {
        self.runtime
            .borrow()
            .as_ref()
            .map_or(0, |rt| rt.sample_rate())
    }

    fn stream_count(&self, chain: &ChainId) -> usize {
        self.runtime
            .borrow()
            .as_ref()
            .map_or(0, |rt| rt.stream_count(chain))
    }

    /// Resolve the tap point against the runtime that owns THAT stream.
    ///
    /// The controller's dispatch does the (global stream index → per-input
    /// runtime, local segment, device channel) walk (#350/#557); this method
    /// only chooses which of its three entry points the point names, so no
    /// resolution rule is duplicated here.
    fn subscribe(
        &self,
        point: &TapPoint,
        capacity_per_channel: usize,
    ) -> Option<Arc<dyn AudioTap>> {
        let borrow = self.runtime.borrow();
        let controller = borrow.as_ref()?;
        let rings: Vec<Arc<SpscRing<f32>>> = match point {
            TapPoint::StreamInput { chain, stream } => {
                vec![controller.subscribe_stream_input_tap(chain, *stream, capacity_per_channel)?]
            }
            TapPoint::StreamOutput { chain, stream } => controller
                .subscribe_stream_tap(chain, *stream, capacity_per_channel)?
                .to_vec(),
            TapPoint::InputChannels {
                chain,
                input,
                total_channels,
                channels,
            } => {
                let rings = controller.subscribe_input_tap(
                    chain,
                    *input,
                    *total_channels,
                    channels,
                    capacity_per_channel,
                );
                // An empty result means the chain has no runtime for that
                // input — a tap that would never fill, so report "no tap"
                // rather than an eternally silent handle.
                if rings.is_empty() {
                    return None;
                }
                rings
            }
        };
        Some(Arc::new(RingTap { rings }))
    }

    /// Both kinds at once. Reclaiming is refcount-based on the engine side, so
    /// a live consumer's tap is never taken away — and the two callers that
    /// used to prune only one kind were doing so for no reason other than
    /// which method they happened to know about.
    fn prune_dead_taps(&self) {
        if let Some(controller) = self.runtime.borrow().as_ref() {
            controller.prune_dead_input_taps();
            controller.prune_dead_stream_taps();
        }
    }
}

/// Build the tap seam over the app's shared runtime handle. Called by
/// `desktop_app`, the module that allocates it.
pub(crate) fn gui_audio_taps(
    runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>,
) -> Rc<dyn AudioTaps> {
    Rc::new(GuiAudioTaps {
        runtime: Rc::clone(runtime),
    })
}

#[cfg(test)]
#[path = "runtime_taps_tests.rs"]
mod tests;
