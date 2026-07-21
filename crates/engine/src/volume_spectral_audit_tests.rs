//! Volume/audio invariants — PINNED (issue #792 split from volume_invariants_tests.rs).
//! Section moved verbatim; shared fixtures live in `volume_invariants_tests.rs`.
#![allow(unused_imports)]
use super::*;
use super::volume_invariants::*;

// ─────────────────────────────────────────────────────────────────────────
// L. Real-engine spectral / quality audit (issue #496).
//
// Drives PINK NOISE (= equal energy per octave, the universal frequency-
// response reference) through a *real* OpenRig chain — chain → runtime
// → `process_input_f32` → `process_output_f32` — and measures objective
// quality on what comes out. No ear, no synthetic math substitute. If
// the bare path (input + output, no blocks) colours the spectrum or
// adds noise, "all-native chain sounds broken" is caught here.
// ─────────────────────────────────────────────────────────────────────────


fn fft_octave_db(samples: &[f32], sr: f32) -> Vec<(f32, f32)> {
    use rustfft::{num_complex::Complex, FftPlanner};
    let n = samples.len().next_power_of_two();
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(n);
    let mut buf: Vec<Complex<f32>> = samples
        .iter()
        .map(|&s| Complex::new(s, 0.0))
        .chain(std::iter::repeat(Complex::new(0.0, 0.0)))
        .take(n)
        .collect();
    fft.process(&mut buf);
    let bin_hz = sr / n as f32;
    let centres = [
        62.5_f32, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0,
    ];
    centres
        .iter()
        .map(|&fc| {
            let lo_b = ((fc / std::f32::consts::SQRT_2) / bin_hz).floor() as usize;
            let hi_b = (((fc * std::f32::consts::SQRT_2) / bin_hz).ceil() as usize).min(n / 2);
            let energy: f32 = buf[lo_b..hi_b].iter().map(|c| c.norm_sqr()).sum();
            (fc, 10.0 * energy.max(1e-12).log10())
        })
        .collect()
}

/// Drive `samples` through the real engine, return the captured output
/// as a single mono-equivalent stream (sum of stereo channels if any).
fn run_pink_through_chain(chain: &Chain, registry: &[IoBinding], mono_samples: &[f32]) -> Vec<f32> {
    let runtime = build_runtime(chain, registry);
    let buffer = 512usize;
    let n_callbacks = mono_samples.len().div_ceil(buffer);
    let mut out_collected: Vec<f32> = Vec::with_capacity(mono_samples.len());
    for cb in 0..n_callbacks {
        let start = cb * buffer;
        let end = (start + buffer).min(mono_samples.len());
        let chunk = &mono_samples[start..end];
        process_input_f32(&runtime, 0, chunk, 1);
        let mut out = vec![0.0_f32; chunk.len() * 2]; // assume stereo out
        process_output_f32(&runtime, 0, &mut out, 2);
        for f in out.chunks_exact(2) {
            out_collected.push((f[0] + f[1]) * 0.5);
        }
    }
    out_collected
}

#[test]
fn l01_real_engine_bare_chain_preserves_spectrum_per_octave() {
    // The simplest possible REAL chain: mono input → stereo output,
    // no blocks. If even THIS colours the spectrum, every chain is
    // mangled at the I/O layer — that's the structural bug.
    let (chain, registry) = chain_with_blocks(
        "l01",
        input_mono(vec![0]),
        vec![],
        output(ChannelMode::Stereo, vec![0, 1]),
    );
    let pink = pink_noise(SR as usize * 2, 0xC0FFEE);
    let out = run_pink_through_chain(&chain, &registry, &pink);
    // Skip the fade-in tail (first ~50 ms of warmup callbacks).
    let skip = (SR as usize) / 20;
    let in_bands = fft_octave_db(&pink[skip..], SR);
    let out_bands = fft_octave_db(&out[skip..], SR);
    eprintln!("\n=== REAL engine bare chain @ unity (mono→stereo) ===");
    eprintln!(" centre Hz   in dB    out dB    delta");
    let mut worst = (0.0_f32, 0.0_f32);
    for ((fc, i), (_, o)) in in_bands.iter().zip(out_bands.iter()) {
        let d = o - i;
        eprintln!(" {fc:>9.1}   {i:>7.2}   {o:>7.2}   {d:>+6.2}");
        if d.abs() > worst.1.abs() {
            worst = (*fc, d);
        }
    }
    assert!(
        worst.1.abs() < 1.0,
        "REAL ENGINE coloured the spectrum at {} Hz by {:+.2} dB — \
         every chain is bandpassed by the bare path",
        worst.0,
        worst.1
    );
}

