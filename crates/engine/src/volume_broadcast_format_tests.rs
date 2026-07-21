//! Volume/audio invariants — PINNED (issue #792 split from volume_invariants_tests.rs).
//! Section moved verbatim; shared fixtures live in `volume_invariants_tests.rs`.
#![allow(unused_imports)]
use super::*;
use super::volume_invariants::*;

// ─────────────────────────────────────────────────────────────────────────
// N. Mono→Stereo broadcast — 30 tests probing L=R bit-equality across
//    levels, frequencies, signal shapes, and buffer sizes.
// ─────────────────────────────────────────────────────────────────────────

fn max_lr_drift(chain: &Chain, registry: &[IoBinding], sig: &[f32], buffer: usize) -> f32 {
    let runtime = build_runtime(chain, registry);
    let mut max_drift = 0.0_f32;
    for (callback_idx, chunk) in sig.chunks(buffer).enumerate() {
        process_input_f32(&runtime, 0, chunk, 1);
        let mut out = vec![0.0_f32; chunk.len() * 2];
        process_output_f32(&runtime, 0, &mut out, 2);
        // Skip first 4 callbacks to avoid fade-in artifacts.
        if callback_idx >= 4 {
            for f in out.chunks_exact(2) {
                let d = (f[0] - f[1]).abs();
                if d > max_drift {
                    max_drift = d;
                }
            }
        }
    }
    max_drift
}

macro_rules! bcast_sine_test {
    ($name:ident, $f:expr, $amp:expr, $buf:expr) => {
        #[test]
        fn $name() {
            let (chain, registry) = bare_chain_for(stringify!($name));
            let sig: Vec<f32> = (0..(SR as usize))
                .map(|i| $amp * (2.0 * std::f32::consts::PI * $f * i as f32 / SR).sin())
                .collect();
            let d = max_lr_drift(&chain, &registry, &sig, $buf);
            eprintln!(
                "[bcast f={} amp={} buf={}] max drift = {d:.6}",
                $f, $amp, $buf
            );
            assert!(d < 1e-5, "L vs R drift {d:.6} ≥ 1e-5");
        }
    };
}

bcast_sine_test!(n01_b_100hz_0_3_buf_128, 100.0, 0.3, 128);
bcast_sine_test!(n02_b_220hz_0_3_buf_128, 220.0, 0.3, 128);
bcast_sine_test!(n03_b_440hz_0_3_buf_128, 440.0, 0.3, 128);
bcast_sine_test!(n04_b_1khz_0_3_buf_128, 1_000.0, 0.3, 128);
bcast_sine_test!(n05_b_4khz_0_3_buf_128, 4_000.0, 0.3, 128);
bcast_sine_test!(n06_b_220hz_0_1_buf_512, 220.0, 0.1, 512);
bcast_sine_test!(n07_b_220hz_0_5_buf_512, 220.0, 0.5, 512);
bcast_sine_test!(n08_b_220hz_0_8_buf_512, 220.0, 0.8, 512);
bcast_sine_test!(n09_b_220hz_0_95_buf_512, 220.0, 0.95, 512);
bcast_sine_test!(n10_b_1khz_0_3_buf_64, 1_000.0, 0.3, 64);
bcast_sine_test!(n11_b_1khz_0_3_buf_256, 1_000.0, 0.3, 256);
bcast_sine_test!(n12_b_1khz_0_3_buf_768, 1_000.0, 0.3, 768);
bcast_sine_test!(n13_b_1khz_0_3_buf_2048, 1_000.0, 0.3, 2048);

macro_rules! bcast_signal_test {
    ($name:ident, $sig_expr:expr, $buf:expr) => {
        #[test]
        fn $name() {
            let (chain, registry) = bare_chain_for(stringify!($name));
            let sig: Vec<f32> = $sig_expr;
            let d = max_lr_drift(&chain, &registry, &sig, $buf);
            eprintln!("[{} buf={}] max drift = {d:.6}", stringify!($name), $buf);
            assert!(d < 1e-5, "L vs R drift {d:.6} ≥ 1e-5");
        }
    };
}

