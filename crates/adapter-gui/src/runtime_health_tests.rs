//! #127: the poll tick's two doors, split by what they actually do.
//!
//! Advancing a pending rebuild is a MUTATION, not a read: an edit to a live
//! chain is built off the control worker (#672) and only becomes audible when
//! the frontend's tick swaps it into the live slot. Before this the tick
//! reached `ProjectRuntimeController::poll_pending_rebuilds` directly, so the
//! one thing keeping every live edit audible was a GUI timer holding the audio
//! backend.
//!
//! The other half — the block errors the audio thread reported, and whether
//! the backend is still there — is a READ, and goes through `LiveSource`. Its
//! impl lives in `gui_live_source` with the frontend's other readings; it is
//! tested here because both doors serve the same tick and share this fixture.
//!
//! These drive a device-less controller: the rebuild path resolves its
//! endpoints from the binding registry, never from real hardware.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use engine::runtime::{build_chain_runtime_state, RuntimeGraph};
use infra_cpal::ProjectRuntimeController;
use project::chain::Chain;

use super::polling_runtime_control;
use crate::gui_live_source::health_live_source;

const CHAIN: &str = "chain:127:health";

fn registry() -> Vec<IoBinding> {
    vec![IoBinding {
        id: "io".into(),
        name: "IO".into(),
        inputs: vec![IoEndpoint {
            name: "in0".into(),
            device_id: DeviceId("dev".into()),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs: vec![IoEndpoint {
            name: "out0".into(),
            device_id: DeviceId("dev".into()),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
    }]
}

fn chain(id: &str) -> Chain {
    Chain {
        id: ChainId(id.into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks: vec![],
        di_output: None,
        loopers: vec![],
    }
}

/// A controller with one live chain slot and no audio devices.
fn controller(chains: &[Chain]) -> Rc<RefCell<Option<ProjectRuntimeController>>> {
    let mut graph = HashMap::new();
    for c in chains {
        graph.insert(
            (c.id.clone(), 0usize),
            Arc::new(build_chain_runtime_state(c, 48_000.0, &[1024], &registry()).unwrap()),
        );
    }
    let mut controller = ProjectRuntimeController::for_testing(RuntimeGraph { chains: graph });
    controller.set_io_bindings(registry());
    Rc::new(RefCell::new(Some(controller)))
}

fn live_runtime(
    runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>,
    chain: &ChainId,
) -> Arc<engine::runtime::ChainRuntimeState> {
    runtime
        .borrow()
        .as_ref()
        .expect("controller")
        .chain_runtime(chain)
        .expect("the chain has a live runtime")
}

/// One audio callback on every runtime the chain owns — enough for the audio
/// thread to drain its queues and post whatever it could not do.
fn drive_one_callback(runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>, chain: &ChainId) {
    let borrow = runtime.borrow();
    let controller = borrow.as_ref().expect("controller");
    let frames = 32usize;
    let input = vec![0.0f32; frames];
    let mut output = vec![0.0f32; frames];
    for rt in controller.runtimes_for_chain(chain) {
        engine::runtime::process_input_f32(&rt, 0, &input, 1);
        engine::runtime::process_output_f32(&rt, 0, &mut output, 1);
    }
}

/// Ask the audio thread to toggle a block this chain does not have. The
/// lookup happens on the audio thread, which cannot log or allocate, so it
/// posts a `BlockError` for the frontend to drain — a real error, produced
/// the way real ones are.
fn provoke_a_block_error(runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>, chain: &ChainId) {
    runtime
        .borrow()
        .as_ref()
        .expect("controller")
        .set_block_enabled(chain, &BlockId("does:not:exist".into()), false)
        .expect("queueing a toggle succeeds even for an unknown block");
    drive_one_callback(runtime, chain);
}

#[test]
fn a_block_error_surfaces_through_the_read_door_naming_its_chain() {
    // The audio thread cannot show a toast, so it posts the failure and the
    // frontend drains it. Reading it through the seam is what lets the poll
    // tick stop holding the audio backend — and the reading names the chain
    // that failed, so an error is never an anonymous entry from "some
    // stream" (`CLAUDE.md` LAW).
    let chain = chain(CHAIN);
    let runtime = controller(std::slice::from_ref(&chain));
    let live = health_live_source(&runtime);

    provoke_a_block_error(&runtime, &chain.id);

    let errors = live
        .block_errors()
        .expect("a hosted runtime always answers, even with nothing to report");
    let first = errors
        .first()
        .expect("the failure the audio thread reported must reach the frontend");
    assert_eq!(
        first.chain, chain.id,
        "the reading must name the chain whose runtime raised it"
    );
    assert_eq!(first.block, BlockId("does:not:exist".into()));
    assert!(
        first.message.contains("not found"),
        "the audio thread's own diagnosis must survive the crossing, got {:?}",
        first.message
    );
}

#[test]
fn the_error_read_drains_what_it_reports() {
    // The queue is emptied by the read, which is why there can only ever be
    // one consumer: a second reader would take the toasts away from the
    // first. Pin it, so nobody later promotes this to a shared query.
    let chain = chain(CHAIN);
    let runtime = controller(std::slice::from_ref(&chain));
    let live = health_live_source(&runtime);

    provoke_a_block_error(&runtime, &chain.id);
    let first = live.block_errors().expect("hosted");
    assert_eq!(first.len(), 1, "the reported failure");

    let second = live.block_errors().expect("hosted");
    assert!(
        second.is_empty(),
        "the first read took them; a second must report nothing new, got {}",
        second.len()
    );
}

#[test]
fn health_reads_as_not_hosted_when_the_rig_is_stopped() {
    // "No runtime" is not "unhealthy": there is nothing to reconnect to, and
    // the tick must not start toasting a disconnection at an app that simply
    // has nothing playing.
    let runtime = Rc::new(RefCell::new(None));
    let live = health_live_source(&runtime);

    assert!(
        live.audio_health().is_none(),
        "with no controller the frontend hosts no health to report"
    );
    assert!(
        live.block_errors().is_none(),
        "and no error queue either — never an empty reading, which would mean \
         'hosted and quiet'"
    );
}

#[test]
fn a_hosted_but_idle_runtime_reads_as_healthy_and_not_running() {
    // The tick only chases a reconnection for a rig that is RUNNING. A
    // controller that exists with nothing sounding must therefore read as
    // hosted, healthy and idle — collapsing idle into unhealthy would have
    // the health timer tear down and re-sync an app at rest.
    let chain = chain(CHAIN);
    let runtime = controller(std::slice::from_ref(&chain));
    let live = health_live_source(&runtime);

    let health = live.audio_health().expect("a controller is hosted");
    assert!(
        !health.running,
        "no stream is open: nothing is sounding on this controller"
    );
    assert!(
        health.healthy,
        "an idle backend is not a broken one — nothing to reconnect"
    );
}

#[test]
fn a_pending_rebuild_is_installed_through_the_write_door() {
    // The edit is already built and waiting; what makes it audible is the
    // tick. If the door does not advance it, the rig keeps playing the
    // pre-edit graph forever — which is what the GUI timer used to prevent by
    // holding the audio backend itself.
    let chain = chain(CHAIN);
    let runtime = controller(std::slice::from_ref(&chain));
    let before = live_runtime(&runtime, &chain.id);
    let session = Rc::new(RefCell::new(None));
    let control = polling_runtime_control(&runtime, &session);

    runtime
        .borrow_mut()
        .as_mut()
        .expect("controller")
        .schedule_chain_rebuild(&chain, 48_000.0, HashMap::new(), vec![1024]);

    let mut applied = 0;
    let deadline = Instant::now() + Duration::from_secs(10);
    while applied == 0 && Instant::now() < deadline {
        applied += control.apply_finished_rebuilds();
        std::thread::yield_now();
    }

    assert_eq!(
        applied, 1,
        "the tick's door must install the rebuild the control worker finished \
         — otherwise a live edit is built and never becomes audible"
    );
    let after = live_runtime(&runtime, &chain.id);
    assert!(
        !Arc::ptr_eq(&before, &after),
        "the freshly built runtime must be the one in the live slot now"
    );
}

#[test]
fn the_door_installs_only_the_rebuild_its_chain_asked_for() {
    // Isolation (`CLAUDE.md` LAW): one chain's rebuild lands in ITS OWN slot.
    // A sibling that asked for nothing must still be playing the exact same
    // runtime Arc afterwards — no re-publish, no swap, no touch.
    let edited = chain(CHAIN);
    let untouched = chain("chain:127:sibling");
    let runtime = controller(&[edited.clone(), untouched.clone()]);
    let sibling_before = live_runtime(&runtime, &untouched.id);
    let session = Rc::new(RefCell::new(None));
    let control = polling_runtime_control(&runtime, &session);

    runtime
        .borrow_mut()
        .as_mut()
        .expect("controller")
        .schedule_chain_rebuild(&edited, 48_000.0, HashMap::new(), vec![1024]);

    let deadline = Instant::now() + Duration::from_secs(10);
    let mut applied = 0;
    while applied == 0 && Instant::now() < deadline {
        applied += control.apply_finished_rebuilds();
        std::thread::yield_now();
    }
    assert_eq!(applied, 1, "the edited chain's rebuild must be installed");

    let sibling_after = live_runtime(&runtime, &untouched.id);
    assert!(
        Arc::ptr_eq(&sibling_before, &sibling_after),
        "a rebuild belongs to the chain that asked for it — the sibling's live \
         runtime must be untouched"
    );
}

#[test]
fn a_tick_with_no_runtime_is_a_no_op() {
    // The tick runs for the whole life of the app, most of it with nothing
    // playing. A tick is not a request to hear anything, so it must never
    // bring a runtime up (unlike an arm or a transport start, #808).
    let runtime = Rc::new(RefCell::new(None));
    let session = Rc::new(RefCell::new(None));
    let control = polling_runtime_control(&runtime, &session);

    assert_eq!(
        control.apply_finished_rebuilds(),
        0,
        "no runtime means no queue to service"
    );
    assert!(
        runtime.borrow().is_none(),
        "the tick must not create an audio runtime"
    );
}
