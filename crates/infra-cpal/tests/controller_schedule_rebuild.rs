//! Issue #672 — `ProjectRuntimeController::schedule_chain_rebuild` enqueues an
//! off-thread runtime build; `poll_pending_rebuilds` (called on the frontend
//! tick) applies the finished build by swapping the live slot AND the
//! runtime_graph in lock-step, so both stay consistent. The heavy build never
//! blocks the caller.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use engine::runtime::{build_chain_runtime_state, RuntimeGraph};
use infra_cpal::ProjectRuntimeController;
use project::chain::Chain;

/// Model A (#716): a mono-in/mono-out chain that selects the "io" binding;
/// its device endpoints live in `io_registry`, not block `entries`.
fn bound_chain(id: &str) -> Chain {
    Chain {
        id: ChainId(id.into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks: vec![],
    }
}

fn io_registry() -> Vec<IoBinding> {
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

#[test]
fn schedule_then_poll_publishes_a_new_runtime_offthread() {
    let chain_id = ChainId("chain:672:rebuild".into());
    let chain = bound_chain(&chain_id.0);
    let registry = io_registry();

    let initial = Arc::new(build_chain_runtime_state(&chain, 48_000.0, &[1024], &registry).unwrap());
    let mut chains = HashMap::new();
    chains.insert((chain_id.clone(), 0_usize), Arc::clone(&initial));
    let graph = RuntimeGraph { chains };

    let mut controller = ProjectRuntimeController::for_testing(graph);
    // The off-thread rebuild resolves device endpoints from the controller's
    // binding registry (#716); install it so the rebuild matches the chain.
    controller.set_io_bindings(registry);

    let before = controller
        .chain_runtime(&chain_id)
        .expect("runtime present");
    assert!(Arc::ptr_eq(&before, &initial));

    // Enqueue the rebuild — must return immediately (no build on this thread).
    controller.schedule_chain_rebuild(&chain, 48_000.0, vec![1024]);

    // The frontend tick drains finished builds; spin the poll until applied.
    let mut applied = 0;
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while applied == 0 {
        applied += controller.poll_pending_rebuilds();
        assert!(
            std::time::Instant::now() < deadline,
            "rebuild never completed"
        );
        std::thread::yield_now();
    }
    assert_eq!(applied, 1);

    let after = controller
        .chain_runtime(&chain_id)
        .expect("runtime present");
    assert!(
        !Arc::ptr_eq(&before, &after),
        "poll must publish the freshly built runtime (new Arc)"
    );
}
