//! Issue #672 — `request_offthread_rebuild_if_live` returns false for a chain
//! that is not currently streaming, so the caller falls back to the synchronous
//! build (cold activation). The off-thread path only applies to a live chain
//! whose IO is unchanged (model swap), which needs real devices to exercise and
//! is validated on hardware.

use std::collections::HashMap;

use domain::ids::ChainId;
use engine::runtime::RuntimeGraph;
use infra_cpal::ProjectRuntimeController;
use project::chain::Chain;
use project::project::Project;

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
fn returns_false_when_chain_is_not_live() {
    let mut controller = ProjectRuntimeController::for_testing(RuntimeGraph {
        chains: HashMap::new(),
    });
    let project = Project {
        name: None,
        device_settings: vec![],
        chains: vec![],
        midi: None,
    };
    let chain = empty_chain("not-live");

    let scheduled = controller
        .request_offthread_rebuild_if_live(&project, &chain)
        .expect("query must not error for a non-live chain");
    assert!(
        !scheduled,
        "a chain with no active streams must fall back to the synchronous build"
    );
}
