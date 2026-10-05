//! Responsibility: holds one drum kit's samples by role.
//!
//! Samples are mono at the kit's `sample_rate`; the engine plays them one to
//! one, so a kit is prepared at the stream's rate before it reaches it.

use std::sync::Arc;

use super::role::DrumRole;

/// One velocity layer: the alternate samples (round-robin) for a velocity
/// range.
#[derive(Clone, Debug, PartialEq)]
pub struct DrumLayer {
    pub min_velocity: f32,
    pub max_velocity: f32,
    pub samples: Vec<Arc<[f32]>>,
    pub gain: f32,
}

/// One instrument of the kit.
#[derive(Clone, Debug, PartialEq)]
pub struct DrumPiece {
    pub layers: Vec<DrumLayer>,
    pub gain: f32,
    /// -1.0 hard left, 0.0 center, 1.0 hard right.
    pub pan: f32,
    /// Pieces sharing a group cut each other off (closed hat chokes open hat).
    pub choke_group: Option<u8>,
}

#[derive(Clone, Debug)]
pub struct DrumKit {
    name: String,
    sample_rate: u32,
    pieces: [Option<DrumPiece>; DrumRole::COUNT],
}

impl DrumKit {
    pub fn new(name: impl Into<String>, sample_rate: u32) -> Self {
        Self {
            name: name.into(),
            sample_rate,
            pieces: std::array::from_fn(|_| None),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn set_piece(&mut self, role: DrumRole, piece: DrumPiece) {
        self.pieces[role.index()] = Some(piece);
    }

    /// The piece mapped to exactly this role.
    pub fn piece(&self, role: DrumRole) -> Option<&DrumPiece> {
        self.pieces[role.index()].as_ref()
    }

    /// The role that actually sounds for `role`, following fallbacks.
    pub fn resolve(&self, role: DrumRole) -> Option<DrumRole> {
        let mut current = Some(role);
        while let Some(r) = current {
            if self.pieces[r.index()].is_some() {
                return Some(r);
            }
            current = r.fallback();
        }
        None
    }

    pub fn piece_for(&self, role: DrumRole) -> Option<&DrumPiece> {
        self.resolve(role).and_then(|r| self.piece(r))
    }
}
