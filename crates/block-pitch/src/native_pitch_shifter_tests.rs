use super::*;
use block_core::StereoProcessor;
use realfft::RealFftPlanner;
use std::f32::consts::TAU;

const SAMPLE_RATE: f32 = 48_000.0;

fn peak_bin(samples: &[f32]) -> usize {
    let mut planner = RealFftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(samples.len());
    let mut input = samples.to_vec();
    let mut output = fft.make_output_vec();
    fft.process(&mut input, &mut output).expect("fft");
    output
        .iter()
        .enumerate()
        .skip(1)
        .max_by(|(_, a), (_, b)| a.norm().total_cmp(&b.norm()))
        .map(|(idx, _)| idx)
        .unwrap_or(0)
}

#[test]
fn schema_lists_expected_parameters() {
    let schema = model_schema();
    assert_eq!(schema.model, MODEL_ID);
    assert_eq!(schema.effect_type, block_core::EFFECT_TYPE_PITCH);
    let keys: Vec<&str> = schema.parameters.iter().map(|p| p.path.as_str()).collect();
    assert!(keys.contains(&"shift_semitones"));
    assert!(keys.contains(&"shift_cents"));
    assert!(keys.contains(&"mix"));
}

#[test]
fn semitone_factor_octave_up_is_two() {
    let factor = semitones_to_pitch_factor(12.0, 0.0);
    assert!((factor - 2.0).abs() < 1e-5, "expected 2.0, got {factor}");
}

#[test]
fn semitone_factor_octave_down_is_half() {
    let factor = semitones_to_pitch_factor(-12.0, 0.0);
    assert!((factor - 0.5).abs() < 1e-5, "expected 0.5, got {factor}");
}

#[test]
fn semitone_factor_zero_is_unity() {
    let factor = semitones_to_pitch_factor(0.0, 0.0);
    assert!((factor - 1.0).abs() < 1e-6, "expected 1.0, got {factor}");
}

#[test]
fn process_frame_octave_up_shifts_peak() {
    let mut shifter = PitchShifter::new(Params {
        shift_semitones: 12.0,
        shift_cents: 0.0,
        mix: 1.0,
    });
    let freq = 440.0_f32;
    let warmup = 4096;
    let capture = 4096;
    let mut output = Vec::with_capacity(capture);
    for n in 0..(warmup + capture) {
        let s = (TAU * freq * n as f32 / SAMPLE_RATE).sin();
        let y = shifter.process_frame([s, s]);
        if n >= warmup {
            output.push(y[0]);
        }
    }
    let bin = peak_bin(&output);
    let expected = (2.0 * freq * capture as f32 / SAMPLE_RATE).round() as usize;
    assert!(
        bin.abs_diff(expected) <= 3,
        "peak bin {bin} not within 3 of expected {expected}"
    );
}

#[test]
fn process_frame_dry_mix_passes_input_through() {
    let mut shifter = PitchShifter::new(Params {
        shift_semitones: 12.0,
        shift_cents: 0.0,
        mix: 0.0,
    });
    let mut max_diff = 0.0_f32;
    for n in 0..2048 {
        let s = (TAU * 440.0 * n as f32 / SAMPLE_RATE).sin();
        let y = shifter.process_frame([s, s]);
        max_diff = max_diff.max((y[0] - s).abs()).max((y[1] - s).abs());
    }
    assert!(
        max_diff < 1e-5,
        "dry mix should match input, max diff {max_diff}"
    );
}

#[test]
fn process_frame_silence_is_finite() {
    let mut shifter = PitchShifter::new(Params::default());
    for _ in 0..8192 {
        let y = shifter.process_frame([0.0, 0.0]);
        assert!(y[0].is_finite() && y[1].is_finite());
    }
}

#[test]
fn process_frame_extreme_inputs_are_finite() {
    let mut shifter = PitchShifter::new(Params {
        shift_semitones: 7.0,
        shift_cents: 25.0,
        mix: 1.0,
    });
    for n in 0..8192 {
        let s = if n % 2 == 0 { 1.0 } else { -1.0 };
        let y = shifter.process_frame([s, -s]);
        assert!(
            y[0].is_finite() && y[1].is_finite(),
            "non-finite at n={n}: [{}, {}]",
            y[0],
            y[1]
        );
    }
}
