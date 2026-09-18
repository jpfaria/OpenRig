//! #957: a preset switch installs fresh runtimes for a live chain with the
//! same stream count. The controller's runtime identity is what lets the
//! meter see the swap and re-subscribe on the runtimes that now play.

#![cfg(test)]

use std::sync::Arc;

use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use project::chain::Chain;

use engine::runtime::{build_chain_runtime_state, RuntimeGraph};

use super::ProjectRuntimeController;

fn chain() -> Chain {
    Chain {
        id: ChainId("c".into()),
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
            mode: ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    }]
}

fn runtime() -> Arc<engine::runtime::ChainRuntimeState> {
    Arc::new(build_chain_runtime_state(&chain(), 48_000.0, &[256], &registry()).unwrap())
}

#[test]
fn replacing_a_chains_runtime_changes_its_identity_but_not_its_stream_count() {
    let c = chain();
    let mut graph = RuntimeGraph {
        chains: std::collections::HashMap::new(),
    };
    graph.chains.insert((c.id.clone(), 0), runtime());
    let mut controller = ProjectRuntimeController::for_testing(graph);
    let before = controller.runtime_identity(&c.id);
    let count_before = controller.stream_count(&c.id);

    // What an activation / off-thread rebuild install does to the graph.
    controller
        .runtime_graph
        .chains
        .insert((c.id.clone(), 0), runtime());

    assert_eq!(controller.stream_count(&c.id), count_before);
    assert_ne!(
        controller.runtime_identity(&c.id),
        before,
        "the chain now runs another runtime; its identity must say so"
    );
}

#[test]
fn a_chain_without_runtime_has_no_identity() {
    let controller = ProjectRuntimeController::for_testing(RuntimeGraph {
        chains: std::collections::HashMap::new(),
    });
    assert_eq!(controller.runtime_identity(&ChainId("c".into())), 0);
}
