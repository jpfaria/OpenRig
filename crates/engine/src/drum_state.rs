//! Responsibility: bridges the drum machine control side to its audio callback.
//!
//! Scalars travel as atomics, versioned by a generation counter so the
//! callback only re-reads them when something changed. Kits and grooves are
//! heavy, so they travel as `Arc`s through an [`ArcHandoff`], which keeps the
//! callback from ever freeing one. The position is packed into one atomic so
//! the UI never reads a bar from one buffer and a beat from the next.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

pub use feature_dsp::drums::{
    DrumHit, DrumKit, DrumLayer, DrumMachine, DrumPattern, DrumPiece, DrumPosition, DrumRole,
    DrumSettings, Groove, MAX_BPM, MIN_BPM,
};

use crate::arc_handoff::ArcHandoff;

pub const DEFAULT_DRUM_SETTINGS: DrumSettings = DrumSettings {
    bpm: 120.0,
    volume: 0.8,
};

/// Beats in the bar of the groove played before any groove is chosen.
const SILENT_BEATS_PER_BAR: u32 = 4;

/// A groove with no hits: the machine keeps time and makes no sound.
pub fn silent_groove() -> Groove {
    Groove {
        id: String::new(),
        name: String::new(),
        genre: String::new(),
        beats_per_bar: SILENT_BEATS_PER_BAR,
        tempo: DEFAULT_DRUM_SETTINGS.bpm,
        beat: DrumPattern::new(f64::from(SILENT_BEATS_PER_BAR), Vec::new()),
        fills: Vec::new(),
    }
}

/// `bar` keeps 16 bits: it tells bars apart on screen and simply wraps.
fn pack_position(pos: DrumPosition) -> u64 {
    ((u64::from(pos.bar) & 0xFFFF) << 32)
        | ((u64::from(pos.beat) & 0xFFFF) << 16)
        | (u64::from(pos.in_fill) << 1)
        | u64::from(pos.playing)
}

fn unpack_position(bits: u64) -> DrumPosition {
    DrumPosition {
        playing: bits & 1 != 0,
        in_fill: bits & 2 != 0,
        beat: ((bits >> 16) & 0xFFFF) as u32,
        bar: ((bits >> 32) & 0xFFFF) as u32,
    }
}

pub struct DrumsShared {
    enabled: AtomicBool,
    /// The transport the control side asked for; the callback follows it.
    playing: AtomicBool,
    fill_requests: AtomicU32,
    /// BPM scaled by 1000, so the store stays a plain atomic.
    bpm_milli: AtomicU32,
    volume_bits: AtomicU32,
    generation: AtomicU64,
    position: AtomicU64,
    /// The newest kit request; a load that finishes under an older ticket is
    /// dropped.
    kit_ticket: AtomicU64,
    kits: ArcHandoff<DrumKit>,
    grooves: ArcHandoff<Groove>,
}

/// Handle shared between the control side and the audio callback.
pub type DrumsCell = Arc<DrumsShared>;

impl Default for DrumsShared {
    fn default() -> Self {
        Self::new(DEFAULT_DRUM_SETTINGS)
    }
}

impl DrumsShared {
    pub fn new(settings: DrumSettings) -> Self {
        let shared = Self {
            enabled: AtomicBool::new(false),
            playing: AtomicBool::new(false),
            fill_requests: AtomicU32::new(0),
            bpm_milli: AtomicU32::new(0),
            volume_bits: AtomicU32::new(0),
            generation: AtomicU64::new(0),
            position: AtomicU64::new(0),
            kit_ticket: AtomicU64::new(0),
            kits: ArcHandoff::new(),
            grooves: ArcHandoff::new(),
        };
        shared.set_settings(settings);
        shared
    }

    pub fn set_enabled(&self, on: bool) {
        self.enabled.store(on, Ordering::Release);
    }

    pub fn enabled(&self) -> bool {
        self.enabled.load(Ordering::Acquire)
    }

    pub fn set_playing(&self, on: bool) {
        self.playing.store(on, Ordering::Release);
    }

    pub fn playing(&self) -> bool {
        self.playing.load(Ordering::Acquire)
    }

    pub fn request_fill(&self) {
        self.fill_requests.fetch_add(1, Ordering::Release);
    }

    /// Counts fill requests; the callback starts a fill when it moved.
    pub fn fill_requests(&self) -> u32 {
        self.fill_requests.load(Ordering::Acquire)
    }

    pub fn set_settings(&self, settings: DrumSettings) {
        self.bpm_milli
            .store((settings.bpm * 1000.0).round() as u32, Ordering::Relaxed);
        self.volume_bits
            .store(settings.volume.to_bits(), Ordering::Relaxed);
        // Released last: a reader that sees the new generation sees the fields.
        self.generation.fetch_add(1, Ordering::Release);
    }

    pub fn settings(&self) -> DrumSettings {
        DrumSettings {
            bpm: self.bpm_milli.load(Ordering::Relaxed) as f32 / 1000.0,
            volume: f32::from_bits(self.volume_bits.load(Ordering::Relaxed)),
        }
    }

    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }

    pub fn publish_position(&self, pos: DrumPosition) {
        self.position.store(pack_position(pos), Ordering::Relaxed);
    }

    pub fn position(&self) -> DrumPosition {
        unpack_position(self.position.load(Ordering::Relaxed))
    }

    pub fn kits(&self) -> &ArcHandoff<DrumKit> {
        &self.kits
    }

    pub fn grooves(&self) -> &ArcHandoff<Groove> {
        &self.grooves
    }

    /// Starts a kit request; only the newest ticket may deliver its kit.
    pub fn begin_kit_load(&self) -> u64 {
        self.kit_ticket.fetch_add(1, Ordering::AcqRel) + 1
    }

    /// Sends `kit` if no newer request started since `ticket`.
    pub fn finish_kit_load(&self, ticket: u64, kit: Arc<DrumKit>) -> bool {
        if self.kit_ticket.load(Ordering::Acquire) != ticket {
            return false;
        }
        self.kits.send(kit);
        true
    }
}

#[cfg(test)]
#[path = "drum_state_tests.rs"]
mod tests;
