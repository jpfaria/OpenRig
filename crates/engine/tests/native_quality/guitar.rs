//! Responsibility: loads the bundled guitar DI the battery plays as real programme.

use std::sync::OnceLock;

/// A clean Strato DI: what a player actually sends, quieter and more
/// mid-heavy than the pink programme.
const DI_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../assets/di-loops/fabiano-antunes-STRATO-clean.wav"
);
/// Seconds of the DI the battery plays.
const DI_SECS: usize = 20;

/// The DI samples and their sample rate, read once.
pub fn guitar_di() -> &'static (Vec<f32>, f32) {
    static DI: OnceLock<(Vec<f32>, f32)> = OnceLock::new();
    DI.get_or_init(|| {
        let mut reader = hound::WavReader::open(DI_PATH)
            .unwrap_or_else(|e| panic!("{DI_PATH}: {e} (is git lfs pulled?)"));
        let spec = reader.spec();
        let scale = (1u64 << (spec.bits_per_sample - 1)) as f32;
        let take = spec.sample_rate as usize * DI_SECS * spec.channels as usize;
        let samples: Vec<f32> = reader
            .samples::<i32>()
            .take(take)
            .step_by(spec.channels as usize)
            .map(|s| s.expect("DI sample") as f32 / scale)
            .collect();
        (samples, spec.sample_rate as f32)
    })
}
