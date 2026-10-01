use std::sync::Arc;
use std::time::SystemTime;

use domain::ids::ChainId;
use engine::runtime::RuntimeGraph;

use crate::input_evidence::InputStreamIdentity;
use crate::input_evidence_registry::register;
use crate::input_evidence_ring::InputEvidenceRing;
use crate::ProjectRuntimeController;

fn ring(chain: &str) -> Arc<InputEvidenceRing> {
    let ring = Arc::new(InputEvidenceRing::with_capacity(
        InputStreamIdentity {
            chain_id: chain.into(),
            input_index: 0,
            device_id: Some("coreaudio:evidence".into()),
            sample_rate: 44_100,
            buffer_frames: 64,
            channels: 2,
            opened_at: SystemTime::UNIX_EPOCH,
        },
        64,
        8,
    ));
    ring.record(&[0.25, -0.25], 42);
    register(&ring);
    ring
}

#[test]
fn the_evidence_of_a_chain_holds_its_streams_and_no_other() {
    let _mine = ring("chain:evidence:mine");
    let _other = ring("chain:evidence:other");
    let controller = ProjectRuntimeController::for_testing(RuntimeGraph {
        chains: Default::default(),
    });

    let evidence = controller.stepped_input_evidence(&ChainId("chain:evidence:mine".into()));

    assert_eq!(evidence.chain_id, "chain:evidence:mine");
    assert_eq!(evidence.streams.len(), 1);
    let stream = &evidence.streams[0];
    assert_eq!(stream.identity.chain_id, "chain:evidence:mine");
    assert_eq!(
        stream.identity.device_id.as_deref(),
        Some("coreaudio:evidence")
    );
    assert_eq!(stream.snapshot.samples, vec![0.25, -0.25]);
    assert_eq!(stream.snapshot.cycles.len(), 1);
}

#[test]
fn a_chain_with_no_live_stream_gives_empty_evidence() {
    let controller = ProjectRuntimeController::for_testing(RuntimeGraph {
        chains: Default::default(),
    });

    let evidence = controller.stepped_input_evidence(&ChainId("chain:evidence:none".into()));

    assert!(evidence.streams.is_empty());
    assert!(evidence.routes.is_empty());
    assert!(!evidence.input_stepped);
}
