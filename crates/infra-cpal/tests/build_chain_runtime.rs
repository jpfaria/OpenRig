//! Issue #672 — `build_chain_runtime` is the worker-runnable, `Send` entry that
//! produces fresh `Arc<ChainRuntimeState>`s off the frontend thread. It wraps
//! the heavy per-binding runtime assembly (NAM loads, segment + route assembly)
//! behind an owned `Send` `BuildRequest` so `ControlWorker` can run it.
//!
//! Clean break (#716): routing is binding-only. A BOUND chain builds one
//! isolated runtime per input port; an UNBOUND chain (empty `io`) builds NONE.

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use infra_cpal::{build_chain_runtime, BuildRequest};
use project::block::{AudioBlock, AudioBlockKind, InputBlock, OutputBlock};
use project::chain::Chain;

fn one_binding() -> Vec<IoBinding> {
    vec![IoBinding {
        id: "io".into(),
        name: "Interface".into(),
        inputs: vec![IoEndpoint {
            name: "in".into(),
            device_id: DeviceId("dev".into()),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs: vec![IoEndpoint {
            name: "out".into(),
            device_id: DeviceId("dev".into()),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
    }]
}

fn bound_chain(id: &str) -> Chain {
    Chain {
        id: ChainId(id.into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        blocks: vec![
            AudioBlock {
                id: BlockId("in".into()),
                enabled: true,
                kind: AudioBlockKind::Input(InputBlock {
                    model: "standard".into(),
                    io: "io".into(),
                    endpoint: "in".into(),
                    entries: Vec::new(),
                }),
            },
            AudioBlock {
                id: BlockId("out".into()),
                enabled: true,
                kind: AudioBlockKind::Output(OutputBlock {
                    model: "standard".into(),
                    io: "io".into(),
                    endpoint: "out".into(),
                    entries: Vec::new(),
                }),
            },
        ],
    }
}

fn unbound_chain(id: &str) -> Chain {
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
        chain: bound_chain("c"),
        sample_rate: 48_000.0,
        buffer_sizes: vec![1024],
        io_bindings: one_binding(),
    };
    let runtimes = build_chain_runtime(&req).expect("build must succeed for a bound chain");
    assert_eq!(
        runtimes.len(),
        1,
        "a single-binding chain produces exactly one isolated input runtime"
    );
    assert!(
        !runtimes[0].1.is_draining(),
        "a freshly built runtime starts active (not draining)"
    );
}

#[test]
fn build_chain_runtime_unbound_chain_produces_no_runtime() {
    // Clean break (#716): routing is binding-only — an unbound chain builds
    // no runtime instead of a legacy all-to-all fallback. No error: the chain
    // simply opens unbound and must be reconfigured via the registry.
    let req = BuildRequest {
        chain: unbound_chain("c"),
        sample_rate: 48_000.0,
        buffer_sizes: vec![1024],
        io_bindings: Vec::new(),
    };
    let runtimes = build_chain_runtime(&req).expect("unbound chain must build cleanly (empty)");
    assert!(
        runtimes.is_empty(),
        "unbound chain must produce NO runtime, got {}",
        runtimes.len()
    );
}

#[test]
fn build_request_is_send() {
    // The build payload must cross to the worker thread.
    fn assert_send<T: Send>() {}
    assert_send::<BuildRequest>();
}