bcast_signal_test!(n14_b_dc_pos, vec![0.3_f32; (SR as usize) * 2], 256);
bcast_signal_test!(n15_b_dc_neg, vec![-0.4_f32; (SR as usize) * 2], 256);
bcast_signal_test!(n16_b_silence, vec![0.0_f32; (SR as usize) * 2], 256);
bcast_signal_test!(n17_b_pink_noise, pink_noise((SR as usize) * 2, 0xCAFE), 256);
bcast_signal_test!(
    n18_b_two_tone,
    (0..(SR as usize))
        .map(
            |i| 0.3 * (2.0 * std::f32::consts::PI * 220.0 * i as f32 / SR).sin()
                + 0.3 * (2.0 * std::f32::consts::PI * 440.0 * i as f32 / SR).sin()
        )
        .collect(),
    256
);
bcast_signal_test!(
    n19_b_ramp_up,
    (0..(SR as usize)).map(|i| 0.8 * (i as f32 / SR)).collect(),
    256
);
bcast_signal_test!(
    n20_b_pluck,
    (0..(SR as usize))
        .map(|i| {
            let t = i as f32 / SR;
            0.5 * (-t / 0.4).exp() * (2.0 * std::f32::consts::PI * 150.0 * t).sin()
        })
        .collect(),
    256
);
bcast_signal_test!(
    n21_b_impulse,
    {
        let mut v = vec![0.0_f32; SR as usize];
        v[128] = 0.7;
        v
    },
    256
);
bcast_signal_test!(
    n22_b_square,
    (0..(SR as usize))
        .map(|i| if (i / 240) % 2 == 0 { 0.3 } else { -0.3 })
        .collect(),
    256
);
bcast_signal_test!(
    n23_b_sawtooth,
    (0..(SR as usize))
        .map(|i| 0.4 * (((i as f32 % 240.0) / 240.0) * 2.0 - 1.0))
        .collect(),
    256
);
bcast_signal_test!(
    n24_b_triangle,
    (0..(SR as usize))
        .map(|i| {
            let p = (i as f32 % 240.0) / 240.0;
            0.4 * (1.0 - (2.0 * p - 1.0).abs() * 2.0)
        })
        .collect(),
    256
);

bcast_sine_test!(n25_b_100hz_0_5_buf_64, 100.0, 0.5, 64);
bcast_sine_test!(n26_b_100hz_0_5_buf_2048, 100.0, 0.5, 2048);
bcast_sine_test!(n27_b_4khz_0_5_buf_64, 4_000.0, 0.5, 64);
bcast_sine_test!(n28_b_4khz_0_5_buf_2048, 4_000.0, 0.5, 2048);
bcast_sine_test!(n29_b_8khz_0_3_buf_512, 8_000.0, 0.3, 512);
bcast_sine_test!(n30_b_60hz_0_5_buf_512, 60.0, 0.5, 512);

// ─────────────────────────────────────────────────────────────────────────
// O. Fade-in ramp — 30 tests checking the ramp does not leak past its
//    documented duration, does not corrupt audio after release, and
//    does not introduce harmonics.
// ─────────────────────────────────────────────────────────────────────────

