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

/// #328: a processor that delays its output by a known amount.
struct Delayed(usize);

impl MonoProcessor for Delayed {
    fn process_sample(&mut self, input: f32) -> f32 {
        input
    }

    fn latency_samples(&self) -> usize {
        self.0
    }
}

impl StereoProcessor for Delayed {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        input
    }

    fn latency_samples(&self) -> usize {
        self.0
    }
}

/// #328: the manifest output-gain wrapper sits around IR and LV2 processors;
/// a chain split aligns its paths from what the OUTER processor reports, so
/// the wrapper must pass the inner latency through.
#[test]
fn output_gain_keeps_the_wrapped_processors_latency() {
    let mono = wrap_with_output_gain_db(BlockProcessor::Mono(Box::new(Delayed(64))), Some(-6.0));
    let stereo =
        wrap_with_output_gain_db(BlockProcessor::Stereo(Box::new(Delayed(64))), Some(-6.0));
    match (mono, stereo) {
        (BlockProcessor::Mono(mono), BlockProcessor::Stereo(stereo)) => {
            assert_eq!(mono.latency_samples(), 64, "mono gain wrapper");
            assert_eq!(stereo.latency_samples(), 64, "stereo gain wrapper");
        }
        _ => panic!("the gain wrapper keeps the processor layout"),
    }
}
