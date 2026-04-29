//! Audio deadline / CPU-budget regression tests.
//!
//! THE PURPOSE: catch a "callback slower than its buffer period"
//! regression *before* the user hears it. This is the only test layer
//! that detects timing/CPU regressions — golden samples, volume
//! invariants and stream isolation are all numerical checks, they pass
//! whether the code runs in 1 ms or 1 s. The slice 1 / inline-loss
//! regression on 2026-04-29 was a 100% pure timing regression: math
//! identical, but per-frame helpers crossed a module boundary, lost
//! same-module inlining, blew the 64-frame buffer budget on the user's
//! Mac. No numerical test could catch it. This one would have.
//!
//! HOW IT WORKS:
//!   - Build a runtime offline.
//!   - Call `process_input_f32` + `process_output_f32` in a tight loop,
//!     N iterations.
//!   - Measure wall-clock ns per iteration with `Instant::now()`.
//!   - Assert two thresholds:
//!       * overrun rate (iterations exceeding the buffer period)
//!         is below `MAX_OVERRUN_RATIO`.
//!       * median ns/iteration is below
//!         `MAX_MEDIAN_FRACTION` × period.
//!
//! GATING: `#[cfg_attr(debug_assertions, ignore = "...")]`.
//! Debug builds skip — they're too slow for any meaningful timing
//! threshold (every method call is a real call, no inlining). To run
//! the suite use:
//!     cargo test -p engine --release --lib audio_deadline
//!
//! THRESHOLDS:
//!   - `MAX_OVERRUN_RATIO = 0.005` (0.5%) — at 5000 iterations that's
//!     up to 25 buffers over budget. Tolerance for OS scheduler jitter
//!     (other processes preempting briefly).
//!   - `MAX_MEDIAN_FRACTION = 0.5` (50%) — median per-buffer must be
//!     at most half the buffer period. Two-times margin so the test
//!     stays meaningful across machines (the user's Mac is the slowest
//!     dev box; CI / Orange Pi need their own calibration).
//!
//! IF YOU REGRESS THIS:
//!   - The most recent commit is the suspect. Read its diff against
//!     the previous green build and look for hot-path code that
//!     crossed a module boundary, lost an `#[inline]` attribute, or
//!     introduced an allocation/lock inside a per-sample / per-frame
//!     helper. Fix the source — never relax the assertions.
//!
//! HONEST LIMITATIONS — do not over-trust this test:
//!   - Offline measurement: this loop runs `process_input_f32` +
//!     `process_output_f32` directly, in the test thread. It does not
//!     reproduce the real audio backend (CoreAudio / JACK) callback
//!     thread, its scheduling priority, or the cache pressure of a
//!     full app running. A regression that sneaks past these tests
//!     can still cause audible glitches in the real callback.
//!   - Same-crate inlining: rustc is aggressive enough in release that
//!     even an unmarked `fn` in this crate often gets inlined into
//!     callers. So a regression that strips an `#[inline]` won't
//!     consistently fail this test on every machine — sometimes the
//!     compiler still does the right thing. The test catches the case
//!     where the lost inlining ALSO blows the budget; it doesn't
//!     guard the inlining itself.
//!   - Single-threaded: no contention from other threads, no GUI work,
//!     no allocator pressure. Real audio thread runs alongside Slint
//!     re-renders, file I/O on saves, plugin GUI updates.
//!
//! WHAT THIS TEST IS:
//!   - A first numerical layer that catches the obvious cliff —
//!     allocations on hot path, locks held per sample, syscalls in
//!     the callback, large amounts of work moved into per-frame fns.
//!   - A baseline for refactor regressions: if you see p50/p99/max
//!     numbers materially worse than the comments here, your refactor
//!     pushed work into the hot path. Investigate before merging,
//!     even if assertions still pass.
//!
//! WHAT THIS TEST IS NOT:
//!   - A substitute for audible A/B validation on real hardware. The
//!     CLAUDE.md non-regression checklist still applies. This test
//!     can pass and the user can still hear glitches under load.
//!
//! WAYS TO HARDEN (future work, not blocking):
//!   - Add a real-audio-callback test that opens a CPAL stream against
//!     a null sink and measures xruns directly.
//!   - Add a cache-pressure variant (large project, many chains, GUI
//!     thread doing work in parallel) to stress the realistic profile.
//!   - Soak test of 5-10 min processing in a background CI job, not
//!     blocking PR but flagging long-running CPU drift.

