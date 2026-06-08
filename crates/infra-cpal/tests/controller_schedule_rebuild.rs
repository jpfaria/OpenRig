//! Issue #672 — `ProjectRuntimeController::schedule_chain_rebuild` rebuilds a
//! chain's runtime on the control worker and publishes it, without blocking the
//! caller. The new runtime replaces the old one in the live graph; the public
//! `chain_runtime` accessor observes the swap.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use domain::ids::ChainId;
use engine::runtime::{build_chain_runtime_state, RuntimeGraph};
use infra_cpal::ProjectRuntimeController;
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
fn schedule_chain_rebuild_publishes_a_new_runtime_offthread() {
    let chain_id = ChainId("chain:672:rebuild".into());
    let chain = empty_chain(&chain_id.0);

    let initial = Arc::new(build_chain_runtime_state(&chain, 48_000.0, &[1024]).unwrap());
    let mut chains = HashMap::new();
    chains.insert((chain_id.clone(), 0_usize), Arc::clone(&initial));
    let graph = RuntimeGraph { chains };

    let mut controller = ProjectRuntimeController::for_testing(graph);

    let before = controller
        .chain_runtime(&chain_id)
        .expect("chain runtime is present before the rebuild");
    assert!(Arc::ptr_eq(&before, &initial));

    // Schedule the rebuild — must return immediately with a completion handle.
    let done = controller.schedule_chain_rebuild(&chain, 48_000.0, vec![1024]);
    done.recv_timeout(Duration::from_secs(10))
        .expect("rebuild completes")
        .expect("rebuild succeeds");

    let after = controller
        .chain_runtime(&chain_id)
        .expect("chain runtime is present after the rebuild");
    assert!(
        !Arc::ptr_eq(&before, &after),
        "schedule_chain_rebuild must publish a freshly built runtime (new Arc)"
    );
}
