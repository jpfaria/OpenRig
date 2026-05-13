use super::{PhaseVocoder, OVERLAP_FACTOR};
use realfft::RealFftPlanner;
use std::f32::consts::TAU;

const SAMPLE_RATE: f32 = 48_000.0;
const WINDOW: usize = 2048;
const WARMUP_LATENCY: usize = WINDOW - WINDOW / OVERLAP_FACTOR;

fn sine(freq: f32, len: usize) -> Vec<f32> {
    (0..len)
        .map(|n| (TAU * freq * n as f32 / SAMPLE_RATE).sin())
        .collect()
}

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
fn new_does_not_panic_for_supported_window_sizes() {
    for size in [512, 1024, 2048, 4096] {
        let _ = PhaseVocoder::new(size);
    }
}

#[test]
fn silence_input_produces_silence_output() {
    let mut pv = PhaseVocoder::new(WINDOW);
    pv.set_pitch_factor(2.0);
    let total = WINDOW * 4;
    let mut max = 0.0_f32;
    for _ in 0..total {
        let y = pv.process_sample(0.0);
        assert!(y.is_finite(), "non-finite output from silence");
        max = max.max(y.abs());
    }
    assert!(max < 1e-6, "expected silence, got max abs {max}");
}

#[test]
fn unity_pitch_preserves_peak_frequency() {
    let mut pv = PhaseVocoder::new(WINDOW);
    pv.set_pitch_factor(1.0);
    let freq = 1000.0_f32;
    let warmup = WARMUP_LATENCY + WINDOW;
    let capture = WINDOW * 2;
    let input = sine(freq, warmup + capture);

    let mut output = Vec::with_capacity(capture);
    for (idx, sample) in input.iter().enumerate() {
        let y = pv.process_sample(*sample);
        if idx >= warmup {
            output.push(y);
        }
    }

    let bin = peak_bin(&output);
    let expected = (freq * capture as f32 / SAMPLE_RATE).round() as usize;
    let tol = 2;
    assert!(
        bin.abs_diff(expected) <= tol,
        "peak bin {bin} not within {tol} of expected {expected}"
    );
}

#[test]
fn octave_up_doubles_peak_frequency() {
    let mut pv = PhaseVocoder::new(WINDOW);
    pv.set_pitch_factor(2.0);
    let freq = 440.0_f32;
    let warmup = WARMUP_LATENCY + WINDOW;
    let capture = WINDOW * 2;
    let input = sine(freq, warmup + capture);

    let mut output = Vec::with_capacity(capture);
    for (idx, sample) in input.iter().enumerate() {
        let y = pv.process_sample(*sample);
        if idx >= warmup {
            output.push(y);
        }
    }

    let bin = peak_bin(&output);
    let expected = (2.0 * freq * capture as f32 / SAMPLE_RATE).round() as usize;
    let tol = 3;
    assert!(
        bin.abs_diff(expected) <= tol,
        "peak bin {bin} not within {tol} of expected {expected} (880 Hz)"
    );
}

#[test]
fn octave_down_halves_peak_frequency() {
    let mut pv = PhaseVocoder::new(WINDOW);
    pv.set_pitch_factor(0.5);
    let freq = 1000.0_f32;
    let warmup = WARMUP_LATENCY + WINDOW;
    let capture = WINDOW * 2;
    let input = sine(freq, warmup + capture);

    let mut output = Vec::with_capacity(capture);
    for (idx, sample) in input.iter().enumerate() {
        let y = pv.process_sample(*sample);
        if idx >= warmup {
            output.push(y);
        }
    }

    let bin = peak_bin(&output);
    let expected = (0.5 * freq * capture as f32 / SAMPLE_RATE).round() as usize;
    let tol = 3;
    assert!(
        bin.abs_diff(expected) <= tol,
        "peak bin {bin} not within {tol} of expected {expected} (500 Hz)"
    );
}

#[test]
fn process_sample_is_finite_for_extreme_inputs() {
    let mut pv = PhaseVocoder::new(WINDOW);
    pv.set_pitch_factor(1.5);
    for n in 0..(WINDOW * 4) {
        let amp = if n % 2 == 0 { 1.0 } else { -1.0 };
        let y = pv.process_sample(amp);
        assert!(y.is_finite(), "non-finite output at n={n}: {y}");
    }
}
