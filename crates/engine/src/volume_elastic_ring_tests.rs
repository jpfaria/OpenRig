//! Volume/audio invariants — PINNED (issue #792 split from volume_invariants_tests.rs).
//! Section moved verbatim; shared fixtures live in `volume_invariants_tests.rs`.
#![allow(unused_imports)]
use super::*;
use super::volume_invariants::*;

// ─────────────────────────────────────────────────────────────────────────
// M. Elastic-buffer / SPSC-ring path audit (issue #496, target found via
//    L08). 30+ RED tests probing the exact culprit: per-callback buffer
//    sizes, signal levels, frequencies, DC, silence, LUFS — each test
//    isolates one independent property of a clean signal path.
// ─────────────────────────────────────────────────────────────────────────

fn thd_n_db_through_chain(
    chain: &Chain,
    registry: &[IoBinding],
    sig: &[f32],
    buffer: usize,
) -> f32 {
    thd_n_db_at_freq_through_chain(chain, registry, sig, buffer, 1_000.0)
}

fn thd_n_db_at_freq_through_chain(
    chain: &Chain,
    registry: &[IoBinding],
    sig: &[f32],
    buffer: usize,
    freq: f32,
) -> f32 {
    use rustfft::{num_complex::Complex, FftPlanner};
    let target = DEFAULT_ELASTIC_TARGET.max(buffer);
    let runtime =
        Arc::new(build_chain_runtime_state(chain, SR, &[target], registry).expect("runtime"));
    let mut out_collected: Vec<f32> = Vec::with_capacity(sig.len());
    for chunk in sig.chunks(buffer) {
        process_input_f32(&runtime, 0, chunk, 1);
        let mut out = vec![0.0_f32; chunk.len() * 2];
        process_output_f32(&runtime, 0, &mut out, 2);
        for f in out.chunks_exact(2) {
            out_collected.push((f[0] + f[1]) * 0.5);
        }
    }
    // Issue #496 measurement-bug fix: truncate the tail to an exact
    // integer number of fundamental cycles BEFORE the FFT. Zero-padding
    // a non-periodic window injects spectral leakage that an earlier
    // version of this helper counted as engine-side noise, producing
    // false THD+N values of -13 dB on a path that is in fact bit-exact
    // after fade-in (verified by `diag_multi_callback_bit_exact_*`).
    let skip = SR as usize;
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

fn ac_rms_for_dc(chain: &Chain, registry: &[IoBinding], dc: f32, buffer: usize) -> f32 {
    let runtime = build_runtime(chain, registry);
    let sig = vec![dc; (SR as usize) * 2];
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
    let tail = &out_collected[skip..];
    let mean = tail.iter().sum::<f32>() / tail.len() as f32;
    (tail.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / tail.len() as f32).sqrt()
}

fn silent_residue(chain: &Chain, registry: &[IoBinding], buffer: usize) -> f32 {
    let runtime = build_runtime(chain, registry);
    let sig = vec![0.0_f32; (SR as usize) * 2];
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
    let tail = &out_collected[skip..];
    tail.iter().fold(0.0_f32, |a, &b| a.max(b.abs()))
}

fn lufs_delta_through_chain(chain: &Chain, registry: &[IoBinding], buffer: usize) -> f64 {
    use ebur128::{EbuR128, Mode};
    let target = DEFAULT_ELASTIC_TARGET.max(buffer);
    let runtime =
        Arc::new(build_chain_runtime_state(chain, SR, &[target], registry).expect("runtime"));
    let pink = pink_noise(SR as usize * 3, 0xDEAD_BEEF);
    let mut out_collected: Vec<f32> = Vec::with_capacity(pink.len());
    for chunk in pink.chunks(buffer) {
        process_input_f32(&runtime, 0, chunk, 1);
        let mut out = vec![0.0_f32; chunk.len() * 2];
        process_output_f32(&runtime, 0, &mut out, 2);
        for f in out.chunks_exact(2) {
            out_collected.push((f[0] + f[1]) * 0.5);
        }
    }
    let skip = SR as usize;
    let mut m_in = EbuR128::new(1, SR as u32, Mode::I).unwrap();
    m_in.add_frames_f32(&pink[skip..]).unwrap();
    let mut m_out = EbuR128::new(1, SR as u32, Mode::I).unwrap();
    m_out.add_frames_f32(&out_collected[skip..]).unwrap();
    m_out.loudness_global().unwrap() - m_in.loudness_global().unwrap()
}


fn sine_2s(freq: f32, amp: f32) -> Vec<f32> {
    (0..(SR as usize) * 2)
        .map(|i| amp * (2.0 * std::f32::consts::PI * freq * i as f32 / SR).sin())
        .collect()
}

// ── M.1 THD+N across buffer sizes (10 tests, 1 kHz @ 0.5) ───────
macro_rules! buf_thd_test {
    ($name:ident, $buf:expr) => {
        #[test]
        fn $name() {
            let (chain, registry) = bare_chain_for(stringify!($name));
            let sig = sine_2s(1_000.0, 0.5);
            let thd = thd_n_db_through_chain(&chain, &registry, &sig, $buf);
            eprintln!("[buffer={}] THD+N = {thd:.2} dB", $buf);
            assert!(thd < -60.0, "buffer={} THD+N {thd:.2} dB ≥ -60", $buf);
        }
    };
}
buf_thd_test!(m01_buf_64, 64);
buf_thd_test!(m02_buf_128, 128);
buf_thd_test!(m03_buf_192, 192);
buf_thd_test!(m04_buf_256, 256);
buf_thd_test!(m05_buf_384, 384);
buf_thd_test!(m06_buf_512, 512);
buf_thd_test!(m07_buf_768, 768);
buf_thd_test!(m08_buf_1024, 1024);
buf_thd_test!(m09_buf_1536, 1536);
buf_thd_test!(m10_buf_2048, 2048);

// ── M.2 THD+N across signal LEVELS at 512-frame buffer (5 tests) ──
macro_rules! lvl_thd_test {
    ($name:ident, $lvl:expr) => {
        #[test]
        fn $name() {
            let (chain, registry) = bare_chain_for(stringify!($name));
            let sig = sine_2s(1_000.0, $lvl);
            let thd = thd_n_db_through_chain(&chain, &registry, &sig, 512);
            eprintln!("[level={}] THD+N = {thd:.2} dB", $lvl);
            assert!(thd < -60.0, "level={} THD+N {thd:.2} dB ≥ -60", $lvl);
        }
    };
}
lvl_thd_test!(m11_level_0_1, 0.1);
lvl_thd_test!(m12_level_0_3, 0.3);
lvl_thd_test!(m13_level_0_5, 0.5);
lvl_thd_test!(m14_level_0_7, 0.7);
lvl_thd_test!(m15_level_0_9, 0.9);

// ── M.3 THD+N across FREQUENCIES at 512-frame buffer (5 tests) ────
macro_rules! freq_thd_test {
    ($name:ident, $f:expr) => {
        #[test]
        fn $name() {
            let (chain, registry) = bare_chain_for(stringify!($name));
            let sig = sine_2s($f, 0.5);
            let thd = thd_n_db_at_freq_through_chain(&chain, &registry, &sig, 512, $f);
            eprintln!("[freq={} Hz] THD+N = {thd:.2} dB", $f);
            assert!(thd < -60.0, "freq={} Hz THD+N {thd:.2} dB ≥ -60", $f);
        }
    };
}
freq_thd_test!(m16_freq_100, 100.0);
// Issue #496: use freqs with integer-cycle period at 48 kHz to avoid
// FFT leakage (220/440 Hz period is ~218/109 samples — non-integer).
freq_thd_test!(m17_freq_200, 200.0); // period = 240
freq_thd_test!(m18_freq_480, 480.0); // period = 100
freq_thd_test!(m19_freq_1000, 1_000.0);
freq_thd_test!(m20_freq_4000, 4_000.0);

// ── M.4 DC injection produces AC noise (5 tests) ────────────────
macro_rules! dc_ac_test {
    ($name:ident, $dc:expr) => {
        #[test]
        fn $name() {
            let (chain, registry) = bare_chain_for(stringify!($name));
            let ac = ac_rms_for_dc(&chain, &registry, $dc, 512);
            eprintln!("[DC={}] AC rms out = {ac:.6e}", $dc);
            assert!(
                ac < 5e-4,
                "DC={} produced AC rms {ac:.6e} (>-66 dBFS = audible hiss)",
                $dc
            );
        }
    };
}
dc_ac_test!(m21_dc_0_1, 0.1_f32);
dc_ac_test!(m22_dc_0_3, 0.3_f32);
dc_ac_test!(m23_dc_0_5, 0.5_f32);
dc_ac_test!(m24_dc_0_7, 0.7_f32);
dc_ac_test!(m25_dc_neg_0_3, -0.3_f32);

// ── M.5 Silent input → silent output across buffer sizes (5) ────
macro_rules! silent_test {
    ($name:ident, $buf:expr) => {
        #[test]
        fn $name() {
            let (chain, registry) = bare_chain_for(stringify!($name));
            let peak = silent_residue(&chain, &registry, $buf);
            eprintln!("[silent buf={}] peak = {peak:.6}", $buf);
            assert!(peak < 1e-6, "silent buf={} produced peak {peak:.6}", $buf);
        }
    };
}
silent_test!(m26_silent_buf_128, 128);
silent_test!(m27_silent_buf_256, 256);
silent_test!(m28_silent_buf_512, 512);
silent_test!(m29_silent_buf_1024, 1024);
silent_test!(m30_silent_buf_2048, 2048);

// ── M.6 LUFS preservation across buffer sizes (5) ───────────────
macro_rules! lufs_test {
    ($name:ident, $buf:expr) => {
        #[test]
        fn $name() {
            let (chain, registry) = bare_chain_for(stringify!($name));
            let d = lufs_delta_through_chain(&chain, &registry, $buf);
            eprintln!("[lufs buf={}] delta = {d:+.2} dB", $buf);
            assert!(d.abs() < 1.0, "lufs buf={} delta {d:+.2} dB", $buf);
        }
    };
}
lufs_test!(m31_lufs_buf_128, 128);
lufs_test!(m32_lufs_buf_256, 256);
lufs_test!(m33_lufs_buf_512, 512);
lufs_test!(m34_lufs_buf_1024, 1024);
lufs_test!(m35_lufs_buf_2048, 2048);

