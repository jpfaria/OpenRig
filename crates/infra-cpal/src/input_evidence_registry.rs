//! Responsibility: finds the live evidence rings of a chain.
//!
//! A ring lives as long as its stream's callback holds it; the registry only
//! keeps a weak handle, so a stream that dies takes its ring with it. Touched
//! when a stream is built and when a mark is taken — never on the audio
//! thread.

use std::sync::{Arc, Mutex, Weak};

use crate::input_evidence::InputStreamIdentity;
use crate::input_evidence_ring::InputEvidenceRing;

static RINGS: Mutex<Vec<Weak<InputEvidenceRing>>> = Mutex::new(Vec::new());

/// A ring for a stream being built, findable by its chain while the stream
/// lives.
#[cfg_attr(all(target_os = "linux", feature = "jack"), allow(dead_code))]
pub(crate) fn open_ring(identity: InputStreamIdentity) -> Arc<InputEvidenceRing> {
    let ring = Arc::new(InputEvidenceRing::new(identity));
    register(&ring);
    ring
}

/// Make a new stream's ring findable by its chain.
pub(crate) fn register(ring: &Arc<InputEvidenceRing>) {
    let mut rings = RINGS.lock().unwrap_or_else(|e| e.into_inner());
    rings.retain(|r| r.strong_count() > 0);
    rings.push(Arc::downgrade(ring));
}

/// The rings of `chain_id` whose streams are still alive, in input order.
pub(crate) fn live_rings(chain_id: &str) -> Vec<Arc<InputEvidenceRing>> {
    let mut rings = RINGS.lock().unwrap_or_else(|e| e.into_inner());
    rings.retain(|r| r.strong_count() > 0);
    let mut live: Vec<Arc<InputEvidenceRing>> = rings
        .iter()
        .filter_map(Weak::upgrade)
        .filter(|r| r.identity().chain_id == chain_id)
        .collect();
    live.sort_by_key(|r| r.identity().input_index);
    live
}

#[cfg(test)]
#[path = "input_evidence_registry_tests.rs"]
mod tests;
