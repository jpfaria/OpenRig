//! Responsibility: decides the bus layout a block hands on down the chain.

use block_core::{AudioChannelLayout, ModelAudioMode};

/// The chain's bus layout after a block whose model declares `audio_mode`,
/// fed a bus of layout `bus`; `None` when the block cannot run on it.
///
/// A true-stereo block on a mono bus gets the bus broadcast to both of its
/// inputs, and the bus is stereo from there on — invariant #5: the stream is
/// stereo inside, a mono output is a mixdown at the end (#696, #992). Every
/// other case is the model's own rule.
pub fn bus_layout_after(
    audio_mode: ModelAudioMode,
    bus: AudioChannelLayout,
) -> Option<AudioChannelLayout> {
    match (audio_mode, bus) {
        (ModelAudioMode::TrueStereo, AudioChannelLayout::Mono) => Some(AudioChannelLayout::Stereo),
        _ => audio_mode.output_layout(bus),
    }
}

#[cfg(test)]
#[path = "chain_bus_layout_tests.rs"]
mod tests;