use super::{
    build_chain_runtime_state, process_input_f32, process_output_f32, DEFAULT_ELASTIC_TARGET,
};
use domain::ids::{BlockId, ChainId, DeviceId};
use project::block::{AudioBlock, AudioBlockKind, InputBlock, InputEntry, OutputBlock, OutputEntry};
use project::chain::{Chain, ChainInputMode, ChainOutputMode};
use std::sync::Arc;
use std::time::Instant;

const N_ITERATIONS: usize = 5_000;
/// Zero buffers out of N may exceed the period. In production every overrun
/// means an underrun → audible click; "0.5% acceptable" is engineer-think,
/// not user-think. Empirically the inline-fixed slice 1 build runs 5_000
/// iterations on Mac release with zero overruns and max ≤ 50 µs against a
/// 1.45 ms period — three orders of magnitude headroom. If a refactor
/// breaks this, audible glitches are imminent.
const MAX_OVERRUNS: usize = 0;
/// p99 latency caps the bad-tail behaviour. The inline-stripped slice 1
/// version showed p99 ≈ 1-3 µs but max bursts hitting 531-1887 µs — the
/// median test missed those. Capping p99 at 25 % of the buffer period
/// catches occasional spikes that a median can hide. 25 % is loose but
/// hard-line: anything that bursts that high is definitely going to start
/// dropping buffers under real load.
const MAX_P99_FRACTION: f64 = 0.25;
/// Median must stay tiny; this catches a sustained CPU regression. 5 % is
/// already 50× the current numbers (≈ 0.1 % of period) so a 50× regression
/// trips this without false-failing on machine variance.
const MAX_MEDIAN_FRACTION: f64 = 0.05;

// ─────────────────────────────────────────────────────────────────────────
// Chain builders (small, copied from volume_invariants_tests.rs to keep
// each test file self-contained).
// ─────────────────────────────────────────────────────────────────────────

fn input_mono(channels: Vec<usize>) -> AudioBlock {
    AudioBlock {
        id: BlockId("input:0".into()),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".into(),
            entries: vec![InputEntry {
                device_id: DeviceId("dev".into()),
                mode: ChainInputMode::Mono,
                channels,
            }],
        }),
    }
}

fn input_stereo(channels: Vec<usize>) -> AudioBlock {
    AudioBlock {
        id: BlockId("input:0".into()),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".into(),
            entries: vec![InputEntry {
                device_id: DeviceId("dev".into()),
                mode: ChainInputMode::Stereo,
                channels,
            }],
        }),
    }
}

fn output(mode: ChainOutputMode, channels: Vec<usize>) -> AudioBlock {
    AudioBlock {
        id: BlockId("output:0".into()),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".into(),
            entries: vec![OutputEntry {
                device_id: DeviceId("dev".into()),
                mode,
                channels,
            }],
        }),
    }
}

