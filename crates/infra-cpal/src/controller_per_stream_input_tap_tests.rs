//! RED tests for issue #557: per-stream INPUT meter and tuner silent
//! (or wrong source) on multi-channel mono chains.
//!
//! Setup mirrors the user's "GUITARRA - DEFAULT" project: one chain
//! with a single `InputBlock` in `ChainInputMode::Mono` and
//! `channels: vec![0, 1]` — i.e. two guitars on the same physical
//! device, two streams hosted by ONE per-input runtime keyed
//! `(chain_id, 0)`. `stream_count(cid)` reports `2` because the
//! runtime's internal segment count is `2`.
//!
//! Under that shape the GUI calls
//! `controller.subscribe_input_tap(cid, i, …)` for `i in 0..2`. The
//! current `controller_taps.rs` impl:
//!
//! 1. Looks up `(cid, i)` in `runtime_graph.chains`. For `i == 1`
//!    the lookup MISSES (only `(cid, 0)` was inserted) and falls back
//!    to `runtime_for_chain(cid)` — the very same runtime that hosts
//!    stream 0.
//! 2. Calls `runtime.subscribe_input_tap(i, …)` on that runtime,
//!    creating an `InputTap` whose `input_index == 1`.
//! 3. The runtime's cpal callback fires `process_input_f32(rt, 0, …)`
//!    because the device was assigned group 0 by `stream_builder.rs`
//!    (one cpal stream per unique device). The tap filter at
//!    `runtime.rs:150` (`if tap.input_index != input_index { continue }`)
//!    skips this tap forever → stream-1 ring stays silent.
//!
//! And independently, the channels-side bug: the GUI asks
//! `subscribe_input_tap(cid, i, total_channels=1, &[0], …)` — always
//! `&[0]` regardless of which device channel the input endpoint is
//! actually wired to (`meter_wiring.rs:278`). That is why the user's
//! "chain wired to ch2 only" screenshot shows the meter reacting to
//! ch1 instead — the tap consumes channel 0 of the interleaved frame,
//! not the endpoint's real channel.
//!
//! These tests pin both invariants. They MUST be RED before any
//! production edit; after the fix lands they must go GREEN without
//! regressing the existing meter / isolation tests.

#![cfg(test)]

use std::sync::Arc;

use domain::ids::{BlockId, ChainId, DeviceId};
use project::block::{
    AudioBlock, AudioBlockKind, InputBlock, InputEntry, OutputBlock, OutputEntry,
};
use project::chain::{Chain, ChainInputMode, ChainOutputMode};

use engine::runtime::{build_chain_runtime_state, process_input_f32, RuntimeGraph};

use super::ProjectRuntimeController;

