//! Issue #672 — `build_chain_slots` wraps each per-group runtime in a
//! `LiveRuntimeSlot` so the stream callbacks read through the slot (live-
//! swappable) and the controller stores the same slots for the worker to
//! publish into.

use std::sync::Arc;

use domain::ids::ChainId;
use engine::runtime::build_chain_runtime_state;
use infra_cpal::{build_chain_slots, LiveRuntimeSlot};
use project::chain::Chain;

fn empty_chain(id: &str) -> Chain {
    Chain {
        id: ChainId(id.into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        blocks: vec![],
    }
}

#[test]
fn build_chain_slots_makes_one_slot_per_group_runtime() {
    let chain = empty_chain("c");
    let rt0 = Arc::new(build_chain_runtime_state(&chain, 48_000.0, &[256]).unwrap());
    let rt1 = Arc::new(build_chain_runtime_state(&chain, 48_000.0, &[256]).unwrap());

    let runtimes = vec![(0_usize, Arc::clone(&rt0)), (1_usize, Arc::clone(&rt1))];
    let slots: Vec<(usize, LiveRuntimeSlot)> = build_chain_slots(&runtimes);

    assert_eq!(slots.len(), 2, "one slot per group runtime");
    assert_eq!(slots[0].0, 0);
    assert_eq!(slots[1].0, 1);
    assert!(
        Arc::ptr_eq(&slots[0].1.load(), &rt0),
        "slot for group 0 holds its runtime"
    );
    assert!(
        Arc::ptr_eq(&slots[1].1.load(), &rt1),
        "slot for group 1 holds its runtime"
    );
}
