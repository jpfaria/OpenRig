//! Responsibility: describes the user-adjustable drum machine settings.

pub const MIN_BPM: f32 = 20.0;
pub const MAX_BPM: f32 = 400.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DrumSettings {
    pub bpm: f32,
    /// Linear output gain.
    pub volume: f32,
}

impl DrumSettings {
    pub(crate) fn clamped(self) -> Self {
        let bpm = if self.bpm.is_finite() {
            self.bpm.clamp(MIN_BPM, MAX_BPM)
        } else {
            120.0
        };
        let volume = if self.volume.is_finite() {
            self.volume.max(0.0)
        } else {
            0.0
        };
        Self { bpm, volume }
    }
}
