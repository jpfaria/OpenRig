use std::sync::Arc;
use std::time::SystemTime;

use super::{live_rings, register};
use crate::input_evidence::InputStreamIdentity;
use crate::input_evidence_ring::InputEvidenceRing;

fn ring(chain: &str, input_index: usize) -> Arc<InputEvidenceRing> {
    Arc::new(InputEvidenceRing::with_capacity(
        InputStreamIdentity {
            chain_id: chain.into(),
            input_index,
            device_id: None,
            sample_rate: 48_000,
            buffer_frames: 64,
            channels: 1,
            opened_at: SystemTime::UNIX_EPOCH,
        },
        8,
        4,
    ))
}

#[test]
fn a_chain_sees_only_its_own_rings_in_input_order() {
    let a1 = ring("chain:registry:a", 1);
    let a0 = ring("chain:registry:a", 0);
    let b0 = ring("chain:registry:b", 0);
    register(&a1);
    register(&a0);
    register(&b0);

    let rings = live_rings("chain:registry:a");

    let order: Vec<usize> = rings.iter().map(|r| r.identity().input_index).collect();
    assert_eq!(order, vec![0, 1]);
    assert!(rings
        .iter()
        .all(|r| r.identity().chain_id == "chain:registry:a"));
}

#[test]
fn a_ring_whose_stream_died_is_gone() {
    let alive = ring("chain:registry:dead", 0);
    let dead = ring("chain:registry:dead", 1);
    register(&alive);
    register(&dead);
    drop(dead);

    let rings = live_rings("chain:registry:dead");

    assert_eq!(rings.len(), 1);
    assert_eq!(rings[0].identity().input_index, 0);
}
