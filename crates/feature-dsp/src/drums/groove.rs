//! Responsibility: describes one drum groove with its fills.

use super::pattern::DrumPattern;

#[derive(Clone, Debug, PartialEq)]
pub struct Groove {
    pub id: String,
    pub name: String,
    pub genre: String,
    pub beats_per_bar: u32,
    /// The tempo the groove was played at; a starting point for the user.
    pub tempo: f32,
    /// The main loop; may span several bars.
    pub beat: DrumPattern,
    /// One-bar fills, used in turn.
    pub fills: Vec<DrumPattern>,
}
