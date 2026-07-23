//! Issue #670 — audio-thread load / xrun counter tests.
//!
//! The user hears crackle at buffer 64 with a heavy rig. The root cause
//! is a DEADLINE OVERRUN (xrun): the per-buffer DSP cost exceeds the
//! buffer period, the output callback misses its deadline, and the
//! dropout is heard as crackle. Today that overrun is SILENT — nothing
//! counts it, nothing surfaces it.
//!
//! This module pins the engine-side counter the cpal / JACK output
//! callback feeds: `record_callback_load(elapsed_ns, period_ns)` is
//! RT-safe (integer math + two relaxed atomics, no alloc / lock / syscall)
//! and the GUI / MCP / gRPC read `xrun_count()` + `peak_callback_load()`
//! to surface the overload instead of letting it crackle unexplained.

use crate::runtime::{build_chain_runtime_state, process_output_f32, DEFAULT_ELASTIC_TARGET};
use crate::runtime_state::ChainRuntimeState;
use domain::ids::{ChainId, DeviceId};
use project::chain::Chain;
use std::sync::Arc;

/// A minimal pipe chain — the load counter is independent of chain
/// content, so the cheapest buildable runtime is enough.
fn pipe_runtime() -> Arc<ChainRuntimeState> {
    let registry = vec![domain::io_binding::IoBinding {
        id: "io".into(),
        name: "IO".into(),
        inputs: vec![domain::io_binding::IoEndpoint {
            name: "in0".into(),
            device_id: DeviceId("dev".into()),
            mode: domain::io_binding::ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs: vec![domain::io_binding::IoEndpoint {
            name: "out0".into(),
            device_id: DeviceId("dev".into()),
            mode: domain::io_binding::ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    }];
    let chain = Chain {
        id: ChainId("load-test".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks: vec![],
        di_output: None,
    };
    Arc::new(
        build_chain_runtime_state(&chain, 48_000.0, &[DEFAULT_ELASTIC_TARGET], &registry)
            .expect("pipe runtime must build"),
    )
}

#[test]
fn fresh_runtime_has_zero_xruns_and_zero_load() {
    let rt = pipe_runtime();
    assert_eq!(rt.xrun_count(), 0);
    assert_eq!(rt.peak_callback_load(), 0.0);
}

#[test]
fn records_an_xrun_when_a_callback_overruns_its_period() {
    let rt = pipe_runtime();
    // A callback that took 2 ms against a 1 ms buffer period overran.
    rt.record_callback_load(2_000_000, 1_000_000);
    assert_eq!(
        rt.xrun_count(),
        1,
        "an overrunning callback must count one xrun"
    );
    assert!(
        (rt.peak_callback_load() - 2.0).abs() < 0.01,
        "peak load must be elapsed/period = 2.0, got {}",
        rt.peak_callback_load()
    );
}

#[test]
fn a_callback_under_budget_is_not_an_xrun() {
    let rt = pipe_runtime();
    rt.record_callback_load(500_000, 1_000_000);
    assert_eq!(
        rt.xrun_count(),
        0,
        "a 50%-of-budget callback is not an xrun"
    );
    assert!(
        (rt.peak_callback_load() - 0.5).abs() < 0.01,
        "peak load must be 0.5, got {}",
        rt.peak_callback_load()
    );
}

#[test]
fn a_callback_exactly_at_the_deadline_is_not_an_xrun() {
    let rt = pipe_runtime();
    rt.record_callback_load(1_000_000, 1_000_000);
    assert_eq!(
        rt.xrun_count(),
        0,
        "exactly meeting the deadline is on-time, not an overrun"
    );
}

#[test]
fn peak_load_keeps_the_worst_callback_not_the_last() {
    let rt = pipe_runtime();
    rt.record_callback_load(900_000, 1_000_000); // 0.9
    rt.record_callback_load(300_000, 1_000_000); // 0.3 — must not lower the peak
    assert!(
        (rt.peak_callback_load() - 0.9).abs() < 0.01,
        "peak must stay at the worst (0.9), got {}",
        rt.peak_callback_load()
    );
}

#[test]
fn multiple_overruns_accumulate() {
    let rt = pipe_runtime();
    for _ in 0..5 {
        rt.record_callback_load(1_500_000, 1_000_000);
    }
    assert_eq!(rt.xrun_count(), 5);
}

#[test]
fn reset_load_stats_clears_count_and_peak() {
    let rt = pipe_runtime();
    rt.record_callback_load(2_000_000, 1_000_000);
    rt.reset_load_stats();
    assert_eq!(rt.xrun_count(), 0);
    assert_eq!(rt.peak_callback_load(), 0.0);
}

#[test]
fn a_zero_period_callback_is_ignored_without_panic() {
    let rt = pipe_runtime();
    rt.record_callback_load(100_000, 0);
    assert_eq!(rt.xrun_count(), 0);
    assert_eq!(rt.peak_callback_load(), 0.0);
}

#[test]
fn underrun_count_starts_at_zero() {
    let rt = pipe_runtime();
    assert_eq!(rt.underrun_count(), 0);
}

#[test]
fn underrun_count_rises_when_output_drains_an_empty_elastic_buffer() {
    // The crackle the user hears on a light single chain at buffer 64 is an
    // elastic-buffer STARVE, not a CPU overrun: the output callback pops a
    // frame the input/DSP producer hasn't delivered yet. Pin that the
    // instrumentation counts exactly this — drain the output with no input
    // fed, so every pop underruns.
    let rt = pipe_runtime();
    assert_eq!(rt.underrun_count(), 0);
    let mut out = vec![0.0f32; 64 * 2];
    process_output_f32(&rt, 0, &mut out, 2);
    assert!(
        rt.underrun_count() > 0,
        "draining an empty elastic buffer must register underruns so the \
         GUI/log can distinguish a starve from a CPU xrun"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// Worker-load vs callback-load semantics, ring drops, and deadline boundary.
// The macOS DSP runs on a per-stream worker (#670/#698): a LATE worker buffer
// is absorbed by the ring + elastic (NOT an xrun); a ring OVERFLOW drop IS a
// gap (an xrun). These pin that distinction so a refactor cannot silently
// conflate or drop a counter.
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn worker_load_overrun_does_not_count_as_xrun() {
    let rt = pipe_runtime();
    // The worker took 2x its period — late, but the ring/elastic absorb it.
    rt.record_worker_load(2_000_000, 1_000_000);
    assert_eq!(
        rt.xrun_count(),
        0,
        "a late worker buffer is absorbed by the ring/elastic — NOT an xrun"
    );
    assert!(
        (rt.peak_callback_load() - 2.0).abs() < 0.01,
        "but the lateness is still visible on the load meter, got {}",
        rt.peak_callback_load()
    );
}

#[test]
fn worker_load_peak_is_monotone_keeps_worst() {
    let rt = pipe_runtime();
    rt.record_worker_load(500_000, 1_000_000); // 0.5
    rt.record_worker_load(300_000, 1_000_000); // 0.3 — must not lower the peak
    assert!((rt.peak_callback_load() - 0.5).abs() < 0.01);
    rt.record_worker_load(1_200_000, 1_000_000); // 1.2 — new worst
    assert!((rt.peak_callback_load() - 1.2).abs() < 0.01);
}

#[test]
fn dropped_ring_buffer_counts_as_one_xrun_each() {
    let rt = pipe_runtime();
    assert_eq!(rt.xrun_count(), 0);
    rt.record_dropped_buffer();
    assert_eq!(
        rt.xrun_count(),
        1,
        "a dropped input buffer is an audible gap"
    );
    rt.record_dropped_buffer();
    rt.record_dropped_buffer();
    assert_eq!(rt.xrun_count(), 3, "drops accumulate");
    // A ring drop is an xrun, not an elastic underrun — distinct counters.
    assert_eq!(rt.underrun_count(), 0);
}

#[test]
fn exact_deadline_is_not_an_xrun_but_one_ns_over_is() {
    let rt = pipe_runtime();
    rt.record_callback_load(1_000_000, 1_000_000); // exactly on time
    assert_eq!(rt.xrun_count(), 0, "meeting the deadline is on-time");
    rt.record_callback_load(1_000_001, 1_000_000); // 1 ns over
    assert_eq!(rt.xrun_count(), 1, "the smallest overrun is still an xrun");
}

#[test]
fn peak_load_matches_elapsed_over_period_for_a_typical_buffer() {
    let rt = pipe_runtime();
    let period_ns = 64 * 1_000_000_000 / 48_000; // 64 frames @ 48 kHz ≈ 1333 us
    rt.record_callback_load(2_000_000, period_ns); // 2 ms callback
    let expected = 2_000_000.0 / period_ns as f32;
    assert!(
        (rt.peak_callback_load() - expected).abs() < 0.01,
        "peak load {} should equal elapsed/period {expected}",
        rt.peak_callback_load()
    );
}

#[test]
fn extreme_overload_does_not_panic_and_stays_finite() {
    let rt = pipe_runtime();
    // Pathological inputs (cannot happen in production — period comes from the
    // buffer size) must not panic and must keep a finite, >1.0 load.
    rt.record_callback_load(u64::MAX / 2, 1);
    let peak = rt.peak_callback_load();
    assert!(
        peak.is_finite(),
        "extreme overload must not produce NaN/Inf"
    );
    assert!(peak > 1.0, "extreme overload reads as an overload");
}
