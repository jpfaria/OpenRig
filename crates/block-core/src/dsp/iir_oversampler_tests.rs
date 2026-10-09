use super::*;

const SR: f32 = 48_000.0;
const N: usize = 16_384;

fn sine(len: usize, freq: f32, amp: f32) -> Vec<f32> {
    (0..len)
        .map(|i| (std::f64::consts::TAU * freq as f64 * i as f64 / SR as f64).sin() as f32 * amp)
        .collect()
}

fn run(factor: usize, input: &[f32], mut f: impl FnMut(f32) -> f32) -> Vec<f32> {
    let mut os = IirOversampler::new(SR, factor);
    input.iter().map(|&x| os.process(x, &mut f)).collect()
}

fn rms_db(x: &[f32]) -> f32 {
    let tail = &x[x.len() - N..];
    let p: f64 = tail.iter().map(|&s| (s as f64).powi(2)).sum::<f64>() / N as f64;
    10.0 * p.log10() as f32
}

/// Windowed single-frequency DFT magnitude of the last N samples, dB.
fn tone_db(x: &[f32], freq: f32) -> f32 {
    let tail = &x[x.len() - N..];
    let (mut re, mut im) = (0.0_f64, 0.0_f64);
    for (i, &s) in tail.iter().enumerate() {
        let t = std::f64::consts::TAU * i as f64 / N as f64;
        let w = 0.35875 - 0.48829 * t.cos() + 0.14128 * (2.0 * t).cos() - 0.01168 * (3.0 * t).cos();
        let ph = std::f64::consts::TAU * freq as f64 * i as f64 / SR as f64;
        re += s as f64 * w * ph.cos();
        im -= s as f64 * w * ph.sin();
    }
    10.0 * (re * re + im * im).max(1e-300).log10() as f32
}

#[test]
fn passes_the_audio_band_unchanged_in_level() {
    for factor in [2, 4, 8, 16, 32] {
        for freq in [100.0, 1_000.0, 10_000.0, 18_000.0] {
            let input = sine(4 * N, freq, 0.5);
            let out = run(factor, &input, |s| s);
            let delta = rms_db(&out) - rms_db(&input);
            assert!(delta.abs() < 0.05, "x{factor} at {freq} Hz: {delta:.3} dB");
        }
    }
}

#[test]
fn reports_the_delay_it_adds() {
    for factor in [2, 4, 8, 16, 32] {
        let mut os = IirOversampler::new(SR, factor);
        let mut impulse = vec![0.0_f32; 512];
        impulse[0] = 1.0;
        let out: Vec<f32> = impulse.iter().map(|&x| os.process(x, |s| s)).collect();
        let peak = out
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
            .map(|(i, _)| i)
            .unwrap();
        let reported = os.latency_samples();
        assert!(
            peak.abs_diff(reported) <= 1,
            "x{factor}: peak at {peak}, reports {reported}"
        );
        assert!(
            reported <= 8,
            "x{factor}: {reported} samples is too much delay"
        );
    }
}

#[test]
fn keeps_a_hard_clipper_from_folding_into_the_audio_band() {
    // 4998.0 Hz: its 7th and 9th harmonics fold to ~13.0 and ~3.0 kHz.
    let f0 = 1_706.0 * SR / N as f32;
    let folded = [SR - 7.0 * f0, 9.0 * f0 - SR];
    let input = sine(3 * N, f0, 0.5);
    let clip = |s: f32| (s * 30.0).tanh();
    let plain: Vec<f32> = input.iter().map(|&s| clip(s)).collect();
    let oversampled = run(16, &input, clip);
    for alias in folded {
        let before = tone_db(&plain, alias) - tone_db(&plain, f0);
        let after = tone_db(&oversampled, alias) - tone_db(&oversampled, f0);
        assert!(
            before > -40.0,
            "the test tone must alias without oversampling: {before:.1}"
        );
        assert!(after < -90.0, "alias at {alias:.0} Hz still {after:.1} dB");
    }
}

#[test]
fn factor_one_is_a_plain_call() {
    let input = sine(1_000, 440.0, 0.5);
    let out = run(1, &input, |s| s * 2.0);
    assert!(input
        .iter()
        .zip(&out)
        .all(|(a, b)| (a * 2.0 - b).abs() < 1e-7));
    assert_eq!(IirOversampler::new(SR, 1).latency_samples(), 0);
}

#[test]
fn wraps_a_mono_processor_built_at_the_inner_rate() {
    struct Rate(f32);
    impl MonoProcessor for Rate {
        fn process_sample(&mut self, _input: f32) -> f32 {
            self.0
        }
    }
    let mut seen = 0.0;
    let mut wrapped = OversampledMono::new(SR, 8, |rate| {
        seen = rate;
        Box::new(Rate(0.0))
    });
    assert_eq!(seen, SR * 8.0);
    assert!(wrapped.process_sample(1.0).abs() < 1e-20);
}