#[test]
fn l02_real_engine_bare_chain_thd_n_low_for_pure_sine() {
    use rustfft::{num_complex::Complex, FftPlanner};
    let (chain, registry) = chain_with_blocks(
        "l02",
        input_mono(vec![0]),
        vec![],
        output(ChannelMode::Stereo, vec![0, 1]),
    );
    let n: usize = SR as usize;
    let sig: Vec<f32> = (0..n)
        .map(|i| 0.5 * (2.0 * std::f32::consts::PI * 1_000.0 * i as f32 / SR).sin())
        .collect();
    let out = run_pink_through_chain(&chain, &registry, &sig);
    let skip = (SR as usize) / 20;
    // Issue #496 measurement fix: integer cycles, no zero-pad.
    let cycle_samples = (SR / 1_000.0).round() as usize;
    let usable = ((out.len() - skip) / cycle_samples) * cycle_samples;
    let tail = &out[skip..skip + usable];
    let nfft = tail.len();
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(nfft);
    let mut buf: Vec<Complex<f32>> = tail.iter().map(|&s| Complex::new(s, 0.0)).collect();
    fft.process(&mut buf);
    let bin_hz = SR / nfft as f32;
    let fb = (1_000.0 / bin_hz).round() as usize;
    let fundamental: f32 = (fb.saturating_sub(1)..=fb + 1)
        .map(|b| buf[b].norm_sqr())
        .sum();
    let total: f32 = buf[..nfft / 2].iter().map(|c| c.norm_sqr()).sum();
    let thd_n_db = 10.0 * ((total - fundamental).max(1e-12) / fundamental).log10();
    eprintln!("\n=== REAL engine THD+N @ 1 kHz mono→stereo ===\n  THD+N = {thd_n_db:.2} dB");
    assert!(thd_n_db < -60.0, "THD+N = {thd_n_db:.2} dB");
}

#[test]
fn l03_real_engine_bare_chain_lufs_transparent_at_unity() {
    use ebur128::{EbuR128, Mode};
    let (chain, registry) = chain_with_blocks(
        "l03",
        input_mono(vec![0]),
        vec![],
        output(ChannelMode::Stereo, vec![0, 1]),
    );
    let pink = pink_noise(SR as usize * 3, 0xBADA55);
    let out = run_pink_through_chain(&chain, &registry, &pink);
    let skip = (SR as usize) / 20;
    let mut m_in = EbuR128::new(1, SR as u32, Mode::I).unwrap();
    m_in.add_frames_f32(&pink[skip..]).unwrap();
    let mut m_out = EbuR128::new(1, SR as u32, Mode::I).unwrap();
    m_out.add_frames_f32(&out[skip..]).unwrap();
    let lin = m_in.loudness_global().unwrap();
    let lout = m_out.loudness_global().unwrap();
    eprintln!(
        "\n=== REAL engine bare chain LUFS @ unity ===\n  in  = {lin:>7.2} LUFS\n  out = {lout:>7.2} LUFS\n  delta = {:+.2} dB",
        lout - lin
    );
    assert!(
        (lout - lin).abs() < 1.0,
        "REAL ENGINE bare chain LUFS delta {:.2} dB — should be transparent",
        lout - lin
    );
}