/// THD+N computed using a specified skip duration (samples). If THD+N
/// keeps improving as skip grows, the fade-in is leaking — its end
/// boundary should be a hard release into transparency.
fn thd_with_skip(
    chain: &Chain,
    registry: &[IoBinding],
    freq: f32,
    amp: f32,
    buffer: usize,
    skip: usize,
) -> f32 {
    use rustfft::{num_complex::Complex, FftPlanner};
    let target = DEFAULT_ELASTIC_TARGET.max(buffer);
    let runtime =
        Arc::new(build_chain_runtime_state(chain, SR, &[target], registry).expect("runtime"));
    let n: usize = (SR as usize) * 3;
    let sig: Vec<f32> = (0..n)
        .map(|i| amp * (2.0 * std::f32::consts::PI * freq * i as f32 / SR).sin())
        .collect();
    let mut out_collected: Vec<f32> = Vec::with_capacity(sig.len());
    for chunk in sig.chunks(buffer) {
        process_input_f32(&runtime, 0, chunk, 1);
        let mut out = vec![0.0_f32; chunk.len() * 2];
        process_output_f32(&runtime, 0, &mut out, 2);
        for f in out.chunks_exact(2) {
            out_collected.push((f[0] + f[1]) * 0.5);
        }
    }
    // Issue #496 measurement fix: truncate to integer cycles, no zero-pad.
    let cycle_samples = (SR / freq).round().max(1.0) as usize;
    let usable_total = out_collected.len() - skip;
    let usable = (usable_total / cycle_samples) * cycle_samples;
    let tail = &out_collected[skip..skip + usable];
    let nfft = tail.len();
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(nfft);
    let mut buf: Vec<Complex<f32>> = tail.iter().map(|&s| Complex::new(s, 0.0)).collect();
    fft.process(&mut buf);
    let bin_hz = SR / nfft as f32;
    let fb = (freq / bin_hz).round() as usize;
    let fundamental: f32 = (fb.saturating_sub(1)..=fb + 1)
        .map(|b| buf[b].norm_sqr())
        .sum();
    let total: f32 = buf[..nfft / 2].iter().map(|c| c.norm_sqr()).sum();
    10.0 * ((total - fundamental).max(1e-12) / fundamental).log10()
}

macro_rules! fade_skip_test {
    ($name:ident, $skip_ms:expr, $buf:expr) => {
        #[test]
        fn $name() {
            let (chain, registry) = bare_chain_for(stringify!($name));
            let skip = (SR as usize) * $skip_ms / 1000;
            let thd = thd_with_skip(&chain, &registry, 1_000.0, 0.5, $buf, skip);
            eprintln!("[skip {} ms, buf {}] THD+N = {thd:.2} dB", $skip_ms, $buf);
            assert!(
                thd < -60.0,
                "skip={}ms buf={} THD+N {thd:.2} dB ≥ -60",
                $skip_ms,
                $buf
            );
        }
    };
}
fade_skip_test!(o01_skip_50ms_buf_128, 50, 128);
fade_skip_test!(o02_skip_100ms_buf_128, 100, 128);
fade_skip_test!(o03_skip_200ms_buf_128, 200, 128);
fade_skip_test!(o04_skip_500ms_buf_128, 500, 128);
fade_skip_test!(o05_skip_1s_buf_128, 1_000, 128);
fade_skip_test!(o06_skip_50ms_buf_512, 50, 512);
fade_skip_test!(o07_skip_100ms_buf_512, 100, 512);
fade_skip_test!(o08_skip_200ms_buf_512, 200, 512);
fade_skip_test!(o09_skip_500ms_buf_512, 500, 512);
fade_skip_test!(o10_skip_1s_buf_512, 1_000, 512);
fade_skip_test!(o11_skip_50ms_buf_2048, 50, 2048);
fade_skip_test!(o12_skip_200ms_buf_2048, 200, 2048);
fade_skip_test!(o13_skip_500ms_buf_2048, 500, 2048);
fade_skip_test!(o14_skip_1s_buf_2048, 1_000, 2048);

// Across signal levels
fade_skip_test!(o15_skip_500ms_lvl_via_freq_100, 500, 256);
fade_skip_test!(o16_skip_500ms_lvl_via_freq_220, 500, 256);
fade_skip_test!(o17_skip_500ms_lvl_via_freq_440, 500, 256);
fade_skip_test!(o18_skip_500ms_lvl_via_freq_2k, 500, 256);
fade_skip_test!(o19_skip_500ms_lvl_via_freq_8k, 500, 256);

// Across many buffers at fixed 500 ms skip
fade_skip_test!(o20_skip_500ms_buf_64, 500, 64);
fade_skip_test!(o21_skip_500ms_buf_192, 500, 192);
fade_skip_test!(o22_skip_500ms_buf_384, 500, 384);
fade_skip_test!(o23_skip_500ms_buf_768, 500, 768);
fade_skip_test!(o24_skip_500ms_buf_1024, 1_024, 1_024);
fade_skip_test!(o25_skip_500ms_buf_1536, 500, 1_536);
fade_skip_test!(o26_skip_500ms_buf_4096, 500, 4_096);

