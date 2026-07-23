//! Issue #670 — tests for the audio-callback deadline timing seam.

use super::{callback_period_ns, record_callback_deadline};
use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use engine::runtime::{build_chain_runtime_state, ChainRuntimeState};
use project::chain::Chain;
use std::sync::Arc;
use std::time::Duration;

fn pipe_runtime() -> Arc<ChainRuntimeState> {
    // Model A (#716): a mono-in/stereo-out passthrough; the endpoints come from
    // the "io" binding, not from block `entries`.
    let chain = Chain {
        id: ChainId("t".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks: vec![],
        di_output: None,
    };
    let registry = vec![IoBinding {
        id: "io".into(),
        name: "IO".into(),
        inputs: vec![IoEndpoint {
            name: "in0".into(),
            device_id: DeviceId("d".into()),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs: vec![IoEndpoint {
            name: "out0".into(),
            device_id: DeviceId("d".into()),
            mode: ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    }];
    Arc::new(build_chain_runtime_state(&chain, 48_000.0, &[256], &registry).unwrap())
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
