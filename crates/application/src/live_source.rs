//! #127: the read-side counterpart to `dyn CommandDispatcher`.
//!
//! Some state only ever lives inside a frontend's own audio runtime —
//! meters, the tuner, the spectrum, the DI loop, loopers (and the rate they
//! count at), the device list. The core has no audio thread of its own to
//! read these from, so a frontend that hosts one implements `LiveSource` and hands
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

use domain::ids::{BlockId, ChainId};
use engine::LooperStatus;

use crate::query_analyzers::{SpectrumReading, TunerReading};
use crate::query_di::DiLoopReading;

/// #14/#127: where the click is in the bar, and whether it is sounding.
///
/// The generator publishes a POSITION (a phase), not a queue of beat events,
/// so a slow frame can never lose or double a beat — every consumer, on-screen
/// lamp or MCP client, reads the beat the click is actually on.
pub struct MetronomeReading {
    /// Whether the click's stream is open and enabled RIGHT NOW. Reported by
    /// the audio side, so it is the truth even if a control-plane mirror
    /// somewhere disagrees.
    pub running: bool,
    pub bar: u32,
    /// Zero-based beat within the bar.
    pub beat: u32,
    /// Zero-based subdivision tick within the beat.
    pub tick: u32,
    /// Still counting the bar off; no downbeat has been played yet.
    pub counting_in: bool,
}

/// One failure the audio thread reported, already attributed to the stream
/// that raised it.
///
/// The engine posts these into a bounded, lock-free queue and drops them when
/// it is full, so this is a diagnostic sample of an error storm and never a
/// complete log — by design, since the alternative is work on the audio thread.
pub struct BlockErrorReading {
    /// The chain whose runtime raised it. Errors are drained per runtime and
    /// tagged with the chain that owns it — never pooled anonymously, so a
    /// reader can always tell which stream is failing (`CLAUDE.md` LAW).
    pub chain: ChainId,
    pub block: BlockId,
    pub message: String,
}

/// Whether this frontend's audio is sounding, and whether its backend still
/// answers.
pub struct AudioHealthReading {
    /// Anything at all is live: a running chain, a pending cold activation, or
    /// an armed DI holding its own stream open (#808).
    pub running: bool,
    /// The backend the open streams depend on is still there. False means the
    /// JACK server disappeared (a USB interface was unplugged and udev
    /// restarted jackd) — on CoreAudio/WASAPI device loss surfaces through the
    /// stream error callbacks instead, so this stays true.
    pub healthy: bool,
}