// Skip much longer than any plausible fade
fade_skip_test!(o27_skip_2s_buf_512, 2_000, 512);
fade_skip_test!(o28_skip_2s_buf_2048, 2_000, 2_048);
fade_skip_test!(o29_skip_2s_buf_128, 2_000, 128);
fade_skip_test!(o30_skip_2s_buf_64, 2_000, 64);

// ─────────────────────────────────────────────────────────────────────────
// P. Sample format conversion math — 30 tests of the exact
//    i16/u16/i32 ↔ f32 expressions used by `stream_builder.rs`.
//    These don't go through the engine; they verify the math the cpal
//    callback runs is bijective and not the source of the swarm-of-bees
//    via bit-cast / off-by-one / wrap-around.
// ─────────────────────────────────────────────────────────────────────────

fn i16_to_f32(s: i16) -> f32 {
    s as f32 / i16::MAX as f32
}
fn f32_to_i16(s: f32) -> i16 {
    (s * i16::MAX as f32).clamp(i16::MIN as f32, i16::MAX as f32) as i16
}
fn u16_to_f32(s: u16) -> f32 {
    (s as f32 / u16::MAX as f32) * 2.0 - 1.0
}
fn f32_to_u16(s: f32) -> u16 {
    ((s + 1.0) * 0.5 * u16::MAX as f32).clamp(0.0, u16::MAX as f32) as u16
}
fn i32_to_f32(s: i32) -> f32 {
    s as f32 / i32::MAX as f32
}
fn f32_to_i32(s: f32) -> i32 {
    (s * i32::MAX as f32).clamp(i32::MIN as f32, i32::MAX as f32) as i32
}

#[test]
fn p01_i16_max_round_trip() {
    assert!((f32_to_i16(i16_to_f32(i16::MAX)) - i16::MAX).abs() <= 1);
}
#[test]
fn p02_i16_min_round_trip() {
    assert!((f32_to_i16(i16_to_f32(i16::MIN)) - i16::MIN).abs() <= 1);
}
#[test]
fn p03_i16_zero_round_trip() {
    assert_eq!(f32_to_i16(i16_to_f32(0)), 0);
}
#[test]
fn p04_i16_one_round_trip() {
    assert_eq!(f32_to_i16(i16_to_f32(1)), 1);
}
#[test]
fn p05_i16_neg_one_round_trip() {
    assert_eq!(f32_to_i16(i16_to_f32(-1)), -1);
}
#[test]
fn p06_i16_half_round_trip() {
    let v = i16::MAX / 2;
    assert!((f32_to_i16(i16_to_f32(v)) - v).abs() <= 1);
}
#[test]
fn p07_i16_neg_half_round_trip() {
    let v = i16::MIN / 2;
    assert!((f32_to_i16(i16_to_f32(v)) - v).abs() <= 1);
}
#[test]
fn p08_i16_clamps_above_unity() {
    assert_eq!(f32_to_i16(2.0), i16::MAX);
}
#[test]
fn p09_i16_clamps_below_minus_unity() {
    assert_eq!(f32_to_i16(-2.0), i16::MIN);
}
#[test]
fn p10_i16_to_f32_bound() {
    for v in [-32768i16, -1, 0, 1, 32767] {
        let x = i16_to_f32(v);
        assert!((-1.001..=1.001).contains(&x), "v={v} x={x}");
    }
}

