//! #127: the three doors the chain row's meter tick reads and writes through.
//!
//! The tick used to hold `ProjectRuntimeController` for a handful of per-chain
//! calls: the DI's playing/peak state, the xrun and underrun counters, whether
//! the chain has a live runtime, and the looper reconcile that makes a RECORD
//! actually capture. Two of those are READS and one is a WRITE, and every one
//! of them addresses ONE chain (`CLAUDE.md` LAW).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use engine::runtime::{build_chain_runtime_state, RuntimeGraph};
use infra_cpal::ProjectRuntimeController;
use project::chain::{Chain, LooperConfig, LooperSpeed};

use crate::gui_live_source::chain_row_live_source;
use crate::runtime_health::polling_runtime_control;

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

fn looper(uid: u64) -> LooperConfig {
    LooperConfig {
        uid,
        mix: 1.0,
        decay: 1.0,
        speed: LooperSpeed::default(),
        reverse: false,
        audio_file: None,
        input: None,
        output: None,
        preset: None,
    }
}

fn chain(id: &str, loopers: Vec<LooperConfig>) -> Chain {
    Chain {
        id: ChainId(id.into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks: vec![],
        di_output: None,
        loopers,
    }
}

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

fn statuses(
    runtime: &Rc<RefCell<Option<ProjectRuntimeController>>>,
    chain: &ChainId,
) -> Vec<engine::LooperStatus> {
    runtime
        .borrow()
        .as_ref()
        .expect("controller")
        .chain_looper_statuses(chain)
}

#[test]
fn a_hosted_chain_reports_its_own_runtime_state() {
    // The row's overload badge and its REC-armable flag both come from here.
    // A chain that HAS a runtime reads as live with no overruns yet; the
    // counters are this chain's own, never a rig-wide pool.
    let c = chain("chain:127:row", vec![]);
    let runtime = controller(std::slice::from_ref(&c));
    let live = chain_row_live_source(&runtime);

    let reading = live
        .chain_runtime(&c.id)
        .expect("a hosted frontend always answers for a chain it knows");
    assert!(
        reading.live,
        "this chain has a per-input runtime, so REC is armable"
    );
    assert_eq!(reading.xruns, 0, "nothing has run yet");
    assert_eq!(reading.underruns, 0);
}

#[test]
fn a_chain_with_no_runtime_reads_as_not_live_on_a_hosted_frontend() {
    // "This chain is not playing" is an ANSWER. Collapsing it into `None`
    // would make the row unable to tell a cold-starting chain from a stopped
    // rig, and REC would be armable over nothing.
    let c = chain("chain:127:row", vec![]);
    let runtime = controller(std::slice::from_ref(&c));
    let live = chain_row_live_source(&runtime);

    let reading = live
        .chain_runtime(&ChainId("chain:127:absent".into()))
        .expect("the frontend is hosted, so it answers");
    assert!(!reading.live, "a chain with no runtime is not live");
}

#[test]
fn a_stopped_rig_answers_neither_runtime_state_nor_di() {
    let runtime = Rc::new(RefCell::new(None));
    let live = chain_row_live_source(&runtime);

    assert!(live
        .chain_runtime(&ChainId("chain:127:row".into()))
        .is_none());
    assert!(live
        .chain_di_loop(&ChainId("chain:127:row".into()))
        .is_none());
}

#[test]
fn a_hosted_chain_reports_its_own_di_state() {
    // The chain tile's DI lamp and the DI meter row. Nothing is armed here, so
    // it must read as not playing — never as "unknown", which the tile would
    // render as the previous frame's lamp.
    let c = chain("chain:127:row", vec![]);
    let runtime = controller(std::slice::from_ref(&c));
    let live = chain_row_live_source(&runtime);

    let reading = live
        .chain_di_loop(&c.id)
        .expect("a hosted frontend always answers for its DI");
    assert_eq!(reading.chain, c.id.0);
    assert!(!reading.playing, "nothing was armed on this chain");
}

#[test]
fn the_tick_reconciles_only_its_own_chains_loopers() {
    // The write door. Without it a looper that exists in the project never
    // gets a slot in the store, so it can never record — and the door must
    // touch ONE chain: the sibling's store stays empty (`CLAUDE.md` LAW).
    let mine = chain("chain:127:row", vec![looper(1)]);
    let sibling = chain("chain:127:sibling", vec![looper(1)]);
    let runtime = controller(&[mine.clone(), sibling.clone()]);
    let session = Rc::new(RefCell::new(None));
    let control = polling_runtime_control(&runtime, &session);

    assert!(
        statuses(&runtime, &mine.id).is_empty(),
        "no slot exists before the tick reconciles"
    );

    control.reconcile_chain_loopers(&mine);

    assert_eq!(
        statuses(&runtime, &mine.id).len(),
        1,
        "the project's looper must get its slot, or REC captures nothing"
    );
    assert!(
        statuses(&runtime, &sibling.id).is_empty(),
        "a reconcile belongs to the chain it names — the sibling is untouched"
    );
}
