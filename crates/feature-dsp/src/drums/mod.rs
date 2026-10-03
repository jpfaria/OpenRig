//! Responsibility: routes the drum machine's public surface.
//!
//! A play-along drum machine: kits of velocity-layered samples addressed by
//! drum role, grooves of beat-positioned hits, and a sample-accurate engine
//! that renders them into a stereo buffer without allocating.

mod groove;
mod kit;
mod machine;
mod pattern;
mod role;
mod sample_select;
mod sequencer;
mod settings;
mod voice;
mod voice_pool;

pub use groove::Groove;
pub use kit::{DrumKit, DrumLayer, DrumPiece};
pub use machine::DrumMachine;
pub use pattern::{DrumHit, DrumPattern};
pub use role::DrumRole;
pub use sequencer::DrumPosition;
pub use settings::{DrumSettings, MAX_BPM, MIN_BPM};
pub use voice_pool::MAX_VOICES;
