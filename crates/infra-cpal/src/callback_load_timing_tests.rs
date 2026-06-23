//! Issue #670 — tests for the audio-callback deadline timing seam.

use super::{callback_period_ns, record_callback_deadline};
use domain::ids::{BlockId, ChainId, DeviceId};
use engine::runtime::{build_chain_runtime_state, ChainRuntimeState};
use project::block::{
    AudioBlock, AudioBlockKind, InputBlock, InputEntry, OutputBlock, OutputEntry,
};
use project::chain::{Chain, ChainInputMode, ChainOutputMode};
use std::sync::Arc;
use std::time::Duration;

fn pipe_runtime() -> Arc<ChainRuntimeState> {
    let chain = Chain {
        id: ChainId("t".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks: vec![
            AudioBlock {
                id: BlockId("in".into()),
                enabled: true,
                kind: AudioBlockKind::Input(InputBlock {
                    model: "standard".into(),
                    io: String::new(),
                    endpoint: String::new(),
                    entries: vec![InputEntry {
                        device_id: DeviceId("d".into()),
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
                    io: String::new(),
                    endpoint: String::new(),
                    entries: vec![OutputEntry {
                        device_id: DeviceId("d".into()),
                        mode: ChainOutputMode::Stereo,
                        channels: vec![0, 1],
                    }],
                }),
            },
        ],
    };
    Arc::new(build_chain_runtime_state(&chain, 48_000.0, &[256]).unwrap())
}

#[test]
fn callback_period_ns_is_frames_over_sample_rate() {
    assert_eq!(callback_period_ns(64, 48_000), 1_333_333);
    assert_eq!(callback_period_ns(128, 48_000), 2_666_666);
}

#[test]
fn callback_period_ns_guards_zero_inputs() {
    assert_eq!(callback_period_ns(0, 48_000), 0);
    assert_eq!(callback_period_ns(64, 0), 0);
}

#[test]
fn record_counts_overrun_when_callback_exceeds_deadline() {
    let rt = pipe_runtime();
    // 2 ms of DSP against a 64-frame @ 48k (1.33 ms) deadline = an xrun.
    record_callback_deadline(&rt, Duration::from_micros(2_000), 64, 48_000);
    assert_eq!(rt.xrun_count(), 1);
    assert!(rt.peak_callback_load() > 1.0);
}

#[test]
fn record_no_xrun_when_callback_under_budget() {
    let rt = pipe_runtime();
    record_callback_deadline(&rt, Duration::from_micros(500), 64, 48_000);
    assert_eq!(rt.xrun_count(), 0);
}

#[test]
fn record_with_zero_frames_is_ignored() {
    let rt = pipe_runtime();
    record_callback_deadline(&rt, Duration::from_micros(2_000), 0, 48_000);
    assert_eq!(rt.xrun_count(), 0);
}
