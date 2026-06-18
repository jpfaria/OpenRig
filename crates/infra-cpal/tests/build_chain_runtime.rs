//! Issue #672 — `build_chain_runtime` is the worker-runnable, `Send` entry that
//! produces fresh `Arc<ChainRuntimeState>`s off the frontend thread. It wraps
//! the heavy `engine::runtime::build_per_input_runtime_states` (NAM loads,
//! segment + route assembly) behind an owned `Send` `BuildRequest` so
//! `ControlWorker` can run it. Issue #703: one runtime per input-entry group.

use domain::ids::ChainId;
use infra_cpal::{build_chain_runtime, BuildRequest};
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
fn build_chain_runtime_produces_a_runnable_runtime() {
    let req = BuildRequest {
        chain: empty_chain("c"),
        sample_rate: 48_000.0,
        buffer_sizes: vec![1024],
        io_bindings: Vec::new(),
    };
    let runtimes = build_chain_runtime(&req).expect("build must succeed for an empty chain");
    assert_eq!(
        runtimes.len(),
        1,
        "an empty chain produces exactly one (fallback-input) runtime"
    );
    assert!(
        !runtimes[0].1.is_draining(),
        "a freshly built runtime starts active (not draining)"
    );
}

#[test]
fn build_request_is_send() {
    // The build payload must cross to the worker thread.
    fn assert_send<T: Send>() {}
    assert_send::<BuildRequest>();
}
