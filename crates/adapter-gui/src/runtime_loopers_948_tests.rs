//! #948: the runtime seam hands the Tone Doctor the loops that are SOUNDING — a
//! playing loop is offered with its rate, a stopped one is not.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use engine::runtime::{build_chain_runtime_state, RuntimeGraph};
use infra_cpal::ProjectRuntimeController;
use project::chain::{Chain, LooperConfig};

use super::playing_chain_loops;

const UID: u64 = 7;
const SR: u32 = 48_000;

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
        outputs: vec![IoEndpoint {
            name: "out0".into(),
            device_id: DeviceId("dev".into()),
            mode: ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    }]
}

fn chain() -> Chain {
    Chain {
        id: ChainId("doctor-loop".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks: vec![],
        di_output: None,
        loopers: vec![LooperConfig::new(UID)],
    }
}

/// A runtime holding one recorded, playing loop.
fn runtime_with_playing_loop(chain: &Chain) -> Rc<RefCell<Option<ProjectRuntimeController>>> {
    let mut graph = HashMap::new();
    graph.insert(
        (chain.id.clone(), 0usize),
        Arc::new(build_chain_runtime_state(chain, SR as f32, &[256], &registry()).unwrap()),
    );
    let mut c =
        ProjectRuntimeController::for_testing_with_sample_rate(RuntimeGraph { chains: graph }, SR);
    c.set_io_bindings(registry());
    c.looper_create(&chain.id, UID);
    c.looper_tap_record(&chain.id, UID);
    c.drain_looper_recording(chain);
    let input = vec![0.5f32; 256];
    let mut out = vec![0.0f32; 256];
    for rt in c.runtimes_for_chain(&chain.id) {
        engine::runtime::process_input_f32(&rt, 0, &input, 2);
        engine::runtime::process_output_f32(&rt, 0, &mut out, 2);
    }
    c.drain_looper_recording(chain);
    c.looper_tap_record(&chain.id, UID);
    Rc::new(RefCell::new(Some(c)))
}

#[test]
fn a_playing_loop_is_handed_to_the_tone_doctor_with_its_rate() {
    let chain = chain();
    let runtime = runtime_with_playing_loop(&chain);

    let loops = playing_chain_loops(&runtime, &chain);
    assert_eq!(loops.len(), 1, "#948: the playing loop is a source");
    assert_eq!(loops[0].sample_rate(), SR);
    assert!(!loops[0].is_empty(), "it carries the recorded take");
}

#[test]
fn a_stopped_loop_is_not_handed_over() {
    let chain = chain();
    let runtime = runtime_with_playing_loop(&chain);
    runtime
        .borrow()
        .as_ref()
        .unwrap()
        .looper_stop(&chain.id, UID);
    assert!(playing_chain_loops(&runtime, &chain).is_empty());
}

#[test]
fn no_runtime_hands_over_nothing() {
    let chain = chain();
    let runtime: Rc<RefCell<Option<ProjectRuntimeController>>> = Rc::new(RefCell::new(None));
    assert!(playing_chain_loops(&runtime, &chain).is_empty());
}
