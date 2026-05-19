//! Tests for the post-process output-gain wrapper. Issue #491 — the
//! manifest calibration is in dB, so the wrapper applies a dB offset
//! (linear `10^(db/20)`), not a percentage ratio.

use super::*;

struct UnitMono;
impl MonoProcessor for UnitMono {
    fn process_sample(&mut self, input: f32) -> f32 {
        input
    }
}

fn run_mono(p: &mut BlockProcessor, x: f32) -> f32 {
    match p {
        BlockProcessor::Mono(m) => m.process_sample(x),
        BlockProcessor::Stereo(_) => panic!("expected mono"),
    }
}

#[test]
fn none_is_passthrough() {
    let mut p = wrap_with_output_gain_db(BlockProcessor::Mono(Box::new(UnitMono)), None);
    assert_eq!(run_mono(&mut p, 0.5), 0.5);
}

#[test]
fn zero_db_is_passthrough() {
    let mut p = wrap_with_output_gain_db(BlockProcessor::Mono(Box::new(UnitMono)), Some(0.0));
    assert_eq!(run_mono(&mut p, 0.5), 0.5);
}

#[test]
fn plus_six_db_doubles_amplitude() {
    // +6.0206 dB == linear ×2.0 (10^(6.0206/20)).
    let mut p = wrap_with_output_gain_db(BlockProcessor::Mono(Box::new(UnitMono)), Some(6.0206));
    assert!((run_mono(&mut p, 0.5) - 1.0).abs() < 1e-4);
}

#[test]
fn minus_six_db_halves_amplitude() {
    let mut p = wrap_with_output_gain_db(BlockProcessor::Mono(Box::new(UnitMono)), Some(-6.0206));
    assert!((run_mono(&mut p, 1.0) - 0.5).abs() < 1e-4);
}
