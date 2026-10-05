//! Responsibility: carries the backing-track player's state across threads without locks.
//!
//! Every field is an atomic, so the output callback never locks or allocates.
//! Settings are versioned by a generation counter that the worker polls; seeks
//! and ring flushes are versioned by their own epochs so a request is never
//! lost between two polls.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

use super::settings::PlayerSettings;

/// Shared, lock-free player state.
pub struct PlayerShared {
    playing: AtomicBool,
    volume_bits: AtomicU32,
    speed_bits: AtomicU32,
    semitones_bits: AtomicU32,
    /// Loop bounds in seconds; NaN in either means no loop.
    loop_start_bits: AtomicU64,
    loop_end_bits: AtomicU64,
    generation: AtomicU64,
    seek_bits: AtomicU64,
    seek_epoch: AtomicU64,
    flush_request: AtomicU64,
    flush_ack: AtomicU64,
    /// Frames the output callback has taken from the ring, heard or drained.
    consumed: AtomicU64,
    position_bits: AtomicU64,
    duration_bits: AtomicU64,
    loading: AtomicBool,
    failed: AtomicBool,
}

/// Shared handle the controller, the worker and the callback all hold.
pub type PlayerCell = Arc<PlayerShared>;

impl Default for PlayerShared {
    fn default() -> Self {
        Self::new(PlayerSettings::default())
    }
}

impl PlayerShared {
    pub fn new(settings: PlayerSettings) -> Self {
        let shared = Self {
            playing: AtomicBool::new(false),
            volume_bits: AtomicU32::new(0),
            speed_bits: AtomicU32::new(0),
            semitones_bits: AtomicU32::new(0),
            loop_start_bits: AtomicU64::new(f64::NAN.to_bits()),
            loop_end_bits: AtomicU64::new(f64::NAN.to_bits()),
            generation: AtomicU64::new(0),
            seek_bits: AtomicU64::new(0f64.to_bits()),
            seek_epoch: AtomicU64::new(0),
            flush_request: AtomicU64::new(0),
            flush_ack: AtomicU64::new(0),
            consumed: AtomicU64::new(0),
            position_bits: AtomicU64::new(0f64.to_bits()),
            duration_bits: AtomicU64::new(0f64.to_bits()),
            loading: AtomicBool::new(false),
            failed: AtomicBool::new(false),
        };
        shared.set_settings(settings);
        shared
    }

    pub fn settings(&self) -> PlayerSettings {
        let start = f64::from_bits(self.loop_start_bits.load(Ordering::Acquire));
        let end = f64::from_bits(self.loop_end_bits.load(Ordering::Acquire));
        PlayerSettings {
            volume: self.volume(),
            speed: f32::from_bits(self.speed_bits.load(Ordering::Acquire)),
            semitones: f32::from_bits(self.semitones_bits.load(Ordering::Acquire)),
            loop_range: (!start.is_nan() && !end.is_nan()).then_some((start, end)),
        }
    }

    /// Publishes new settings, clamped, and bumps the generation.
    pub fn set_settings(&self, settings: PlayerSettings) {
        let settings = settings.clamped();
        let (start, end) = settings.loop_range.unwrap_or((f64::NAN, f64::NAN));
        self.volume_bits
            .store(settings.volume.to_bits(), Ordering::Release);
        self.speed_bits
            .store(settings.speed.to_bits(), Ordering::Release);
        self.semitones_bits
            .store(settings.semitones.to_bits(), Ordering::Release);
        self.loop_start_bits
            .store(start.to_bits(), Ordering::Release);
        self.loop_end_bits.store(end.to_bits(), Ordering::Release);
        self.generation.fetch_add(1, Ordering::AcqRel);
    }

    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }

    pub fn volume(&self) -> f32 {
        f32::from_bits(self.volume_bits.load(Ordering::Acquire))
    }

    pub fn is_playing(&self) -> bool {
        self.playing.load(Ordering::Acquire)
    }

    pub fn set_playing(&self, playing: bool) {
        self.playing.store(playing, Ordering::Release);
    }

    /// Asks the worker to jump to `seconds` of the track.
    pub fn request_seek(&self, seconds: f64) {
        self.seek_bits.store(seconds.to_bits(), Ordering::Release);
        self.seek_epoch.fetch_add(1, Ordering::AcqRel);
    }

    /// The latest seek request as `(epoch, seconds)`.
    pub fn seek_request(&self) -> (u64, f64) {
        let epoch = self.seek_epoch.load(Ordering::Acquire);
        (
            epoch,
            f64::from_bits(self.seek_bits.load(Ordering::Acquire)),
        )
    }

    /// Worker side: asks the callback to fade out and empty the ring. Returns
    /// the epoch to wait for with [`Self::flush_done`].
    pub fn request_flush(&self) -> u64 {
        self.flush_request.fetch_add(1, Ordering::AcqRel) + 1
    }

    /// Callback side: the flush epoch still waiting, if any.
    pub fn flush_pending(&self) -> Option<u64> {
        let request = self.flush_request.load(Ordering::Acquire);
        (request != self.flush_ack.load(Ordering::Acquire)).then_some(request)
    }

    /// Callback side: the ring is empty for `epoch`.
    pub fn ack_flush(&self, epoch: u64) {
        self.flush_ack.store(epoch, Ordering::Release);
    }

    pub fn flush_done(&self, epoch: u64) -> bool {
        self.flush_ack.load(Ordering::Acquire) >= epoch
    }

    pub fn add_consumed(&self, frames: u64) {
        self.consumed.fetch_add(frames, Ordering::AcqRel);
    }

    pub fn consumed(&self) -> u64 {
        self.consumed.load(Ordering::Acquire)
    }

    /// Seconds of the track the listener is hearing now.
    pub fn position_seconds(&self) -> f64 {
        f64::from_bits(self.position_bits.load(Ordering::Acquire))
    }

    pub fn set_position_seconds(&self, seconds: f64) {
        self.position_bits
            .store(seconds.to_bits(), Ordering::Release);
    }

    pub fn duration_seconds(&self) -> f64 {
        f64::from_bits(self.duration_bits.load(Ordering::Acquire))
    }

    pub fn set_duration_seconds(&self, seconds: f64) {
        self.duration_bits
            .store(seconds.to_bits(), Ordering::Release);
    }

    pub fn is_loading(&self) -> bool {
        self.loading.load(Ordering::Acquire)
    }

    pub fn set_loading(&self, loading: bool) {
        self.loading.store(loading, Ordering::Release);
    }

    pub fn has_failed(&self) -> bool {
        self.failed.load(Ordering::Acquire)
    }

    pub fn set_failed(&self, failed: bool) {
        self.failed.store(failed, Ordering::Release);
    }
}

#[cfg(test)]
#[path = "shared_tests.rs"]
mod tests;
