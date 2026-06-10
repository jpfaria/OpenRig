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
use domain::ids::{BlockId, ChainId, DeviceId};
use project::block::{
    AudioBlock, AudioBlockKind, InputBlock, InputEntry, OutputBlock, OutputEntry,
};
use project::chain::{Chain, ChainInputMode, ChainOutputMode};
use std::sync::Arc;

/// A minimal pipe chain — the load counter is independent of chain
/// content, so the cheapest buildable runtime is enough.
fn pipe_runtime() -> Arc<ChainRuntimeState> {
    let chain = Chain {
        id: ChainId("load-test".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        blocks: vec![
            AudioBlock {
                id: BlockId("in".into()),
                enabled: true,
                kind: AudioBlockKind::Input(InputBlock {
                    model: "standard".into(),
                    entries: vec![InputEntry {
                        device_id: DeviceId("dev".into()),
                        mode: ChainInputMode::Mono,
                        channels: vec![0],
                    }],
                }),
            },
            AudioBlock {
                id: BlockId("out".into()),
                enabled: true,
                kind: AudioBlockKind::Output(OutputBlock {
                    model: "standard".into(),
                    entries: vec![OutputEntry {
                        device_id: DeviceId("dev".into()),
                        mode: ChainOutputMode::Stereo,
                        channels: vec![0, 1],
                    }],
                }),
            },
        ],
    };
    Arc::new(
        build_chain_runtime_state(&chain, 48_000.0, &[DEFAULT_ELASTIC_TARGET])
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
