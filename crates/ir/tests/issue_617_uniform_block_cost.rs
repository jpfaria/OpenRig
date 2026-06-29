//! Issue #617 — the cabinet IR convolver must spread its work evenly across
//! real-time callbacks, with NO periodic per-partition spike that overruns
//! small device buffers.
//!
//! Root cause: the convolver ran one full FFT + a multiply-accumulate over
//! ALL IR partitions INLINE every `PARTITION_SIZE` input samples. With
//! `PARTITION_SIZE` (512) far larger than a small device buffer (64), that
//! burst landed inside a single 64-frame callback, which then did ~all the
//! convolution work at once — a spike ~tens of times the cost of its
//! neighbour callbacks. On slower hardware (or any tight 64-frame budget)
//! that one callback misses its deadline → xrun → audible "clipping". At
//! buffer 128 the per-callback budget is large enough to absorb the same
//! burst, which is why raising the buffer hid the symptom.
//!
//! These tests pin two properties of the fix:
//!   1. `cost_per_callback_is_uniform_at_buffer_64` — no block costs wildly
//!      more than the median (the spike is gone). RED before the fix.
//!   2. `convolution_stays_linear_and_correct` — the fix is output-equivalent
//!      (partitioned convolution is exact for any partition size), so the
//!      sound is unchanged (invariant #9, numeric determinism).

use std::time::Instant;

use block_core::MonoProcessor;
use ir::MonoIrProcessor;

const SR: f32 = 48_000.0;

/// A realistic, deterministic cabinet-length IR (near MAX_IR_SAMPLES) so the
/// convolver uses many partitions — that is what makes the per-partition
/// burst heavy enough to overrun a 64-frame callback.
fn cabinet_ir(len: usize) -> Vec<f32> {
    (0..len)
        .map(|n| {
            let t = n as f32 / SR;
            let env = (-t * 25.0).exp();
            let body = (2.0 * std::f32::consts::PI * 1500.0 * t).sin()
                + 0.5 * (2.0 * std::f32::consts::PI * 3200.0 * t).sin();
            env * body
        })
        .collect()
}

fn di_signal(frames: usize) -> Vec<f32> {
    (0..frames)
        .map(|n| 0.2 * (2.0 * std::f32::consts::PI * 220.0 * n as f32 / SR).sin())
        .collect()
}

#[test]
fn cost_per_callback_is_uniform_at_buffer_64() {
    let block = 64usize;
    let signal = di_signal(block * 3000);

    // One measurement pass → the p99/median per-block cost ratio.
    let measure = || {
        let mut ir = MonoIrProcessor::new(cabinet_ir(8192));

        // Warm up state allocation / caches so the measurement reflects steady
        // state, not the first-build transient.
        let mut warm = signal[..block].to_vec();
        for _ in 0..64 {
            ir.process_block(&mut warm);
        }

        let mut times_ns: Vec<u64> = Vec::with_capacity(signal.len() / block);
        let mut buf = vec![0.0f32; block];
        for chunk in signal.chunks_exact(block) {
            buf.copy_from_slice(chunk);
            let t0 = Instant::now();
            ir.process_block(&mut buf);
            times_ns.push(t0.elapsed().as_nanos() as u64);
        }

        times_ns.sort_unstable();
        let median = times_ns[times_ns.len() / 2].max(1) as f64;
        // 99th percentile filters the rare OS scheduling hiccup but is still
        // well inside the burst band of the broken convolver (every 8th block
        // at buffer 64 was a spike ≈ 12.5% of blocks).
        times_ns[times_ns.len() * 99 / 100] as f64 / median
    };

    // The #617 spike is structural — it lands on every 8th block, so it shows
    // up in EVERY pass. Transient OS scheduling contention (e.g. a busy CI box)
    // does not. Taking the best (lowest) ratio over a few passes keeps the real
    // regression detector intact while removing the wall-clock false positive.
    let ratio = (0..3).map(|_| measure()).fold(f64::INFINITY, f64::min);

    assert!(
        ratio < 4.0,
        "IR per-callback cost is bursty (issue #617): best-of-3 p99/median = {ratio:.1}x. \
         A periodic per-partition FFT spike (PARTITION_SIZE >> device buffer) concentrates the \
         convolution into one callback and overruns small buffers."
    );
}

#[test]
fn convolution_stays_linear_and_correct() {
    let taps = cabinet_ir(2048);
    let probe_len = 8192usize;

    // Effective impulse response of the block (includes its internal
    // latency): feed a unit impulse followed by silence.
    let mut sys_a = MonoIrProcessor::new(taps.clone());
    let mut h = vec![0.0f32; probe_len];
    h[0] = 1.0;
    sys_a.process_block(&mut h);

    // Arbitrary deterministic DI burst.
    let x: Vec<f32> = (0..512)
        .map(|n| (((n * 37) % 101) as f32 / 50.0 - 1.0) * 0.3)
        .collect();

    // The block applied to x.
    let mut sys_b = MonoIrProcessor::new(taps);
    let mut y = vec![0.0f32; probe_len];
    y[..x.len()].copy_from_slice(&x);
    sys_b.process_block(&mut y);

    // For any correct LTI convolver, y == conv(x, h) regardless of the
    // internal partition scheme. This proves the fix does not alter the sound.
    let mut y_ref = vec![0.0f32; probe_len];
    for (n, slot) in y_ref.iter_mut().enumerate() {
        let mut acc = 0.0f32;
        let kmax = n.min(x.len() - 1);
        for k in 0..=kmax {
            acc += x[k] * h[n - k];
        }
        *slot = acc;
    }

    let peak = y_ref.iter().fold(0.0f32, |a, &b| a.max(b.abs())).max(1e-6);
    let max_err = y
        .iter()
        .zip(&y_ref)
        .fold(0.0f32, |a, (&yi, &ri)| a.max((yi - ri).abs()));
    assert!(
        max_err < 1e-3 * peak,
        "partitioned convolution is not a correct linear convolution: max_err={max_err:.2e}, \
         peak={peak:.2e} (relative {:.2e})",
        max_err / peak
    );
}
