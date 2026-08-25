//! Responsibility: states how many channels a model reads.
//!
//! Split out of `audio_types.rs` (#873). Pure type definitions with a few
//! `const fn` predicates — no state, no I/O.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioChannelLayout {
    Mono,
    Stereo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ModelAudioMode {
    MonoOnly,
    DualMono,
    TrueStereo,
    MonoToStereo,
}

impl ModelAudioMode {
    pub const fn accepts_input(self, layout: AudioChannelLayout) -> bool {
        matches!(
            (self, layout),
            (Self::MonoOnly, AudioChannelLayout::Mono)
                | (Self::DualMono, AudioChannelLayout::Mono)
                | (Self::DualMono, AudioChannelLayout::Stereo)
                | (Self::TrueStereo, AudioChannelLayout::Stereo)
                | (Self::MonoToStereo, AudioChannelLayout::Mono)
                | (Self::MonoToStereo, AudioChannelLayout::Stereo)
        )
    }

    pub const fn output_layout(self, input: AudioChannelLayout) -> Option<AudioChannelLayout> {
        match self {
            Self::MonoOnly => match input {
                AudioChannelLayout::Mono => Some(AudioChannelLayout::Mono),
                AudioChannelLayout::Stereo => None,
            },
            Self::DualMono => Some(input),
            Self::TrueStereo => match input {
                AudioChannelLayout::Stereo => Some(AudioChannelLayout::Stereo),
                AudioChannelLayout::Mono => None,
            },
            Self::MonoToStereo => Some(AudioChannelLayout::Stereo),
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MonoOnly => "mono_only",
            Self::DualMono => "dual_mono",
            Self::TrueStereo => "true_stereo",
            Self::MonoToStereo => "mono_to_stereo",
        }
    }
}
