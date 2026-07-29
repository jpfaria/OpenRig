//! Issue #323 — the wiring that turns looper EVENTS into controller-store
//! mutations. A dispatch alone is dead (#614); `apply_looper_event` is what
//! makes the store learn about it. These tests drive the event path and assert
//! on the store state and the isolated playback stream — never on the event.

use std::collections::HashMap;
use std::sync::Arc;

use adapter_gui::looper_wiring::{apply_looper_event, apply_looper_events};
use application::command::{LooperAction, LooperParam};
use application::event::Event;
use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use engine::runtime::{build_chain_runtime_state, RuntimeGraph};
use engine::LooperState;
use infra_cpal::ProjectRuntimeController;
use project::chain::{Chain, EndpointRef, LooperConfig, LooperSpeed};

const UID: u64 = 1;

fn registry() -> Vec<IoBinding> {
    vec![IoBinding {
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
    }]
}

fn chain_with_looper(id: &str) -> Chain {
    Chain {
        id: ChainId(id.into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks: vec![],
        di_output: None,
        loopers: vec![LooperConfig {
            output: Some(EndpointRef {
                binding_id: "io".into(),
                endpoint: "out1".into(),
            }),
            ..LooperConfig::new(UID)
        }],
    }
}

fn controller_for(chain: &Chain) -> ProjectRuntimeController {
    let registry = registry();
    let mut chains = HashMap::new();
    chains.insert(
        (chain.id.clone(), 0usize),
        Arc::new(build_chain_runtime_state(chain, 48_000.0, &[256], &registry).expect("runtime")),
    );
    let mut c =
        ProjectRuntimeController::for_testing_with_sample_rate(RuntimeGraph { chains }, 48_000);
    c.set_io_bindings(registry);
    c
}

fn feed_input(c: &ProjectRuntimeController, chain: &ChainId, level: f32) {
    let frames = 128usize;
    let input = vec![level; frames * 2];
    let mut out = vec![0.0f32; frames * 2];
    for rt in c.runtimes_for_chain(chain) {
        engine::runtime::process_input_f32(&rt, 0, &input, 2);
        engine::runtime::process_output_f32(&rt, 0, &mut out, 2);
    }
}

fn ev(chain: &ChainId, action: LooperAction) -> Event {
    Event::ChainLooperTransportChanged {
        chain: chain.clone(),
        looper: UID,
        action,
    }
}

/// Record + close one loop through the EVENT path + the input tap, so it plays.
fn record_and_arm(c: &ProjectRuntimeController, chain: &Chain) {
    apply_looper_event(
        c,
        &Event::ChainLooperAdded {
            chain: chain.id.clone(),
            looper: UID,
        },
    );
    apply_looper_event(c, &ev(&chain.id, LooperAction::Record)); // → Recording
    c.drain_looper_recording(chain); // subscribe tap
    feed_input(c, &chain.id, 0.5); // fill ring
    c.drain_looper_recording(chain); // drain into loop
    apply_looper_event(c, &ev(&chain.id, LooperAction::Record)); // close → Playing
    c.sync_looper_streams(chain);
    assert!(
        c.looper_stream_active(&chain.id, UID),
        "precondition: a closed loop arms its isolated stream"
    );
}

/// The bug the MCP repro exposed: a looper driven by a NON-GUI transport
/// (MCP/MIDI) drains its events through the shared `apply_events_to_ui`, which
/// never touched the looper store — so Record left the loop `Empty`. The store
/// must learn about the batch through the shared `apply_looper_events` path,
/// exactly like the GUI button path does inline.
#[test]
fn drained_looper_events_reach_the_store() {
    let chain = chain_with_looper("wire-drain");
    let c = controller_for(&chain);
    let chains = vec![chain.clone()];
    // The batch a non-GUI transport (MCP/MIDI) drains: add then record.
    let events = vec![
        Event::ChainLooperAdded {
            chain: chain.id.clone(),
            looper: UID,
        },
        ev(&chain.id, LooperAction::Record),
    ];
    apply_looper_events(&c, &chains, &events);
    assert_eq!(
        c.chain_looper_status(&chain.id, UID).map(|s| s.state),
        Some(LooperState::Recording),
        "a looper transport drained from MCP/MIDI must reach the store, not just the GUI"
    );
}

#[test]
fn added_event_creates_the_loop_in_the_store() {
    let chain = chain_with_looper("wire-add");
    let c = controller_for(&chain);
    apply_looper_event(
        &c,
        &Event::ChainLooperAdded {
            chain: chain.id.clone(),
            looper: UID,
        },
    );
    assert_eq!(
        c.chain_looper_status(&chain.id, UID).map(|s| s.state),
        Some(LooperState::Empty),
        "the store holds the loop — a dispatch alone is dead (#614)"
    );
}

#[test]
fn record_event_records_then_closes_the_loop() {
    let chain = chain_with_looper("wire-record");
    let c = controller_for(&chain);
    apply_looper_event(
        &c,
        &Event::ChainLooperAdded {
            chain: chain.id.clone(),
            looper: UID,
        },
    );
    apply_looper_event(&c, &ev(&chain.id, LooperAction::Record));
    assert_eq!(
        c.chain_looper_status(&chain.id, UID).map(|s| s.state),
        Some(LooperState::Recording)
    );
    c.drain_looper_recording(&chain);
    feed_input(&c, &chain.id, 0.5);
    c.drain_looper_recording(&chain);
    apply_looper_event(&c, &ev(&chain.id, LooperAction::Record));
    let s = c.chain_looper_status(&chain.id, UID).expect("status");
    assert_eq!(s.state, LooperState::Playing);
    assert!(s.len_frames > 0, "the captured input defines the loop");
}

#[test]
fn stop_and_clear_events_reach_the_store() {
    let chain = chain_with_looper("wire-transport");
    let c = controller_for(&chain);
    record_and_arm(&c, &chain);

    apply_looper_event(&c, &ev(&chain.id, LooperAction::Stop));
    assert_eq!(
        c.chain_looper_status(&chain.id, UID).map(|s| s.state),
        Some(LooperState::Stopped)
    );

    apply_looper_event(&c, &ev(&chain.id, LooperAction::Clear));
    let s = c.chain_looper_status(&chain.id, UID).expect("status");
    assert_eq!(s.state, LooperState::Empty);
    assert_eq!(s.len_frames, 0);
}

#[test]
fn param_events_reach_the_store_and_the_loop_keeps_playing() {
    let chain = chain_with_looper("wire-params");
    let c = controller_for(&chain);
    record_and_arm(&c, &chain);

    for param in [
        LooperParam::Mix(0.5),
        LooperParam::Decay(0.5),
        LooperParam::Speed(LooperSpeed::Double),
        LooperParam::Reverse(true),
    ] {
        apply_looper_event(
            &c,
            &Event::ChainLooperParamChanged {
                chain: chain.id.clone(),
                looper: UID,
                param,
            },
        );
    }
    let s = c.chain_looper_status(&chain.id, UID).expect("status");
    assert_eq!(s.state, LooperState::Playing);
    assert!(c.export_chain_looper(&chain.id, UID).is_some());
}

#[test]
fn stop_disarms_the_stream_even_when_the_chain_is_not_streaming() {
    // The redesign's core: stop is authoritative regardless of the chain
    // callback. After arming, stop with NO further tick/drain and reconcile.
    let chain = chain_with_looper("wire-stop-no-stream");
    let c = controller_for(&chain);
    record_and_arm(&c, &chain);

    apply_looper_event(&c, &ev(&chain.id, LooperAction::Stop));
    c.sync_looper_streams(&chain);
    assert!(
        !c.looper_stream_active(&chain.id, UID),
        "stop silences the loop even when nothing is streaming"
    );
}

#[test]
fn play_after_stop_re_arms() {
    let chain = chain_with_looper("wire-replay");
    let c = controller_for(&chain);
    record_and_arm(&c, &chain);

    apply_looper_event(&c, &ev(&chain.id, LooperAction::Stop));
    c.sync_looper_streams(&chain);
    assert!(!c.looper_stream_active(&chain.id, UID));

    apply_looper_event(&c, &ev(&chain.id, LooperAction::PlayStop)); // stopped → play
    c.sync_looper_streams(&chain);
    assert!(
        c.looper_stream_active(&chain.id, UID),
        "play after stop must sound the loop again"
    );
}

#[test]
fn removed_event_frees_the_loop_and_disarms_its_stream() {
    let chain = chain_with_looper("wire-remove");
    let c = controller_for(&chain);
    record_and_arm(&c, &chain);

    apply_looper_event(
        &c,
        &Event::ChainLooperRemoved {
            chain: chain.id.clone(),
            looper: UID,
        },
    );
    assert!(c.chain_looper_status(&chain.id, UID).is_none());
    assert!(
        !c.looper_stream_active(&chain.id, UID),
        "removing a loop stops its isolated stream"
    );
}
