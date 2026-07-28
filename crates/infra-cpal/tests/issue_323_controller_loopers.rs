//! Issue #323 — the controller's looper facade: ops reach every runtime of a
//! chain (each one records its OWN input — stream isolation), the published
//! status is readable without touching the audio thread, and retired layer
//! buffers are collected off it.

use std::collections::HashMap;
use std::sync::Arc;

use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use engine::runtime::{build_chain_runtime_state, RuntimeGraph};
use engine::{LooperOp, LooperState};
use infra_cpal::ProjectRuntimeController;
use project::chain::Chain;

const UID: u64 = 3;

fn chain_and_registry(id: &str) -> (Chain, Vec<IoBinding>) {
    let chain = Chain {
        id: ChainId(id.into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks: vec![],
        di_output: None,
        loopers: vec![],
    };
    let registry = vec![IoBinding {
        id: "io".into(),
        name: "IO".into(),
        inputs: vec![IoEndpoint {
            name: "in0".into(),
            device_id: DeviceId("dev".into()),
            mode: ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
        outputs: vec![IoEndpoint {
            name: "out0".into(),
            device_id: DeviceId("dev".into()),
            mode: ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    }];
    (chain, registry)
}

/// Controller holding one chain with `runtime_count` parallel runtimes — the
/// per-entry isolation of #703.
fn controller(id: &str, runtime_count: usize) -> (ProjectRuntimeController, ChainId) {
    let (chain, registry) = chain_and_registry(id);
    let mut chains = HashMap::new();
    for idx in 0..runtime_count {
        chains.insert(
            (chain.id.clone(), idx),
            Arc::new(
                build_chain_runtime_state(&chain, 48_000.0, &[256], &registry).expect("runtime"),
            ),
        );
    }
    let mut controller =
        ProjectRuntimeController::for_testing_with_sample_rate(RuntimeGraph { chains }, 48_000);
    controller.set_io_bindings(registry);
    (controller, chain.id)
}

/// Run one audio callback on every runtime of the chain, so queued ops are
/// applied and the status mirror is refreshed.
fn tick(controller: &ProjectRuntimeController, chain: &ChainId, level: f32) {
    let frames = 128usize;
    let input = vec![level; frames * 2];
    let mut output = vec![0.0f32; frames * 2];
    for runtime in controller.runtimes_for_chain(chain) {
        engine::runtime::process_input_f32(&runtime, 0, &input, 2);
        engine::runtime::process_output_f32(&runtime, 0, &mut output, 2);
    }
}

#[test]
fn create_reaches_every_runtime_of_the_chain() {
    let (controller, chain) = controller("looper-ctrl", 2);
    controller.push_chain_looper_op(&chain, |_| Some(LooperOp::Create { uid: UID, seg: 0 }));
    tick(&controller, &chain, 0.0);

    for runtime in controller.runtimes_for_chain(&chain) {
        assert!(
            runtime.looper_status(UID).is_some(),
            "every parallel runtime of the chain owns its own slot for the looper"
        );
    }
    assert_eq!(
        controller.chain_looper_status(&chain, UID).unwrap().state,
        LooperState::Empty
    );
}

#[test]
fn a_record_tap_carries_one_buffer_per_runtime() {
    let (controller, chain) = controller("looper-buffers", 2);
    controller.push_chain_looper_op(&chain, |_| Some(LooperOp::Create { uid: UID, seg: 0 }));
    controller.push_chain_looper_op(&chain, |runtime| {
        Some(LooperOp::TapRecord {
            uid: UID,
            // Each runtime gets ITS OWN buffer — sharing one would mean two
            // audio threads writing the same memory.
            buffer: Some(vec![0.0f32; runtime.looper_max_frames() * 2].into_boxed_slice()),
        })
    });
    tick(&controller, &chain, 0.5);

    for runtime in controller.runtimes_for_chain(&chain) {
        assert_eq!(
            runtime.looper_status(UID).unwrap().state,
            LooperState::Recording
        );
    }
}

#[test]
fn the_chain_status_reports_the_runtime_that_holds_material() {
    let (controller, chain) = controller("looper-status", 2);
    controller.push_chain_looper_op(&chain, |_| Some(LooperOp::Create { uid: UID, seg: 0 }));
    // Only the FIRST runtime is armed, as if only one input was recorded.
    let runtimes = controller.runtimes_for_chain(&chain);
    let first = runtimes.first().expect("one runtime").clone();
    first
        .push_looper_op(LooperOp::TapRecord {
            uid: UID,
            buffer: Some(vec![0.0f32; first.looper_max_frames() * 2].into_boxed_slice()),
        })
        .expect("queued");
    tick(&controller, &chain, 0.5);
    first
        .push_looper_op(LooperOp::TapRecord {
            uid: UID,
            buffer: None,
        })
        .expect("queued");
    tick(&controller, &chain, 0.0);

    let status = controller
        .chain_looper_status(&chain, UID)
        .expect("the chain reports a status");
    assert_eq!(status.state, LooperState::Playing);
    assert_eq!(status.len_frames, 128);
}

#[test]
fn retired_layers_are_collected_off_the_audio_thread() {
    let (controller, chain) = controller("looper-retire", 1);
    controller.push_chain_looper_op(&chain, |_| Some(LooperOp::Create { uid: UID, seg: 0 }));
    controller.push_chain_looper_op(&chain, |runtime| {
        Some(LooperOp::TapRecord {
            uid: UID,
            buffer: Some(vec![0.0f32; runtime.looper_max_frames() * 2].into_boxed_slice()),
        })
    });
    tick(&controller, &chain, 0.5);
    controller.push_chain_looper_op(&chain, |_| Some(LooperOp::Clear { uid: UID }));
    tick(&controller, &chain, 0.0);

    assert_eq!(
        controller.drain_chain_looper_layers(&chain),
        1,
        "the cleared layer is dropped here, never on the audio thread"
    );
}

#[test]
fn ops_for_one_chain_never_reach_another() {
    let (controller_a, chain_a) = controller("looper-iso-a", 1);
    let (controller_b, chain_b) = controller("looper-iso-b", 1);

    controller_a.push_chain_looper_op(&chain_a, |_| Some(LooperOp::Create { uid: UID, seg: 0 }));
    tick(&controller_a, &chain_a, 0.0);
    tick(&controller_b, &chain_b, 0.0);

    assert!(controller_a.chain_looper_status(&chain_a, UID).is_some());
    assert!(
        controller_b.chain_looper_status(&chain_b, UID).is_none(),
        "a looper belongs to ONE chain's runtimes"
    );
}

// ── #323 v2: the looper plays on an isolated stream at its chosen output ──

use project::chain::{EndpointRef, LooperConfig};

/// A chain with a two-output binding, so a looper can pick output 1 while
/// recording input 0 — the "no link between input and output" case.
fn two_output_controller(id: &str) -> (ProjectRuntimeController, Chain) {
    let registry = vec![IoBinding {
        id: "io".into(),
        name: "IO".into(),
        inputs: vec![IoEndpoint {
            name: "in0".into(),
            device_id: DeviceId("dev".into()),
            mode: ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
        outputs: vec![
            IoEndpoint {
                name: "out0".into(),
                device_id: DeviceId("dev".into()),
                mode: ChannelMode::Stereo,
                channels: vec![0, 1],
            },
            IoEndpoint {
                name: "out1".into(),
                device_id: DeviceId("dev".into()),
                mode: ChannelMode::Stereo,
                channels: vec![2, 3],
            },
        ],
    }];
    let mut chain = Chain {
        id: ChainId(id.into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks: vec![],
        di_output: None,
        loopers: vec![LooperConfig {
            uid: UID,
            output: Some(EndpointRef {
                binding_id: "io".into(),
                endpoint: "out1".into(),
            }),
            ..LooperConfig::new(UID)
        }],
    };
    let mut chains = HashMap::new();
    chains.insert(
        (chain.id.clone(), 0usize),
        Arc::new(build_chain_runtime_state(&chain, 48_000.0, &[256], &registry).expect("runtime")),
    );
    let mut controller =
        ProjectRuntimeController::for_testing_with_sample_rate(RuntimeGraph { chains }, 48_000);
    controller.set_io_bindings(registry);
    chain.loopers[0].output = Some(EndpointRef {
        binding_id: "io".into(),
        endpoint: "out1".into(),
    });
    (controller, chain)
}

#[test]
fn a_playing_looper_arms_an_isolated_stream_and_stop_disarms_it() {
    let (controller, chain) = two_output_controller("looper-iso-out");
    let id = chain.id.clone();

    // Record one callback, close the loop.
    controller.push_chain_looper_op(&id, |_| Some(LooperOp::Create { uid: UID, seg: 0 }));
    controller.push_chain_looper_op(&id, |rt| {
        Some(LooperOp::TapRecord {
            uid: UID,
            buffer: Some(vec![0.5f32; rt.looper_max_frames() * 2].into_boxed_slice()),
        })
    });
    tick(&controller, &id, 0.5);
    controller.push_chain_looper_op(&id, |_| {
        Some(LooperOp::TapRecord {
            uid: UID,
            buffer: None,
        })
    });
    tick(&controller, &id, 0.0);
    assert_eq!(
        controller.chain_looper_status(&id, UID).unwrap().state,
        LooperState::Playing
    );

    // The meter tick reconciles the stream: a Playing looper arms its own
    // isolated stream (separate from the DI's).
    controller.sync_looper_streams(&chain);
    assert!(
        controller.looper_stream_active(&id, UID),
        "a playing looper must arm its isolated playback stream"
    );
    assert!(
        !controller.di_stream_active(&id),
        "the looper stream is separate from the DI (invariant #4)"
    );

    // Stop → next reconcile disarms it.
    controller.push_chain_looper_op(&id, |_| Some(LooperOp::Stop { uid: UID }));
    tick(&controller, &id, 0.0);
    controller.sync_looper_streams(&chain);
    assert!(
        !controller.looper_stream_active(&id, UID),
        "stopping the looper must disarm its stream"
    );
}

/// Removing a looper must stop its isolated playback. The user deletes a
/// looper (the × button) but the loop keeps sounding: the command drops the
/// looper from `chain.loopers`, and the reconciler only visits the loopers
/// STILL in the chain, so the armed stream is never disarmed. RED until the
/// reconciler disarms streams for loopers that are gone.
#[test]
fn removing_a_playing_looper_disarms_its_isolated_stream() {
    let (controller, chain) = two_output_controller("looper-remove-iso");
    let id = chain.id.clone();

    // Record + close so the looper plays, then arm its stream.
    controller.push_chain_looper_op(&id, |_| Some(LooperOp::Create { uid: UID, seg: 0 }));
    controller.push_chain_looper_op(&id, |rt| {
        Some(LooperOp::TapRecord {
            uid: UID,
            buffer: Some(vec![0.5f32; rt.looper_max_frames() * 2].into_boxed_slice()),
        })
    });
    tick(&controller, &id, 0.5);
    controller.push_chain_looper_op(&id, |_| Some(LooperOp::TapRecord { uid: UID, buffer: None }));
    tick(&controller, &id, 0.0);
    controller.sync_looper_streams(&chain);
    assert!(
        controller.looper_stream_active(&id, UID),
        "precondition: the loop is armed and playing"
    );

    // The user presses × : the command removes the looper from the chain and
    // frees the runtime slot.
    controller.push_chain_looper_op(&id, |_| Some(LooperOp::Remove { uid: UID }));
    tick(&controller, &id, 0.0);
    let mut without = chain.clone();
    without.loopers.clear();

    // The very next reconcile must silence it.
    controller.sync_looper_streams(&without);
    assert!(
        !controller.looper_stream_active(&id, UID),
        "removing a looper must disarm its isolated stream — the loop must stop"
    );
}
