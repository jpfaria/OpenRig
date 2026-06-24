//! Issue #672 — `build_chain_runtime` is the worker-runnable, `Send` entry that
//! produces fresh `Arc<ChainRuntimeState>`s off the frontend thread. It wraps
//! the heavy per-input runtime assembly (NAM loads, segment + route assembly)
//! behind an owned `Send` `BuildRequest` so `ControlWorker` can run it.
//!
//! A chain with input entries builds one isolated runtime per input port; a
//! chain with no I/O blocks builds NONE.

use domain::ids::{BlockId, ChainId, DeviceId};
use infra_cpal::{build_chain_runtime, BuildRequest};
use project::block::{AudioBlock, AudioBlockKind, InputBlock, InputEntry, OutputBlock, OutputEntry};
use project::chain::{Chain, ChainInputMode, ChainOutputMode};

fn bound_chain(id: &str) -> Chain {
    Chain {
        id: ChainId(id.into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks: vec![
            AudioBlock {
                id: BlockId("in".into()),
                enabled: true,
                kind: AudioBlockKind::Input(InputBlock {
                    model: "standard".into(),
                    io: String::new(),
                    endpoint: String::new(),
                    entries: vec![InputEntry {
                        device_id: DeviceId("dev".into()),
                        mode: ChainInputMode::Mono,
                        channels: vec![0],
                    }],
                }),
            },
            AudioBlock {
                id: BlockId("out".into()),
                enabled: true,
                kind: AudioBlockKind::Output(OutputBlock {
                    model: "standard".into(),
                    io: String::new(),
                    endpoint: String::new(),
                    entries: vec![OutputEntry {
                        device_id: DeviceId("dev".into()),
                        mode: ChainOutputMode::Mono,
                        channels: vec![0],
                    }],
                }),
            },
        ],
    }
}

#[test]
fn build_chain_runtime_produces_a_runnable_runtime() {
    let req = BuildRequest {
        chain: bound_chain("c"),
        sample_rate: 48_000.0,
        buffer_sizes: vec![1024],
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