/// Drive a long signal and report THD+N AFTER a generous skip — kills
/// the fade-in hypothesis. If THD+N is still bad with skip = 1 s, the
/// noise is steady-state from the path, not a startup transient.
#[test]
fn l04_real_engine_thd_after_one_second_skip_isolates_fade_in() {
    use rustfft::{num_complex::Complex, FftPlanner};
    let (chain, registry) = chain_with_blocks(
        "l04",
        input_mono(vec![0]),
        vec![],
        output(ChannelMode::Stereo, vec![0, 1]),
    );
    let n: usize = (SR as usize) * 3; // 3 seconds
    let sig: Vec<f32> = (0..n)
        .map(|i| 0.5 * (2.0 * std::f32::consts::PI * 1_000.0 * i as f32 / SR).sin())
        .collect();
    let out = run_pink_through_chain(&chain, &registry, &sig);
    let skip = SR as usize; // skip first 1 s
                            // Issue #496 measurement fix: integer cycles, no zero-pad.
    let cycle_samples = (SR / 1_000.0).round() as usize;
    let usable = ((out.len() - skip) / cycle_samples) * cycle_samples;
    let tail = &out[skip..skip + usable];
    let nfft = tail.len();
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(nfft);
    let mut buf: Vec<Complex<f32>> = tail.iter().map(|&s| Complex::new(s, 0.0)).collect();
    fft.process(&mut buf);
    let bin_hz = SR / nfft as f32;
    let fb = (1_000.0 / bin_hz).round() as usize;
    let fundamental: f32 = (fb.saturating_sub(1)..=fb + 1)
        .map(|b| buf[b].norm_sqr())
        .sum();
    let total: f32 = buf[..nfft / 2].iter().map(|c| c.norm_sqr()).sum();
    let thd_n_db = 10.0 * ((total - fundamental).max(1e-12) / fundamental).log10();
    eprintln!("\n=== L04 THD+N (3s sine, 1s skip) ===\n  THD+N = {thd_n_db:.2} dB");
    assert!(
        thd_n_db < -60.0,
        "L04: THD+N {thd_n_db:.2} dB after 1s skip"
    );
}

/// Drive SILENCE and capture output. A clean path produces pure
/// zeros. Any non-zero sample = engine is injecting (fade-in tail,
/// underrun, buffer state, anything).
#[test]
fn l05_real_engine_silent_input_must_produce_silent_output() {
    let (chain, registry) = chain_with_blocks(
        "l05",
        input_mono(vec![0]),
        vec![],
        output(ChannelMode::Stereo, vec![0, 1]),
    );
    let sig = vec![0.0_f32; (SR as usize) * 2];
    let out = run_pink_through_chain(&chain, &registry, &sig);
    let skip = SR as usize; // 1 s skip
    let tail = &out[skip..];
    let peak = tail.iter().fold(0.0_f32, |a, &b| a.max(b.abs()));
    let rms = (tail.iter().map(|v| v * v).sum::<f32>() / tail.len() as f32).sqrt();
    eprintln!("\n=== L05 silent input ===\n  peak = {peak:.6}  rms = {rms:.6}");
    assert!(
        peak < 1e-6,
        "L05: silent input produced non-silent output: peak {peak:.6}"
    );
}

/// DC input (a constant) — there is no signal to harmonise, so any
/// AC content in the output is path-injected noise. Pure isolator.
#[test]
fn l06_real_engine_dc_input_steady_output_has_no_ac_noise() {
    let (chain, registry) = chain_with_blocks(
        "l06",
        input_mono(vec![0]),
        vec![],
        output(ChannelMode::Stereo, vec![0, 1]),
    );
    let sig = vec![0.3_f32; (SR as usize) * 2];
    let out = run_pink_through_chain(&chain, &registry, &sig);
    let skip = SR as usize;
    let tail = &out[skip..];
    let mean = tail.iter().sum::<f32>() / tail.len() as f32;
    let ac_rms = (tail.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / tail.len() as f32).sqrt();
    eprintln!(
        "\n=== L06 DC input (0.3 const) ===\n  output mean = {mean:.6}  AC rms = {ac_rms:.6e}"
    );
    assert!(
        ac_rms < 5e-4,
        "L06: DC in → AC noise out (rms {ac_rms:.6e}, > -66 dBFS = audible)"
    );
}

