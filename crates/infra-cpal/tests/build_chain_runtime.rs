//! Issue #672 — `build_chain_runtime` is the worker-runnable, `Send` entry that
//! produces fresh `Arc<ChainRuntimeState>`s off the frontend thread. It wraps
//! the heavy per-input runtime assembly (NAM loads, segment + route assembly)
//! behind an owned `Send` `BuildRequest` so `ControlWorker` can run it.
//!
//! A chain with input entries builds one isolated runtime per input port; a
//! chain with no I/O blocks builds NONE.

use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use infra_cpal::{build_chain_runtime, BuildRequest};
use project::chain::Chain;

/// Model A (#716): a single mono-in/mono-out chain that selects the "io"
/// binding; the device endpoints live in `io_registry`, not block `entries`.
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
fn build_chain_runtime_produces_a_runnable_runtime() {
    let req = BuildRequest {
        chain: bound_chain("c"),
        sample_rate: 48_000.0,
        buffer_sizes: vec![1024],
        io_bindings: io_registry(),
    };
    let runtimes = build_chain_runtime(&req).expect("build must succeed for a chain with I/O");
    assert_eq!(
        runtimes.len(),
        1,
        "a single-input chain produces exactly one isolated input runtime"
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
