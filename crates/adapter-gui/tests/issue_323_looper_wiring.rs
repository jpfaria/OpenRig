//! Issue #323 — the wiring that turns looper events into audio-thread ops.
//!
//! The #614 trap: dispatching a command only records intent. Unless the
//! adapter applies it to the runtime, the transport button lights up and
//! nothing is recorded. These tests drive the event path and assert on the
//! RUNTIME state, never on the emitted event.

use std::collections::HashMap;
use std::sync::Arc;

use adapter_gui::looper_wiring::apply_looper_event;
use application::command::{LooperAction, LooperParam};
use application::event::Event;
use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use engine::runtime::{build_chain_runtime_state, RuntimeGraph};
use engine::LooperState;
use infra_cpal::ProjectRuntimeController;
use project::chain::{Chain, LooperSpeed};

const UID: u64 = 1;

fn controller(id: &str) -> (ProjectRuntimeController, ChainId) {
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
    let mut chains = HashMap::new();
    chains.insert(
        (chain.id.clone(), 0usize),
        Arc::new(build_chain_runtime_state(&chain, 48_000.0, &[256], &registry).expect("runtime")),
    );
    let mut controller =
        ProjectRuntimeController::for_testing_with_sample_rate(RuntimeGraph { chains }, 48_000);
    controller.set_io_bindings(registry);
    (controller, chain.id)
}

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
fn added_event_claims_the_slot_on_the_runtime() {
    let (controller, chain) = controller("wire-add");
    apply_looper_event(
        &controller,
        &Event::ChainLooperAdded {
            chain: chain.clone(),
            looper: UID,
        },
    );
    tick(&controller, &chain, 0.0);

    assert_eq!(
        controller.chain_looper_status(&chain, UID).map(|s| s.state),
        Some(LooperState::Empty),
        "the runtime must hold the looper — a dispatch alone is dead (#614)"
    );
}

#[test]
fn record_tap_records_then_closes_the_loop_on_the_runtime() {
    let (controller, chain) = controller("wire-record");
    apply_looper_event(
        &controller,
        &Event::ChainLooperAdded {
            chain: chain.clone(),
            looper: UID,
        },
    );
    tick(&controller, &chain, 0.0);

    let record = Event::ChainLooperTransportChanged {
        chain: chain.clone(),
        looper: UID,
        action: LooperAction::Record,
    };
    apply_looper_event(&controller, &record);
    tick(&controller, &chain, 0.5);
    assert_eq!(
        controller.chain_looper_status(&chain, UID).map(|s| s.state),
        Some(LooperState::Recording),
        "the first tap must arm a recording — the wiring allocates the layer"
    );

    // The same action taps again: the loop closes and starts playing.
    apply_looper_event(&controller, &record);
    tick(&controller, &chain, 0.0);
    let status = controller.chain_looper_status(&chain, UID).expect("status");
    assert_eq!(status.state, LooperState::Playing);
    assert_eq!(status.len_frames, 128);
}

#[test]
fn stop_undo_clear_reach_the_runtime() {
    let (controller, chain) = controller("wire-transport");
    apply_looper_event(
        &controller,
        &Event::ChainLooperAdded {
            chain: chain.clone(),
            looper: UID,
        },
    );
    tick(&controller, &chain, 0.0);
    let action = |a: LooperAction| Event::ChainLooperTransportChanged {
        chain: chain.clone(),
        looper: UID,
        action: a,
    };
    apply_looper_event(&controller, &action(LooperAction::Record));
    tick(&controller, &chain, 0.5);
    apply_looper_event(&controller, &action(LooperAction::Record));
    tick(&controller, &chain, 0.0);

    apply_looper_event(&controller, &action(LooperAction::Stop));
    tick(&controller, &chain, 0.0);
    assert_eq!(
        controller.chain_looper_status(&chain, UID).map(|s| s.state),
        Some(LooperState::Stopped)
    );

    apply_looper_event(&controller, &action(LooperAction::Undo));
    tick(&controller, &chain, 0.0);
    assert_eq!(
        controller
            .chain_looper_status(&chain, UID)
            .map(|s| s.layers),
        Some(0),
        "undo drops the only layer"
    );

    apply_looper_event(&controller, &action(LooperAction::Clear));
    tick(&controller, &chain, 0.0);
    let status = controller.chain_looper_status(&chain, UID).expect("status");
    assert_eq!(status.state, LooperState::Empty);
    assert_eq!(status.len_frames, 0);
}

#[test]
fn param_events_reach_the_runtime_and_the_loop_level_follows() {
    let (controller, chain) = controller("wire-params");
    apply_looper_event(
        &controller,
        &Event::ChainLooperAdded {
            chain: chain.clone(),
            looper: UID,
        },
    );
    tick(&controller, &chain, 0.0);
    let record = Event::ChainLooperTransportChanged {
        chain: chain.clone(),
        looper: UID,
        action: LooperAction::Record,
    };
    apply_looper_event(&controller, &record);
    tick(&controller, &chain, 1.0);
    apply_looper_event(&controller, &record);

    for param in [
        LooperParam::Mix(0.0),
        LooperParam::Decay(0.5),
        LooperParam::Speed(LooperSpeed::Double),
        LooperParam::Reverse(true),
    ] {
        apply_looper_event(
            &controller,
            &Event::ChainLooperParamChanged {
                chain: chain.clone(),
                looper: UID,
                param,
            },
        );
    }

    // Mix 0 silences the loop: with a silent input the output stays silent.
    let frames = 128usize;
    let input = vec![0.0f32; frames * 2];
    let mut output = vec![0.0f32; frames * 2];
    for runtime in controller.runtimes_for_chain(&chain) {
        engine::runtime::process_input_f32(&runtime, 0, &input, 2);
        engine::runtime::process_output_f32(&runtime, 0, &mut output, 2);
    }
    let peak = output.iter().cloned().fold(0.0f32, |m, s| m.max(s.abs()));
    assert!(peak < 1e-6, "mix 0 must silence the loop, got peak {peak}");
}

#[test]
fn removed_event_frees_the_slot_and_the_layer_memory() {
    let (controller, chain) = controller("wire-remove");
    apply_looper_event(
        &controller,
        &Event::ChainLooperAdded {
            chain: chain.clone(),
            looper: UID,
        },
    );
    tick(&controller, &chain, 0.0);
    apply_looper_event(
        &controller,
        &Event::ChainLooperTransportChanged {
            chain: chain.clone(),
            looper: UID,
            action: LooperAction::Record,
        },
    );
    tick(&controller, &chain, 0.5);

    apply_looper_event(
        &controller,
        &Event::ChainLooperRemoved {
            chain: chain.clone(),
            looper: UID,
        },
    );
    tick(&controller, &chain, 0.0);

    assert!(controller.chain_looper_status(&chain, UID).is_none());
    // The audio thread parked the layer on the return queue during the
    // callback above; the GUI tick collects and drops it here — the audio
    // thread never frees (invariant #8).
    assert_eq!(controller.drain_chain_looper_layers(&chain), 1);
    assert_eq!(
        controller.drain_chain_looper_layers(&chain),
        0,
        "nothing is left waiting on the return queue"
    );
}
