//! Issue #672 — `LiveRuntimeSlot` publishes a new runtime that the audio-side
//! `load()` observes wait-free, while the old `Arc` is handed back for an
//! off-thread drop.

use std::sync::Arc;

use domain::ids::ChainId;
use engine::runtime::build_chain_runtime_state;
use infra_cpal::LiveRuntimeSlot;
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
fn publish_swaps_runtime_and_returns_old_for_drop() {
    let first =
        Arc::new(build_chain_runtime_state(&empty_chain("c"), 48_000.0, &[1024]).unwrap());
    let slot = LiveRuntimeSlot::new(Arc::clone(&first));

    // Audio-side load returns the current runtime (wait-free).
    let loaded = slot.load();
    assert!(
        Arc::ptr_eq(&loaded, &first),
        "load() must see the published runtime"
    );
    drop(loaded);

    let second =
        Arc::new(build_chain_runtime_state(&empty_chain("c"), 48_000.0, &[1024]).unwrap());
    let second_addr = Arc::as_ptr(&second) as usize;
    let old = slot.publish(second);

    assert!(
        Arc::ptr_eq(&old, &first),
        "publish must return the previous runtime for off-thread drop"
    );
    assert_eq!(
        Arc::as_ptr(&slot.load()) as usize,
        second_addr,
        "load() now sees the new runtime"
    );
}
