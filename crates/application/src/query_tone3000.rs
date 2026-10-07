//! Responsibility: serializes the TONE3000 browser state for a read.
//!
//! The snapshot holds no key, so nothing here can leak it (#879).

use crate::tone3000_state::Tone3000Snapshot;

pub fn tone3000_state_json(snapshot: &Tone3000Snapshot) -> String {
    serde_json::to_string(snapshot).unwrap_or_else(|_| "{}".to_string())
}