#[test]
fn p11_u16_zero_maps_to_minus_one() {
    assert!((u16_to_f32(0) + 1.0).abs() < 1e-4);
}
#[test]
fn p12_u16_max_maps_to_plus_one() {
    assert!((u16_to_f32(u16::MAX) - 1.0).abs() < 1e-4);
}
#[test]
fn p13_u16_mid_maps_near_zero() {
    let v = u16::MAX / 2;
    assert!(u16_to_f32(v).abs() < 1e-4);
}
#[test]
fn p14_u16_round_trip_zero() {
    assert_eq!(f32_to_u16(-1.0), 0);
}
#[test]
fn p15_u16_round_trip_max() {
    assert_eq!(f32_to_u16(1.0), u16::MAX);
}
#[test]
fn p16_u16_round_trip_mid() {
    let v = u16::MAX / 2;
    let back = f32_to_u16(u16_to_f32(v));
    assert!((back as i32 - v as i32).abs() <= 1);
}
#[test]
fn p17_u16_clamps_above_unity() {
    assert_eq!(f32_to_u16(2.0), u16::MAX);
}
#[test]
fn p18_u16_clamps_below_minus_unity() {
    assert_eq!(f32_to_u16(-2.0), 0);
}
#[test]
fn p19_u16_to_f32_bound() {
    for v in [0u16, 1, u16::MAX / 2, u16::MAX] {
        let x = u16_to_f32(v);
        assert!((-1.001..=1.001).contains(&x));
    }
}
#[test]
fn p20_u16_round_trip_dense() {
    for v in (0..u16::MAX).step_by(257) {
        let back = f32_to_u16(u16_to_f32(v));
        assert!((back as i32 - v as i32).abs() <= 1, "v={v} back={back}");
    }
}

#[test]
fn p21_i32_zero_round_trip() {
    assert_eq!(f32_to_i32(i32_to_f32(0)), 0);
}
#[test]
fn p22_i32_max_round_trip_bounded() {
    let x = i32_to_f32(i32::MAX);
    assert!((x - 1.0).abs() < 1e-6);
}
#[test]
fn p23_i32_min_round_trip_bounded() {
    let x = i32_to_f32(i32::MIN);
    assert!((x + 1.0).abs() < 1e-3);
}
#[test]
fn p24_i32_clamps_above_unity() {
    assert_eq!(f32_to_i32(2.0), i32::MAX);
}
#[test]
fn p25_i32_clamps_below_minus_unity() {
    assert_eq!(f32_to_i32(-2.0), i32::MIN);
}
#[test]
fn p26_i32_to_f32_bound() {
    for v in [i32::MIN, -1, 0, 1, i32::MAX] {
        let x = i32_to_f32(v);
        assert!((-1.001..=1.001).contains(&x), "v={v} x={x}");
    }
}
#[test]
fn p27_i32_unity_round_trip() {
    assert_eq!(f32_to_i32(1.0), i32::MAX);
}
#[test]
fn p28_i32_neg_unity_round_trip() {
    assert_eq!(f32_to_i32(-1.0), i32::MIN);
}
#[test]
fn p29_i32_below_unity_is_within() {
    for &v in &[0.1_f32, 0.5, 0.9, -0.3] {
        assert!(f32_to_i32(v).abs() < i32::MAX);
    }
}
#[test]
fn p30_i32_subnormal_safe() {
    let x = i32_to_f32(1);
    let back = f32_to_i32(x);
    assert!((back - 1).abs() <= 1);
}

#[test]
fn diag_thd_with_single_callback_push_pop() {
    use rustfft::{num_complex::Complex, FftPlanner};
    let (chain, registry) = bare_chain_for("diag_single");
    let n: usize = 16_384;
    let sig: Vec<f32> = (0..n)
        .map(|i| 0.5_f32 * (2.0 * std::f32::consts::PI * 1_000.0 * i as f32 / SR).sin())
        .collect();
    let runtime = build_runtime(&chain, &registry);
    // Single big push + single big pop.
    process_input_f32(&runtime, 0, &sig, 1);
    let mut out_st = vec![0.0_f32; n * 2];
    process_output_f32(&runtime, 0, &mut out_st, 2);
    let out: Vec<f32> = out_st
        .chunks_exact(2)
        .map(|f| (f[0] + f[1]) * 0.5_f32)
        .collect();
    // Skip fade-in.
    let skip = (crate::runtime_state::FADE_IN_FRAMES + 16).min(out.len() / 4);
    let tail = &out[skip..];
    let nfft = tail.len().next_power_of_two();
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(nfft);
    let mut buf: Vec<Complex<f32>> = tail
        .iter()
        .map(|&s| Complex::new(s, 0.0))
        .chain(std::iter::repeat(Complex::new(0.0, 0.0)))
        .take(nfft)
        .collect();
    fft.process(&mut buf);
    let bin_hz = SR / nfft as f32;
    let fb = (1_000.0 / bin_hz).round() as usize;
    let fundamental: f32 = (fb.saturating_sub(3)..=fb + 3)
        .map(|b| buf[b].norm_sqr())
        .sum();
    let total: f32 = buf[..nfft / 2].iter().map(|c| c.norm_sqr()).sum();
    let thd_n_db = 10.0 * ((total - fundamental).max(1e-12) / fundamental).log10();
    eprintln!("\n=== diag SINGLE callback push/pop ===\n  THD+N = {thd_n_db:.2} dB  (signal length = {n} samples)");
}

