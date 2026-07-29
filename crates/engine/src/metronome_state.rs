//! Issue #14 — lock-free bridge between the control side (GUI, dispatcher,
//! MIDI) and the metronome's audio callback.
//!
//! Every field is an atomic: the callback never locks and never allocates
//! (invariant #8). Settings are versioned by [`MetronomeShared::generation`] so
//! the callback can skip re-reading them on the overwhelming majority of
//! buffers where nothing changed, and [`BeatPosition`] is packed into a single
//! atomic so the UI can never observe a half-updated bar/beat pair.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

pub use feature_dsp::metronome::{
    BeatPosition, MetronomeGenerator, MetronomeSettings, Subdivision, Timbre, BPM_MAX, BPM_MIN,
};

fn subdivision_code(s: Subdivision) -> u32 {
    match s {
        Subdivision::Off => 0,
        Subdivision::Eighths => 1,
        Subdivision::Triplets => 2,
        Subdivision::Sixteenths => 3,
    }
}

fn subdivision_from_code(code: u32) -> Subdivision {
    match code {
        1 => Subdivision::Eighths,
        2 => Subdivision::Triplets,
        3 => Subdivision::Sixteenths,
        _ => Subdivision::Off,
    }
}

fn timbre_code(t: Timbre) -> u32 {
    match t {
        Timbre::Click => 0,
        Timbre::Wood => 1,
        Timbre::Beep => 2,
    }
}

fn timbre_from_code(code: u32) -> Timbre {
    match code {
        1 => Timbre::Wood,
        2 => Timbre::Beep,
        _ => Timbre::Click,
    }
}

/// Pack a position into one `u64` so the UI reads it in a single load.
///
/// `bar` is truncated to 16 bits: it exists to tell bars apart on screen, and a
/// practice session that reaches 65 535 bars simply wraps — harmless, where a
/// torn read would not be.
fn pack_position(pos: BeatPosition) -> u64 {
    ((pos.bar as u64 & 0xFFFF) << 48)
        | ((pos.beat as u64 & 0xFFFF) << 32)
        | ((pos.tick as u64 & 0xFFFF) << 16)
        | (pos.counting_in as u64)
}

fn unpack_position(bits: u64) -> BeatPosition {
    BeatPosition {
        bar: ((bits >> 48) & 0xFFFF) as u32,
        beat: ((bits >> 32) & 0xFFFF) as u32,
        tick: ((bits >> 16) & 0xFFFF) as u32,
        counting_in: (bits & 1) != 0,
    }
}

/// Shared, lock-free metronome state.
pub struct MetronomeShared {
    enabled: AtomicBool,
    /// BPM scaled by 1000 — an integer keeps the store atomic without a lock
    /// while still resolving finer than any tempo control the UI offers.
    bpm_milli: AtomicU32,
    beats_per_bar: AtomicU32,
    subdivision: AtomicU32,
    timbre: AtomicU32,
    volume_bits: AtomicU32,
    count_in: AtomicBool,
    generation: AtomicU64,
    restart: AtomicBool,
    position: AtomicU64,
}

/// Handle shared between the control side and the audio callback.
pub type MetronomeCell = Arc<MetronomeShared>;

impl MetronomeShared {
    pub fn new(settings: MetronomeSettings) -> Self {
        let shared = Self {
            enabled: AtomicBool::new(false),
            bpm_milli: AtomicU32::new(0),
            beats_per_bar: AtomicU32::new(0),
            subdivision: AtomicU32::new(0),
            timbre: AtomicU32::new(0),
            volume_bits: AtomicU32::new(0),
            count_in: AtomicBool::new(false),
            generation: AtomicU64::new(0),
            restart: AtomicBool::new(false),
            position: AtomicU64::new(0),
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

    pub fn set_settings(&self, settings: MetronomeSettings) {
        self.bpm_milli
            .store((settings.bpm * 1000.0) as u32, Ordering::Relaxed);
        self.beats_per_bar
            .store(settings.beats_per_bar, Ordering::Relaxed);
        self.subdivision
            .store(subdivision_code(settings.subdivision), Ordering::Relaxed);
        self.timbre
            .store(timbre_code(settings.timbre), Ordering::Relaxed);
        self.volume_bits
            .store(settings.volume.to_bits(), Ordering::Relaxed);
        self.count_in.store(settings.count_in, Ordering::Relaxed);
        // Released last: a callback that sees the new generation is guaranteed
        // to see every field above it.
        self.generation.fetch_add(1, Ordering::Release);
    }

    pub fn settings(&self) -> MetronomeSettings {
        MetronomeSettings {
            bpm: self.bpm_milli.load(Ordering::Relaxed) as f32 / 1000.0,
            beats_per_bar: self.beats_per_bar.load(Ordering::Relaxed),
            subdivision: subdivision_from_code(self.subdivision.load(Ordering::Relaxed)),
            timbre: timbre_from_code(self.timbre.load(Ordering::Relaxed)),
            volume: f32::from_bits(self.volume_bits.load(Ordering::Relaxed)),
            count_in: self.count_in.load(Ordering::Relaxed),
        }
    }

    /// Bumped by every settings change. The callback compares it against its
    /// own copy instead of re-reading each field on every buffer.
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }

    pub fn request_restart(&self) {
        self.restart.store(true, Ordering::Release);
    }

    /// Consume a pending restart request. One-shot: a request is delivered to
    /// exactly one buffer, so a restart can never be applied twice.
    pub fn take_restart(&self) -> bool {
        self.restart.swap(false, Ordering::AcqRel)
    }

    pub fn publish_position(&self, pos: BeatPosition) {
        self.position.store(pack_position(pos), Ordering::Relaxed);
    }

    pub fn position(&self) -> BeatPosition {
        unpack_position(self.position.load(Ordering::Relaxed))
    }
}

#[cfg(test)]
#[path = "metronome_state_tests.rs"]
mod tests;
