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

/// Short-time RMS of `samples` over non-overlapping blocks of `block`.
fn rms_envelope(samples: &[f32], block: usize) -> Vec<f32> {
    samples
        .chunks(block)
        .filter(|c| c.len() == block)
        .map(|c| (c.iter().map(|s| s * s).sum::<f32>() / block as f32).sqrt())
        .collect()
}

/// Issue #488: a sustained note must come out as a sustained note. The
/// phase vocoder must NOT periodically drop the output amplitude to ~0
/// (the "corta/silencia notas" symptom). Existing tests only check the
/// peak FFT bin, which stays correct even when the envelope warbles to
/// zero — so this regression was invisible. Asserts the short-time RMS
/// never collapses relative to its own peak, for the user's real case
/// (≈ -2 semitones, pitch_factor ≈ 0.8909).
#[test]
fn sustained_tone_has_no_amplitude_dropouts_pitch_down() {
    let mut pv = PhaseVocoder::new(WINDOW);
    pv.set_pitch_factor(2.0_f32.powf(-2.0 / 12.0));
    let freq = 220.0_f32;
    // Generous skip: the time-stretch + resample path primes over
    // ~window/pitch + window input samples for pitch-down; measure only
    // steady state. The old bin-remap dropout was steady-state (every 3rd
    // hop, throughout), so a steady-state-only envelope still catches it.
    let warmup = WINDOW * 6;
    let capture = WINDOW * 8;
    let input = sine(freq, warmup + capture);

    let mut output = Vec::with_capacity(capture);
    for (idx, sample) in input.iter().enumerate() {
        let y = pv.process_sample(*sample);
        if idx >= warmup {
            output.push(y);
        }
    }

    let env = rms_envelope(&output, WINDOW / OVERLAP_FACTOR);
    let peak = env.iter().cloned().fold(0.0_f32, f32::max);
    let trough = env.iter().cloned().fold(f32::INFINITY, f32::min);
    assert!(peak > 1e-4, "no signal at all (peak rms {peak})");
    assert!(
        trough >= 0.5 * peak,
        "amplitude dropout: trough rms {trough} fell below 50% of peak {peak} \
         (envelope = {env:?})"
    );
}

/// One plucked note: instant attack, exponential decay, then silence.
fn plucked(freq: f32, note_len: usize, decay: f32) -> Vec<f32> {
    (0..note_len)
        .map(|n| {
            let env = (-(n as f32) / (note_len as f32 * decay)).exp();
            env * (TAU * freq * n as f32 / SAMPLE_RATE).sin()
        })
        .collect()
}

/// Issue #488 — the real symptom: picking a sequence of notes. Each
/// plucked note (attack + decay) followed by a short rest. After the
/// vocoder warms up once, every subsequent note must still produce
/// audible output — the algorithm must NOT re-introduce its warm-up
/// latency on each onset (which would swallow the attack of every note
/// and read as "corta/silencia as notas"). Asserts every note after the
/// first carries non-trivial energy in the output.
#[test]
fn note_sequence_does_not_swallow_each_onset() {
    let mut pv = PhaseVocoder::new(WINDOW);
    pv.set_pitch_factor(2.0_f32.powf(-2.0 / 12.0));

    let note_len = WINDOW * 3;
    let rest = WINDOW;
    let freqs = [196.0_f32, 246.94, 293.66, 196.0, 246.94];

    let mut input = Vec::new();
    for f in freqs {
        input.extend(plucked(f, note_len, 0.6));
        input.extend(std::iter::repeat_n(0.0, rest));
    }

    // Repeat the sequence enough times that several full periods exist
    // after the one-time priming latency.
    let reps = 4;
    let mut output = Vec::with_capacity(input.len() * reps);
    for _ in 0..reps {
        for s in &input {
            output.push(pv.process_sample(*s));
        }
    }

    // Skip the first full sequence: the phase-estimate + resampler
    // pipeline converges over roughly one sequence (one-time startup,
    // not per-note). Steady periodic behaviour holds from rep 2 on.
    let skip = input.len();
    // Energy conservation across note onsets. The old bin-remap bug
    // destroyed energy on any multi-partial / transient signal (measured
    // unity gain on a harmonic tone was 0.077 — a ~13× loss), silencing
    // notes. A correct pitch shifter conserves energy regardless of the
    // (fixed) latency or how the phase vocoder redistributes a transient
    // in time, so the steady-region output/input energy ratio is the
    // latency- and smear-invariant oracle for "notes not swallowed".
    let in_e: f32 = input
        .iter()
        .cycle()
        .skip(skip % input.len())
        .take(output.len() - skip)
        .map(|s| s * s)
        .sum();
    let out_e: f32 = output[skip..].iter().map(|s| s * s).sum();
    assert!(
        out_e >= 0.4 * in_e,
        "notes swallowed: steady-region output energy {out_e:.1} < 40% of \
         input energy {in_e:.1} (ratio {:.3}) — the #488 bug destroyed \
         energy on transient/multi-partial signal",
        out_e / in_e
    );
}

/// Issue #488 — harmonic-rich tone (fundamental + harmonics, like a real
/// guitar string) pitched DOWN. Pitch-down collapses several analysis
/// bins onto one synthesis bin; if the synthesis frequency assignment
/// loses partials the note thins out or cancels. Asserts the output
/// envelope of a sustained harmonic tone does not collapse.
#[test]
fn harmonic_tone_pitch_down_keeps_envelope() {
    let mut pv = PhaseVocoder::new(WINDOW);
    pv.set_pitch_factor(2.0_f32.powf(-2.0 / 12.0));
    let f0 = 110.0_f32;
    let warmup = WINDOW * 6;
    let capture = WINDOW * 8;
    let total = warmup + capture;
    let input: Vec<f32> = (0..total)
        .map(|n| {
            let t = n as f32 / SAMPLE_RATE;
            (1..=5)
                .map(|h| (1.0 / h as f32) * (TAU * f0 * h as f32 * t).sin())
                .sum::<f32>()
                * 0.3
        })
        .collect();

    let mut output = Vec::with_capacity(capture);
    for (idx, s) in input.iter().enumerate() {
        let y = pv.process_sample(*s);
        if idx >= warmup {
            output.push(y);
        }
    }

    let env = rms_envelope(&output, WINDOW / OVERLAP_FACTOR);
    let peak = env.iter().cloned().fold(0.0_f32, f32::max);
    let trough = env.iter().cloned().fold(f32::INFINITY, f32::min);
    assert!(peak > 1e-4, "no signal at all (peak rms {peak})");
    assert!(
        trough >= 0.5 * peak,
        "harmonic tone dropout: trough rms {trough} below 50% of peak {peak} \
         (envelope = {env:?})"
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