#[test]
fn diag_multi_callback_bit_exact_chunks_of_64() {
    let (chain, registry) = bare_chain_for("diag_multi");
    let n: usize = 4096;
    let sig: Vec<f32> = (0..n)
        .map(|i| 0.5_f32 * (2.0 * std::f32::consts::PI * 1_000.0 * i as f32 / SR).sin())
        .collect();
    let runtime = build_runtime(&chain, &registry);
    let buffer = 64;
    let mut out_collected: Vec<f32> = Vec::with_capacity(n);
    for chunk in sig.chunks(buffer) {
        process_input_f32(&runtime, 0, chunk, 1);
        let mut out = vec![0.0_f32; chunk.len() * 2];
        process_output_f32(&runtime, 0, &mut out, 2);
        for f in out.chunks_exact(2) {
            out_collected.push((f[0] + f[1]) * 0.5_f32);
        }
    }
    // Count exact mismatches per region.
    let skip = 128_usize; // past FADE_IN_FRAMES
    let mut mismatches = 0_usize;
    let mut worst: (usize, f32, f32) = (0, 0.0, 0.0);
    for i in skip..n {
        let want = sig[i];
        let got = out_collected[i];
        let d = (got - want).abs();
        if d > 1e-5 {
            mismatches += 1;
            if d > worst.1.abs().max(worst.2.abs()).max(0.0) {
                worst = (i, want, got);
            }
        }
    }
    eprintln!("\n=== diag MULTI 64-frame callbacks (skip {skip}) ===");
    eprintln!("  total frames after skip = {}", n - skip);
    eprintln!("  mismatches (|delta| > 1e-5) = {mismatches}");
    eprintln!(
        "  worst @ i={}: want={:+.6} got={:+.6}",
        worst.0, worst.1, worst.2
    );
    // Print 16 around worst.
    let around = worst.0.saturating_sub(8);
    eprintln!("  around worst (i={around}..{}):", around + 16);
    for i in around..(around + 16).min(n) {
        eprintln!(
            "   {i:>5}: want={:>+9.6}  got={:>+9.6}  delta={:>+9.6}",
            sig[i],
            out_collected[i],
            out_collected[i] - sig[i]
        );
    }
}

#[test]
fn diag_print_first_chunk_of_bare_chain_output() {
    let (chain, registry) = bare_chain_for("diag");
    let n: usize = 256;
    let sig: Vec<f32> = (0..n)
        .map(|i| 0.5 * (2.0 * std::f32::consts::PI * 1_000.0 * i as f32 / SR).sin())
        .collect();
    let runtime = build_runtime(&chain, &registry);
    // Push and pop a few callbacks to get past fade-in.
    for _ in 0..4 {
        process_input_f32(&runtime, 0, &sig, 1);
        let mut out = vec![0.0_f32; n * 2];
        process_output_f32(&runtime, 0, &mut out, 2);
    }
    // Capture next callback.
    process_input_f32(&runtime, 0, &sig, 1);
    let mut out = vec![0.0_f32; n * 2];
    process_output_f32(&runtime, 0, &mut out, 2);
    eprintln!("\n=== diag: first 16 stereo frames after warmup ===");
    eprintln!("  i:  in            outL          outR          delta");
    for i in 0..16 {
        let want = sig[i];
        let l = out[i * 2];
        let r = out[i * 2 + 1];
        eprintln!(
            " {i:>3}: {want:>+10.6}  {l:>+10.6}  {r:>+10.6}  L-want={:+.6}",
            l - want
        );
    }
}
