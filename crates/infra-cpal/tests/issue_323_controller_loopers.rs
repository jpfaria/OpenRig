//! Issue #323 — the controller's looper facade, redesigned around the
//! controller-owned `LooperStore`: control is deterministic with no runtime
//! running, recording drains the chain's input tap into the store, and the
//! isolated playback stream arms/disarms from store state.

use std::collections::HashMap;
use std::sync::Arc;

use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use engine::runtime::{build_chain_runtime_state, RuntimeGraph};
use engine::LooperState;
use infra_cpal::ProjectRuntimeController;
use project::chain::{Chain, EndpointRef, LooperConfig};

const UID: u64 = 3;

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

fn controller_for(chain: &Chain, registry: &[IoBinding]) -> ProjectRuntimeController {
    let mut chains = HashMap::new();
    chains.insert(
        (chain.id.clone(), 0usize),
        Arc::new(build_chain_runtime_state(chain, 48_000.0, &[256], registry).expect("runtime")),
    );
    let mut c =
        ProjectRuntimeController::for_testing_with_sample_rate(RuntimeGraph { chains }, 48_000);
    c.set_io_bindings(registry.to_vec());
    c
}

/// One audio callback on the chain's runtime — fills any subscribed input tap.
fn feed_input(c: &ProjectRuntimeController, chain: &ChainId, level: f32) {
    let frames = 128usize;
    let input = vec![level; frames * 2];
    let mut out = vec![0.0f32; frames * 2];
    for rt in c.runtimes_for_chain(chain) {
        engine::runtime::process_input_f32(&rt, 0, &input, 2);
        engine::runtime::process_output_f32(&rt, 0, &mut out, 2);
    }
}

/// Record + close one loop through the store + tap so it is Playing.
fn record_and_close(c: &ProjectRuntimeController, chain: &Chain) {
    let id = chain.id.clone();
    c.looper_create(&id, UID);
    c.looper_tap_record(&id, UID); // Empty → Recording
    c.drain_looper_recording(chain); // subscribe the input tap
    feed_input(c, &id, 0.5); // fill the ring
    c.drain_looper_recording(chain); // drain into the loop
    c.looper_tap_record(&id, UID); // close → Playing
}

#[test]
fn control_is_deterministic_with_no_runtime() {
    // No runtime at all — the store alone answers.
    let c = ProjectRuntimeController::for_testing(RuntimeGraph {
        chains: HashMap::new(),
    });
    let id = ChainId("x".into());
    c.looper_create(&id, UID);
    assert_eq!(
        c.chain_looper_status(&id, UID).unwrap().state,
        LooperState::Empty
    );
    c.looper_remove(&id, UID);
    assert!(c.chain_looper_status(&id, UID).is_none());
}

#[test]
fn recording_captures_from_the_input_tap_and_closing_defines_the_loop() {
    let chain = chain_with_looper("rec");
    let c = controller_for(&chain, &registry());
    record_and_close(&c, &chain);

    let s = c.chain_looper_status(&chain.id, UID).expect("status");
    assert_eq!(s.state, LooperState::Playing);
    assert!(
        s.len_frames > 0,
        "the captured input defines the loop length"
    );
    assert!(c.export_chain_looper(&chain.id, UID).is_some());
}

#[test]
fn a_playing_looper_arms_its_isolated_stream_and_stop_disarms_it() {
    let chain = chain_with_looper("arm");
    let c = controller_for(&chain, &registry());
    record_and_close(&c, &chain);

    c.sync_looper_streams(&chain);
    assert!(
        c.looper_stream_active(&chain.id, UID),
        "a playing loop arms its isolated stream"
    );
    assert!(!c.di_stream_active(&chain.id), "separate from the DI (#4)");

    c.looper_stop(&chain.id, UID);
    assert_eq!(
        c.chain_looper_status(&chain.id, UID).unwrap().state,
        LooperState::Stopped
    );
    c.sync_looper_streams(&chain);
    assert!(
        !c.looper_stream_active(&chain.id, UID),
        "stop disarms the stream"
    );
}

#[test]
fn stop_disarms_even_with_the_chain_not_streaming() {
    // The redesign's whole point: stop is authoritative regardless of the
    // chain callback. Record + arm, then stop with NO further tick/drain.
    let chain = chain_with_looper("no-stream");
    let c = controller_for(&chain, &registry());
    record_and_close(&c, &chain);
    c.sync_looper_streams(&chain);
    assert!(c.looper_stream_active(&chain.id, UID));

    c.looper_stop(&chain.id, UID); // store updates immediately, no runtime needed
    c.sync_looper_streams(&chain);
    assert!(
        !c.looper_stream_active(&chain.id, UID),
        "stop silences the loop even when nothing is streaming"
    );
}

#[test]
fn removing_a_playing_looper_disarms_its_isolated_stream() {
    let chain = chain_with_looper("remove");
    let c = controller_for(&chain, &registry());
    record_and_close(&c, &chain);
    c.sync_looper_streams(&chain);
    assert!(c.looper_stream_active(&chain.id, UID));

    c.looper_remove(&chain.id, UID);
    let mut without = chain.clone();
    without.loopers.clear();
    c.sync_looper_streams(&without);
    assert!(
        !c.looper_stream_active(&chain.id, UID),
        "removing a looper must stop its isolated stream"
    );
}

#[test]
fn sync_looper_slots_creates_store_entries_for_project_loopers() {
    // A looper added by any transport (MCP/MIDI) lands in the project but not
    // the store until reconciled — so the read query would be empty. The tick's
    // reconcile creates the missing entry.
    let chain = chain_with_looper("slot-sync");
    let c = controller_for(&chain, &registry());
    assert!(
        c.chain_looper_status(&chain.id, UID).is_none(),
        "precondition: no store entry yet"
    );
    c.sync_looper_slots(&chain);
    assert!(
        c.chain_looper_status(&chain.id, UID).is_some(),
        "the reconcile gives the project's looper a store entry"
    );
}

#[test]
fn a_loop_belongs_to_one_chain_only() {
    let chain_a = chain_with_looper("iso-a");
    let chain_b = chain_with_looper("iso-b");
    let c = controller_for(&chain_a, &registry());
    c.looper_create(&chain_a.id, UID);
    assert!(c.chain_looper_status(&chain_a.id, UID).is_some());
    assert!(
        c.chain_looper_status(&chain_b.id, UID).is_none(),
        "a loop belongs to the chain it was created on"
    );
}