fn chain_with_blocks(id: &str, blocks: Vec<AudioBlock>) -> Chain {
    Chain {
        id: ChainId(id.into()),
        description: Some("deadline test".into()),
        instrument: "electric_guitar".into(),
        enabled: true,
        blocks,
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Deadline harness
// ─────────────────────────────────────────────────────────────────────────

struct DeadlineResult {
    label: &'static str,
    iterations: usize,
    overruns: usize,
    period_ns: u128,
    p50_ns: u128,
    p95_ns: u128,
    p99_ns: u128,
    max_ns: u128,
}

impl DeadlineResult {
    fn print(&self) {
        eprintln!(
            "[deadline] {:<32} iter={:>5} period={:>5}us  p50={:>5}us ({:>4.1}%)  p95={:>5}us  p99={:>5}us  max={:>5}us  overruns={:>3} ({:.3}%)",
            self.label,
            self.iterations,
            self.period_ns / 1_000,
            self.p50_ns / 1_000,
            (self.p50_ns as f64 / self.period_ns as f64) * 100.0,
            self.p95_ns / 1_000,
            self.p99_ns / 1_000,
            self.max_ns / 1_000,
            self.overruns,
            (self.overruns as f64 / self.iterations as f64) * 100.0,
        );
    }

    fn assert_meets(&self) {
        let median_ratio = self.p50_ns as f64 / self.period_ns as f64;
        let p99_ratio = self.p99_ns as f64 / self.period_ns as f64;

        assert!(
            self.overruns <= MAX_OVERRUNS,
            "{}: {} buffers exceeded period (max allowed: {}). \
             period={}us, max observed={}us, p99={}us. \
             Every overrun is an audible click in production. The most recent commit \
             likely regressed the audio thread — check for hot-path helpers that \
             crossed module boundaries without #[inline], allocations in per-sample / \
             per-frame paths, or new locks / syscalls in the callback.",
            self.label,
            self.overruns,
            MAX_OVERRUNS,
            self.period_ns / 1_000,
            self.max_ns / 1_000,
            self.p99_ns / 1_000,
        );

        assert!(
            p99_ratio < MAX_P99_FRACTION,
            "{}: p99 = {}us is {:.1}% of buffer period {}us (max {:.0}%). \
             Tail latency regression — even though the median looks fine, occasional \
             spikes will cause clicks under real audio load.",
            self.label,
            self.p99_ns / 1_000,
            p99_ratio * 100.0,
            self.period_ns / 1_000,
            MAX_P99_FRACTION * 100.0,
        );

        assert!(
            median_ratio < MAX_MEDIAN_FRACTION,
            "{}: median = {}ns is {:.2}% of buffer period {}ns (max {:.0}%). \
             Sustained CPU regression — every callback now eats this much of its \
             budget. Will glitch the moment another thread competes for the core.",
            self.label,
            self.p50_ns,
            median_ratio * 100.0,
            self.period_ns,
            MAX_MEDIAN_FRACTION * 100.0,
        );
    }
}

fn run_deadline(
    label: &'static str,
    chain: &Chain,
    input_total_channels: usize,
    output_total_channels: usize,
    buffer_frames: usize,
    sample_rate_hz: u32,
    iterations: usize,
) -> DeadlineResult {
    let runtime = Arc::new(
        build_chain_runtime_state(
            chain,
            sample_rate_hz as f32,
            &[DEFAULT_ELASTIC_TARGET],
        )
        .expect("runtime should build for deadline test"),
    );

    let period_ns = (buffer_frames as u128 * 1_000_000_000) / sample_rate_hz as u128;
    let input_buf = vec![0.1_f32; buffer_frames * input_total_channels];
    let mut output_buf = vec![0.0_f32; buffer_frames * output_total_channels];

    // Warm-up: a few iterations so caches/branch predictors stabilize and the
    // FADE_IN ramp completes. Not measured.
    for _ in 0..16 {
        process_input_f32(&runtime, 0, &input_buf, input_total_channels);
        process_output_f32(&runtime, 0, &mut output_buf, output_total_channels);
    }

    let mut elapsed: Vec<u128> = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let t0 = Instant::now();
        process_input_f32(&runtime, 0, &input_buf, input_total_channels);
        process_output_f32(&runtime, 0, &mut output_buf, output_total_channels);
        elapsed.push(t0.elapsed().as_nanos());
    }

    let overruns = elapsed.iter().filter(|&&t| t > period_ns).count();
    elapsed.sort_unstable();
    let p50 = elapsed[iterations / 2];
    let p95 = elapsed[(iterations * 95) / 100];
    let p99 = elapsed[(iterations * 99) / 100];
    let max = *elapsed.last().unwrap();

    let result = DeadlineResult {
        label,
        iterations,
        overruns,
        period_ns,
        p50_ns: p50,
        p95_ns: p95,
        p99_ns: p99,
        max_ns: max,
    };
    result.print();
    result
}

// ─────────────────────────────────────────────────────────────────────────
// Tests
//
// All tests are gated with #[cfg_attr(debug_assertions, ignore)] — they
// only run in release builds, where timing measurements are meaningful.
// In debug builds (no optimizations, no inlining) every method call is
// a real call, ~100x slower than release. The thresholds below assume
// release codegen.
//
// To run locally:
//     cargo test -p engine --release --lib audio_deadline
//
// To run including ignored debug tests (will fail by design — that's the
// point, debug build is not realtime):
//     cargo test -p engine --lib audio_deadline -- --include-ignored
// ─────────────────────────────────────────────────────────────────────────

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "deadline tests require --release for meaningful timing"
)]
fn pipe_only_mono_64_at_44100_meets_deadline() {
    // Pipe-only: input→output, no DSP block. Catches the slice 1 class
    // of regression directly — every per-frame helper (push, pop,
    // mono_mix, frame_to_bits, read_input_frame) runs every iteration.
    let chain = chain_with_blocks(
        "pipe-mono-64-44k",
        vec![
            input_mono(vec![0]),
            output(ChainOutputMode::Mono, vec![0]),
        ],
    );
    let result = run_deadline(
        "pipe_only_mono_64@44.1k",
        &chain,
        1,
        1,
        64,
        44_100,
        N_ITERATIONS,
    );
    result.assert_meets();
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "deadline tests require --release for meaningful timing"
)]
fn pipe_only_stereo_64_at_44100_meets_deadline() {
    // Stereo path stresses the bit-packed last_frame_bits in
    // ElasticBuffer plus the stereo branches in read_input_frame and
    // silent_frame.
    let chain = chain_with_blocks(
        "pipe-stereo-64-44k",
        vec![
            input_stereo(vec![0, 1]),
            output(ChainOutputMode::Stereo, vec![0, 1]),
        ],
    );
    let result = run_deadline(
        "pipe_only_stereo_64@44.1k",
        &chain,
        2,
        2,
        64,
        44_100,
        N_ITERATIONS,
    );
    result.assert_meets();
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "deadline tests require --release for meaningful timing"
)]
fn pipe_only_mono_128_at_48000_meets_deadline() {
    // Larger buffer + 48k. Period roughly 2.67 ms — gives more headroom,
    // a regression here means something quite expensive landed.
    let chain = chain_with_blocks(
        "pipe-mono-128-48k",
        vec![
            input_mono(vec![0]),
            output(ChainOutputMode::Mono, vec![0]),
        ],
    );
    let result = run_deadline(
        "pipe_only_mono_128@48k",
        &chain,
        1,
        1,
        128,
        48_000,
        N_ITERATIONS,
    );
    result.assert_meets();
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "deadline tests require --release for meaningful timing"
)]
fn pipe_only_mono_64_at_48000_meets_deadline() {
    // 64 frames @ 48k — period 1.333 ms, the tightest realistic budget
    // openrig has to hit on Mac/Linux at default settings.
    let chain = chain_with_blocks(
        "pipe-mono-64-48k",
        vec![
            input_mono(vec![0]),
            output(ChainOutputMode::Mono, vec![0]),
        ],
    );
    let result = run_deadline(
        "pipe_only_mono_64@48k",
        &chain,
        1,
        1,
        64,
        48_000,
        N_ITERATIONS,
    );
    result.assert_meets();
}
