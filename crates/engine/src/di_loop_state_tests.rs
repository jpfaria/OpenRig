//! Tests for the per-chain DI loop state fields on `ChainRuntimeState` (issue #614).

use super::{build_chain_runtime_state, DEFAULT_ELASTIC_TARGET};
use crate::di_loop::DiLoop;
use domain::ids::ChainId;
use project::block::AudioBlock;
use project::chain::Chain;
use std::sync::atomic::Ordering;
use std::sync::Arc;

fn empty_chain() -> Chain {
    Chain {
        id: ChainId("chain:di-test".into()),
        description: Some("DI loop test chain".into()),
        instrument: "electric_guitar".to_string(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks: Vec::<AudioBlock>::new(),
        di_output: None,
    }
}

#[test]
fn set_di_loop_publishes_and_resets_cursor() {
    let chain = empty_chain();
    let runtime: Arc<crate::runtime::ChainRuntimeState> = Arc::new(
        build_chain_runtime_state(&chain, 48_000.0, &[DEFAULT_ELASTIC_TARGET], &[])
            .expect("runtime state should build"),
    );

    // Initially no DI loop is active.
    assert!(!runtime.has_di_loop());

    // Simulate a non-zero cursor (as if playback had advanced).
    runtime.di_loop_pos.store(123, Ordering::Relaxed);

    // Publish a DI loop — cursor must reset to 0.
    let di = Arc::new(DiLoop::from_samples(&[0.0, 0.5, 1.0], 48_000, 1, 48_000, 0));
    runtime.set_di_loop(Some(di));
    assert!(runtime.has_di_loop());
    assert_eq!(runtime.di_loop_pos.load(Ordering::Relaxed), 0);

    // Clearing the DI loop removes it.
    runtime.set_di_loop(None);
    assert!(!runtime.has_di_loop());
}