/// What the engine says about ONE chain's runtime right now — read on the
/// frontend's meter tick, in one borrow, for that chain's row.
///
/// Deliberately per-chain (`CLAUDE.md` LAW): there is no pooled "the rig's
/// xruns". A row shows its OWN stream's health or it shows nothing.
pub struct ChainRuntimeReading {
    /// This chain has at least one live per-input runtime — what gates REC on
    /// a looper (an enabled chain still cold-starting has none yet).
    pub live: bool,
    /// Audio-thread deadline overruns counted on THIS chain since it started.
    /// A cumulative count, not a delta: the reader keeps the previous value
    /// and a DECREASE means the counter was reset by a rebuild, not a fresh
    /// overrun (#670).
    pub xruns: u64,
    /// Output-side elastic-buffer underruns on THIS chain, same contract —
    /// the other way the user hears crackle when the callback itself was fast.
    pub underruns: u64,
}

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

    /// #14: the click's live beat position. `None` ⇒ this frontend hosts no
    /// metronome runtime (no project started); the caller answers the
    /// documented silent shape rather than inventing a beat.
    fn metronome(&self) -> Option<MetronomeReading> {
        None
    }

    /// The sample rate THIS chain's streams run at — or would be opened at,
    /// with the rig stopped.
    ///
    /// `None` ⇒ this frontend cannot tell (it hosts no audio, or the chain's
    /// devices could not be resolved), and the caller falls back to the
    /// dispatcher's tracked engine rate. It must never be answered with a
    /// guess (issue #723).
    ///
    /// It exists because the dispatcher's `engine_sr` is a mirror of a RUNNING
    /// stream: since #127 it goes back to `REFERENCE_SAMPLE_RATE` when the rig
    /// stops, which is honest for "nothing is running" and wrong as an answer
    /// to "what would this chain be measured at". The latency probe asked the
    /// mirror and reported DSP latency computed at 48 kHz for a stopped rig on
    /// a 44.1 kHz interface. The frontend that owns the devices can resolve the
    /// real one, so it is asked first.
    ///
    /// **Isolation:** the rate of the chain NAMED, resolved from its own
    /// endpoints. Never "the rate the rig is at" (`CLAUDE.md` LAW).
    fn chain_sample_rate(&self, chain: &ChainId) -> Option<f32> {
        let _ = chain;
        None
    }

    /// Per-chain live looper transport state + the rate it was counted at.
    ///
    /// Three states, mirroring [`Self::devices`]: `None` ⇒ this frontend
    /// hosts no looper runtime for any chain (the caller falls back to the
    /// chain's persisted shape with silent statuses, at the dispatcher's
    /// tracked engine rate); `Some(Err(msg))` ⇒ hosted, but THIS chain's
    /// rate could not be resolved (its device/endpoint could not be found,
    /// for example) — that must propagate as a real failure, never a
    /// fabricated rate (issue #723); `Some(Ok((statuses, rate)))` ⇒ hosted
    /// and resolved.
    fn chain_loopers(&self, chain: &ChainId) -> Option<Result<(Vec<LooperStatus>, u32), String>> {
        let _ = chain;
        None
    }

    /// #127: the block failures the audio thread reported since the last
    /// call, each tagged with the chain that raised it.
    ///
    /// **A DRAINING read.** The engine's queue is emptied by this call, so a
    /// reading is delivered exactly once and there can only ever be ONE
    /// consumer — whoever drains takes the errors away from everyone else.
    /// That is why this is not (and must not become) a `QueryKind`: a second
    /// transport polling it would silently steal the frontend's toasts.
    ///
    /// `None` ⇒ no runtime is hosted; `Some(vec![])` ⇒ hosted and nothing
    /// failed.
    fn block_errors(&self) -> Option<Vec<BlockErrorReading>> {
        None
    }

    /// #127: the DI loop's live state for ONE chain — the same reading
    /// [`Self::di_loop`] reports for every chain, addressed by identity so the
    /// chain row can redraw itself without asking about the whole project.
    ///
    /// `source` is owned by the dispatcher and filled in by the resolver;
    /// whatever a frontend sets here is discarded.
    fn chain_di_loop(&self, chain: &ChainId) -> Option<DiLoopReading> {
        let _ = chain;
        None
    }

    /// #127: what the engine says about ONE chain's runtime right now.
    ///
    /// `None` ⇒ this frontend hosts no audio runtime at all. A chain that has
    /// no runtime OF ITS OWN on a hosted frontend is `Some` with
    /// `live: false` — "nothing is playing here" is an answer, not an absence.
    fn chain_runtime(&self, chain: &ChainId) -> Option<ChainRuntimeReading> {
        let _ = chain;
        None
    }

    /// #127: the diagnostic stream one block publishes for its editor panel —
    /// already-reduced `key` / `value` / `text` / `peak` entries a worker
    /// thread stores wait-free, never audio.
    ///
    /// `None` ⇒ this frontend hosts no runtime; `Some(vec![])` ⇒ hosted, and
    /// this block publishes nothing (it has no stream, or its stream is
    /// silent). The panel needs the distinction: the first keeps whatever it
    /// was showing, the second turns the display off.
    fn block_stream(&self, block: &BlockId) -> Option<Vec<block_core::StreamEntry>> {
        let _ = block;
        None
    }

    /// #127: is anything sounding, and is the backend still there.
    ///
    /// `None` ⇒ this frontend hosts no audio runtime, which is not the same
    /// as an unhealthy one: there is nothing to reconnect to.
    fn audio_health(&self) -> Option<AudioHealthReading> {
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
}

/// The empty `LiveSource`: a frontend that hosts no live reads at all.
pub struct NoLiveSource;

impl LiveSource for NoLiveSource {}

#[cfg(test)]
#[path = "live_source_tests.rs"]
mod tests;
