//! #127: the read-side counterpart to `dyn CommandDispatcher`.
//!
//! Some state only ever lives inside a frontend's own audio runtime —
//! meters, the tuner, the spectrum, the DI loop, loopers, the device list,
//! the sample rate. The core has no audio thread of its own to read these
//! from, so a frontend that hosts one implements `LiveSource` and hands
//! the core a reference to it; Task 4's `QueryKind` resolver reads through
//! this trait instead of reaching into a concrete GUI type.
//!
//! **Readings only — PCM never crosses this boundary.** Every method
//! returns an already-reduced value (dBFS, note/cents, band levels, a
//! looper's position/state) computed on the frontend's own audio thread.
//! No raw sample buffer, tap, or stream handle is ever passed through a
//! `LiveSource` method.
//!
//! Every method defaults to `None`, meaning "this frontend hosts no such
//! source" — a frontend implements only the methods for the sources it
//! actually owns. `None` is never "hosted, but silent"; the caller (Task 4)
//! supplies the documented empty shape (e.g. an empty `Vec`, `running:
//! false`) for a hosted-but-idle source. `application` must not depend on
//! `infra_cpal`: devices are supplied BY the frontend, never enumerated by
//! the core.

use domain::ids::ChainId;
use engine::LooperStatus;

use crate::query_analyzers::{SpectrumReading, TunerReading};
use crate::query_di::DiLoopReading;

/// Per-chain meter reading: input/output peak in dBFS.
pub struct ChainMeterReading {
    pub chain: ChainId,
    pub in_dbfs: f32,
    pub out_dbfs: f32,
}

/// Live, frontend-hosted readings the core cannot produce on its own.
///
/// A frontend implements only the methods for the sources it hosts; every
/// other method keeps the default `None`. See the module docs for the
/// PCM-never-crosses-this-boundary rule and the `None` vs. hosted-but-idle
/// distinction.
pub trait LiveSource {
    fn chain_meters(&self) -> Option<Vec<ChainMeterReading>> {
        None
    }

    fn tuner(&self) -> Option<Vec<TunerReading>> {
        None
    }

    fn spectrum(&self) -> Option<Vec<SpectrumReading>> {
        None
    }

    fn di_loop(&self) -> Option<Vec<DiLoopReading>> {
        None
    }

    fn chain_loopers(&self, chain: &ChainId) -> Option<(Vec<LooperStatus>, u32)> {
        let _ = chain;
        None
    }

    /// The audio device listing, from the frontend that owns a host.
    ///
    /// Three states, deliberately: `None` ⇒ this frontend hosts no device
    /// source (the caller answers the documented empty listing);
    /// `Some(Ok(names))` ⇒ enumerated; `Some(Err(msg))` ⇒ hosted, but
    /// enumeration FAILED — a dead host or a JACK server that is down. That
    /// last case must reach the caller as an error: "the audio host is
    /// broken" is not the same answer as "this transport has no devices to
    /// report", and collapsing the two hides a real failure.
    fn devices(&self) -> Option<Result<Vec<String>, String>> {
        None
    }

    fn sample_rate(&self) -> Option<u32> {
        None
    }
}

/// The empty `LiveSource`: a frontend that hosts no live reads at all.
pub struct NoLiveSource;

impl LiveSource for NoLiveSource {}

#[cfg(test)]
#[path = "live_source_tests.rs"]
mod tests;