/// Two-stream chain: one `InputBlock` in `Mono` mode with channels
/// `[0, 1]` (two guitars on the same Scarlett 2i2) routed to a stereo
/// output. Mirrors the project the user reproduced the bug on.
fn two_stream_mono_chain(id: &str, input_device: &str, output_device: &str) -> Chain {
    Chain {
        id: ChainId(id.into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        blocks: vec![
            AudioBlock {
                id: BlockId("input:0".into()),
                enabled: true,
                kind: AudioBlockKind::Input(InputBlock {
                    model: "standard".into(),
                    entries: vec![InputEntry {
                        device_id: DeviceId(input_device.into()),
                        mode: ChainInputMode::Mono,
                        channels: vec![0, 1],
                    }],
                }),
            },
            AudioBlock {
                id: BlockId("output:0".into()),
                enabled: true,
                kind: AudioBlockKind::Output(OutputBlock {
                    model: "standard".into(),
                    entries: vec![OutputEntry {
                        device_id: DeviceId(output_device.into()),
                        mode: ChainOutputMode::Stereo,
                        channels: vec![0, 1],
                    }],
                }),
            },
        ],
    }
}

/// Single-stream chain wired exclusively to device channel 1 (NOT 0).
/// Mirrors the user's "unplug one guitar, leave the other on ch2"
/// screenshot.
fn single_stream_on_channel_one(id: &str, input_device: &str, output_device: &str) -> Chain {
    Chain {
        id: ChainId(id.into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        blocks: vec![
            AudioBlock {
                id: BlockId("input:0".into()),
                enabled: true,
                kind: AudioBlockKind::Input(InputBlock {
                    model: "standard".into(),
                    entries: vec![InputEntry {
                        device_id: DeviceId(input_device.into()),
                        mode: ChainInputMode::Mono,
                        channels: vec![1],
                    }],
                }),
            },
            AudioBlock {
                id: BlockId("output:0".into()),
                enabled: true,
                kind: AudioBlockKind::Output(OutputBlock {
                    model: "standard".into(),
                    entries: vec![OutputEntry {
                        device_id: DeviceId(output_device.into()),
                        mode: ChainOutputMode::Stereo,
                        channels: vec![0, 1],
                    }],
                }),
            },
        ],
    }
}

fn controller_with_single_runtime(chain: &Chain) -> (ProjectRuntimeController, Arc<engine::runtime::ChainRuntimeState>) {
    let runtime = Arc::new(
        build_chain_runtime_state(chain, 48_000.0, &[256])
            .expect("two-stream mono chain must build a runtime"),
    );
    let mut graph = RuntimeGraph {
        chains: std::collections::HashMap::new(),
    };
    graph.chains.insert((chain.id.clone(), 0), Arc::clone(&runtime));
    let controller = ProjectRuntimeController {
        runtime_graph: graph,
        active_chains: std::collections::HashMap::new(),
        #[cfg(all(target_os = "linux", feature = "jack"))]
        supervisor: super::jack_supervisor::JackSupervisor::new(
            super::jack_supervisor::LiveJackBackend::new(),
        ),
    };
    (controller, runtime)
}

/// Issue #557 bug A: subscribing the per-stream INPUT tap for global
/// stream index 1 must return a ring that receives samples from the
/// device's second channel when the runtime processes audio. Today
/// the tap is silent because the controller passes `1` as the
/// runtime-side `input_index` filter and the cpal callback for that
/// runtime always fires with the local cpal group index `0`.
#[test]
fn subscribe_input_tap_stream_one_must_receive_signal_when_runtime_processes_audio() {
    let chain = two_stream_mono_chain("rig:input-1", "scarlett", "monitor");
    let (controller, runtime) = controller_with_single_runtime(&chain);

    assert_eq!(
        controller.stream_count(&chain.id),
        2,
        "channels=[0,1] in Mono mode produces two parallel streams in one runtime"
    );

    // Mirror the GUI subscription for stream 1 with the channel mapping
    // the fix must honour: the SECOND stream's audio rides on device
    // channel 1. Tests for the existing hardcoded `&[0]` path live
    // alongside this one to keep the channel-routing bug pinned too.
    let rings_stream_1 = controller.subscribe_input_tap(
        &chain.id,
        /* input_index = */ 1,
        /* total_channels = */ 2,
        /* subscribed_channels = */ &[1],
        /* capacity_per_channel = */ 256,
    );
    assert_eq!(
        rings_stream_1.len(),
        1,
        "one ring per subscribed channel"
    );

    // Drive the runtime as the real cpal callback would: a single
    // device group (index 0) with interleaved stereo samples. Channel
    // 0 is silent; channel 1 carries +0.5 for every frame — that is
    // the second guitar's signal.
    let frames = 8usize;
    let samples: Vec<f32> = (0..frames).flat_map(|_| [0.0_f32, 0.5_f32]).collect();
    process_input_f32(&runtime, 0, &samples, 2);

    let mut got = Vec::new();
    while let Some(s) = rings_stream_1[0].pop() {
        got.push(s);
    }
    assert_eq!(
        got,
        vec![0.5_f32; frames],
        "stream 1's input tap MUST receive the second guitar's samples \
         (device channel 1). Today the ring is empty because the \
         controller wires the tap's `input_index` to the GLOBAL stream \
         index instead of translating it to the per-input runtime's \
         LOCAL cpal group index (always 0 for a single-device chain)."
    );
}

/// Issue #557 bug B: chain wired to device channel 1 (only). The
/// INPUT meter for the chain's single stream MUST sample channel 1,
/// NOT channel 0. Today `build_streams_from_taps` (GUI side) and the
/// controller together pin `subscribed_channels = &[0]`, so the meter
/// reads device ch0 regardless of the project's input endpoint — which
/// is why the user sees the meter lighting up from ch1 when only ch2
/// is wired (unplug guitar from ch1, tap the cable, meter reacts).
#[test]
fn subscribe_input_tap_must_honour_endpoint_channel_not_default_to_zero() {
    let chain = single_stream_on_channel_one("rig:ch1-only", "scarlett", "monitor");
    let (controller, runtime) = controller_with_single_runtime(&chain);

    assert_eq!(
        controller.stream_count(&chain.id),
        1,
        "one InputEntry with channels=[1] in Mono is one stream"
    );

    // The fix must surface the endpoint's real device channel(s) to the
    // tap subscription — either by computing it in the controller, or
    // via a new MeterTapApi method. Either way, the meter must end up
    // reading channel 1, not channel 0. We assert against the public
    // controller signature the GUI calls; for the existing wrong path
    // the GUI passes `&[0]` and the meter samples silence (or worse,
    // ch0 — the wrong source).
    let rings = controller.subscribe_input_tap(
        &chain.id,
        /* input_index = */ 0,
        /* total_channels = */ 2,
        /* subscribed_channels = */ &[1],
        /* capacity_per_channel = */ 256,
    );
    assert_eq!(rings.len(), 1, "one ring per subscribed channel");

    // Send stereo: channel 0 = +0.9 (a loud "wrong" signal from ch1
    // unplugged-but-cable-touched), channel 1 = -0.25 (the actual guitar
    // on ch2 — the only channel the project wired). A correct tap MUST
    // see -0.25 only.
    let frames = 4usize;
    let samples: Vec<f32> = (0..frames).flat_map(|_| [0.9_f32, -0.25_f32]).collect();
    process_input_f32(&runtime, 0, &samples, 2);

    let mut got = Vec::new();
    while let Some(s) = rings[0].pop() {
        got.push(s);
    }
    assert_eq!(
        got,
        vec![-0.25_f32; frames],
        "the chain wired to device ch1 MUST surface ch1 samples in its \
         meter ring; today the ring sees 0.9 (ch0) instead, which is \
         the user-visible 'meter responds to the wrong guitar' bug."
    );
}
