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

fn controller_with_single_runtime(
    chain: &Chain,
) -> (
    ProjectRuntimeController,
    Arc<engine::runtime::ChainRuntimeState>,
) {
    let runtime = Arc::new(
        build_chain_runtime_state(chain, 48_000.0, &[256])
            .expect("two-stream mono chain must build a runtime"),
    );
    let mut graph = RuntimeGraph {
        chains: std::collections::HashMap::new(),
    };
    graph
        .chains
        .insert((chain.id.clone(), 0), Arc::clone(&runtime));
    let controller = ProjectRuntimeController {
        runtime_graph: graph,
        active_chains: std::collections::HashMap::new(),
        chain_slots: std::collections::HashMap::new(),
        worker: crate::ControlWorker::new(),
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

    // Use the per-stream meter API (`subscribe_stream_input_tap`) the
    // GUI must call. It takes the GLOBAL stream index, walks the per-
    // input runtimes to find the one hosting the segment, and asks
    // that runtime for the segment's real cpal group index and device
    // channels before subscribing the tap.
    let ring_stream_1 = controller
        .subscribe_stream_input_tap(&chain.id, /* stream_index = */ 1, 256)
        .expect("stream 1 must yield a ring on a 2-stream chain");

    // Drive the runtime as the real cpal callback would: a single
    // device group (index 0) with interleaved stereo samples. Channel
    // 0 is silent; channel 1 carries +0.5 for every frame — that is
    // the second guitar's signal.
    let frames = 8usize;
    let samples: Vec<f32> = (0..frames).flat_map(|_| [0.0_f32, 0.5_f32]).collect();
    process_input_f32(&runtime, 0, &samples, 2);

    let mut got = Vec::new();
    while let Some(s) = ring_stream_1.pop() {
        got.push(s);
    }
    assert_eq!(
        got,
        vec![0.5_f32; frames],
        "stream 1's input meter MUST receive the second guitar's \
         samples (device channel 1). The pre-fix call pattern \
         (`subscribe_input_tap(cid, 1, 1, &[0], cap)`) returned an \
         always-empty ring: the controller's lookup fell back to \
         runtime 0 with `input_index=1`, which never matches the \
         cpal callback's `input_index=0`."
    );
}

/// Issue #557 — tuner side: the tuner subscribes via the lower-level
/// `subscribe_input_tap(cid, per_entry_counter, total_channels,
/// &entry.channels, cap)` because it needs multi-channel access for
/// stereo / dual-mono entries. For chains where multiple entries
/// share one per-input runtime (e.g. two mono guitars on the same
/// Scarlett, modeled as either one entry with `channels=[0,1]` or
/// two entries each with `channels=[X]`), the pre-fix lookup of
/// `(cid, 1)` MISSED and fell back to runtime 0 with the global
/// `input_index=1` filter — which the runtime's cpal callback for
/// group 0 never matches, so the tuner ring for the second guitar
/// stayed silent. The translation fix below mirrors
/// `subscribe_stream_tap`'s walk and resolves the local cpal group
/// index before calling the runtime, so tuner subscriptions for
/// every global index past 0 land on the right cpal callback.
#[test]
fn subscribe_input_tap_translates_global_index_to_local_cpal_group() {
    let chain = two_stream_mono_chain("rig:tuner", "scarlett", "monitor");
    let (controller, runtime) = controller_with_single_runtime(&chain);

    assert_eq!(controller.stream_count(&chain.id), 2);

    // Mimic the tuner's per-entry call shape for the SECOND stream.
    // The second stream's audio rides on device channel 1; total
    // channel count of the cpal callback is 2.
    let rings = controller.subscribe_input_tap(
        &chain.id,
        /* input_index = */ 1,
        /* total_channels = */ 2,
        /* subscribed_channels = */ &[1],
        /* capacity_per_channel = */ 256,
    );
    assert_eq!(rings.len(), 1, "one ring per subscribed channel");

    // Drive runtime as the cpal callback does: group 0, stereo, ch1 = -0.5.
    let frames = 6usize;
    let samples: Vec<f32> = (0..frames).flat_map(|_| [0.0_f32, -0.5_f32]).collect();
    process_input_f32(&runtime, 0, &samples, 2);

    let mut got = Vec::new();
    while let Some(s) = rings[0].pop() {
        got.push(s);
    }
    assert_eq!(
        got,
        vec![-0.5_f32; frames],
        "tuner's input tap for stream 1 MUST receive the second \
         guitar's ch1 samples; pre-fix the controller passed the \
         global input_index `1` straight through to the runtime, \
         and the runtime's cpal callback (input_index=0) never \
         matched the tap's filter."
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

    // The per-stream meter API must auto-resolve the endpoint's real
    // device channel — the GUI does NOT pass a channel index. Today
    // (before the fix) the GUI hardcoded `&[0]`, producing the user-
    // visible "meter responds to the wrong guitar" symptom; the new
    // method takes the chain's `InputEntry.channels` into account.
    let ring = controller
        .subscribe_stream_input_tap(&chain.id, /* stream_index = */ 0, 256)
        .expect("single-stream chain must yield a ring");

    // Send stereo: channel 0 = +0.9 (a loud "wrong" signal from ch1
    // unplugged-but-cable-touched), channel 1 = -0.25 (the actual guitar
    // on ch2 — the only channel the project wired). A correct tap MUST
    // see -0.25 only.
    let frames = 4usize;
    let samples: Vec<f32> = (0..frames).flat_map(|_| [0.9_f32, -0.25_f32]).collect();
    process_input_f32(&runtime, 0, &samples, 2);

    let mut got = Vec::new();
    while let Some(s) = ring.pop() {
        got.push(s);
    }
    assert_eq!(
        got,
        vec![-0.25_f32; frames],
        "the chain wired to device ch1 MUST surface ch1 samples in its \
         meter ring; pre-fix the ring read 0.9 (ch0) instead — that's \
         the screenshot the user posted."
    );
}