/// Mono input broadcasts to BOTH stereo output channels — they must
/// be byte-identical. If they drift, the broadcast itself has a bug.
#[test]
fn l07_real_engine_mono_broadcast_writes_identical_l_and_r() {
    let (chain, registry) = chain_with_blocks(
        "l07",
        input_mono(vec![0]),
        vec![],
        output(ChannelMode::Stereo, vec![0, 1]),
    );
    let runtime = build_runtime(&chain, &registry);
    let n_frames = 512;
    let sig: Vec<f32> = (0..n_frames)
        .map(|i| 0.4 * (2.0 * std::f32::consts::PI * 220.0 * i as f32 / SR).sin())
        .collect();
    // Drive a few callbacks then capture the steady one.
    for _ in 0..6 {
        process_input_f32(&runtime, 0, &sig, 1);
    }
    let mut out = vec![0.0_f32; n_frames * 2];
    process_output_f32(&runtime, 0, &mut out, 2);
    let mut max_drift = 0.0_f32;
    for f in out.chunks_exact(2) {
        let drift = (f[0] - f[1]).abs();
        if drift > max_drift {
            max_drift = drift;
        }
    }
    eprintln!("\n=== L07 mono→stereo broadcast ===\n  max L vs R drift = {max_drift:.6}");
    assert!(
        max_drift < 1e-6,
        "L07: broadcast L and R drift by {max_drift:.6} — broadcast is BROKEN"
    );
}

/// Run the SAME signal through TWO different callback buffer sizes
/// and check the output is the same. A path that depends on buffer
/// size has state leaking somewhere (elastic buffer, fade-in counter,
/// FIFO underflow). Same input ⇒ same output.
#[test]
fn l08_real_engine_thd_is_independent_of_callback_buffer_size() {
    use rustfft::{num_complex::Complex, FftPlanner};
    let (chain, registry) = chain_with_blocks(
        "l08",
        input_mono(vec![0]),
        vec![],
        output(ChannelMode::Stereo, vec![0, 1]),
    );
    let n: usize = (SR as usize) * 2;
    let sig: Vec<f32> = (0..n)
        .map(|i| 0.5 * (2.0 * std::f32::consts::PI * 1_000.0 * i as f32 / SR).sin())
        .collect();

    let drive = |buffer: usize| -> f32 {
        let target = DEFAULT_ELASTIC_TARGET.max(buffer);
        let runtime =
            Arc::new(build_chain_runtime_state(&chain, SR, &[target], &registry).expect("runtime"));
        let mut out_collected: Vec<f32> = Vec::with_capacity(sig.len());
        for chunk in sig.chunks(buffer) {
            process_input_f32(&runtime, 0, chunk, 1);
            let mut out = vec![0.0_f32; chunk.len() * 2];
            process_output_f32(&runtime, 0, &mut out, 2);
            for f in out.chunks_exact(2) {
                out_collected.push((f[0] + f[1]) * 0.5);
            }
        }
        let skip = SR as usize;
        // Issue #496 measurement fix: integer cycles, no zero-pad.
        let cycle_samples = (SR / 1_000.0).round() as usize;
        let usable = ((out_collected.len() - skip) / cycle_samples) * cycle_samples;
        let tail = &out_collected[skip..skip + usable];
        let nfft = tail.len();
        let mut planner = FftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(nfft);
        let mut buf: Vec<Complex<f32>> = tail.iter().map(|&s| Complex::new(s, 0.0)).collect();
        fft.process(&mut buf);
        let bin_hz = SR / nfft as f32;
        let fb = (1_000.0 / bin_hz).round() as usize;
        let fundamental: f32 = (fb.saturating_sub(1)..=fb + 1)
            .map(|b| buf[b].norm_sqr())
            .sum();
        let total: f32 = buf[..nfft / 2].iter().map(|c| c.norm_sqr()).sum();
        10.0 * ((total - fundamental).max(1e-12) / fundamental).log10()
    };

    let thd_128 = drive(128);
    let thd_512 = drive(512);
    let thd_2048 = drive(2048);
    eprintln!(
        "\n=== L08 THD+N vs buffer size ===\n  128 frames  → {thd_128:.2} dB\n  512 frames  → {thd_512:.2} dB\n  2048 frames → {thd_2048:.2} dB"
    );
    let spread = thd_128.max(thd_512.max(thd_2048)) - thd_128.min(thd_512.min(thd_2048));
    assert!(
        spread < 3.0,
        "L08: THD+N depends on buffer size (spread = {spread:.2} dB) — elastic / FIFO bug"
    );
}

