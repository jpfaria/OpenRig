//! Regression guard (#794, audit #792 finding #7): constructing an IR
//! processor from a degenerate (empty) impulse response must return `Err`, not
//! panic. The convolver build runs off the audio thread, but a panic there
//! still crashes the block build instead of letting the builder bypass the
//! block. `FftBlockConvolver::new` already rejects an empty IR; these guard
//! that the public `MonoIrProcessor` / `StereoIrProcessor` constructors
//! propagate that error rather than `.expect()` it.

use ir::{MonoIrProcessor, StereoIrProcessor};

#[test]
fn mono_ir_processor_from_empty_is_err() {
    assert!(
        MonoIrProcessor::new(Vec::new()).is_err(),
        "an empty IR must yield Err, not panic"
    );
}

#[test]
fn stereo_ir_processor_from_empty_is_err() {
    assert!(
        StereoIrProcessor::new(Vec::new(), Vec::new()).is_err(),
        "an empty stereo IR must yield Err, not panic"
    );
    // A half-empty pair is still invalid.
    assert!(
        StereoIrProcessor::new(vec![0.1, 0.2], Vec::new()).is_err(),
        "an empty right channel must yield Err"
    );
}
