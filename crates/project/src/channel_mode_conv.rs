//! Compiler-enforced bridge between `domain::io_binding::ChannelMode` and the
//! chain-layout enums (`ChainInputMode`, `ChainOutputMode`).
//!
//! `ChannelMode` is the registry vocabulary (domain crate, single source of
//! truth for physical-endpoint descriptions). The chain enums drive the
//! runtime processing layout. These conversions make the relationship
//! compiler-checked: adding a new `ChannelMode` variant forces an update here.

use domain::io_binding::ChannelMode;

use crate::chain::{ChainInputMode, ChainOutputMode};

// ── From<ChannelMode> for ChainInputMode ────────────────────────────────────
// Exhaustive match — every ChannelMode variant must map to a ChainInputMode.

impl From<ChannelMode> for ChainInputMode {
    fn from(mode: ChannelMode) -> Self {
        match mode {
            ChannelMode::Mono => ChainInputMode::Mono,
            ChannelMode::Stereo => ChainInputMode::Stereo,
            ChannelMode::DualMono => ChainInputMode::DualMono,
        }
    }
}

// ── TryFrom<ChannelMode> for ChainOutputMode ────────────────────────────────
// Outputs have no dual-mono layout — DualMono returns Err.

/// Error returned when a `ChannelMode` cannot be represented as a
/// `ChainOutputMode` (specifically: `DualMono` has no output equivalent).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChannelModeConvError {
    /// The mode that could not be converted.
    pub source: ChannelMode,
}

impl std::fmt::Display for ChannelModeConvError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "ChannelMode::{:?} has no ChainOutputMode equivalent \
             (outputs do not support dual-mono)",
            self.source
        )
    }
}

impl std::error::Error for ChannelModeConvError {}

impl TryFrom<ChannelMode> for ChainOutputMode {
    type Error = ChannelModeConvError;

    fn try_from(mode: ChannelMode) -> Result<Self, Self::Error> {
        match mode {
            ChannelMode::Mono => Ok(ChainOutputMode::Mono),
            ChannelMode::Stereo => Ok(ChainOutputMode::Stereo),
            // Outputs have no dual-mono layout.
            ChannelMode::DualMono => Err(ChannelModeConvError { source: mode }),
        }
    }
}

// The forward direction (`From<ChainInputMode>/From<ChainOutputMode> for
// ChannelMode`) was only consumed by the legacy-entries migration, which the
// clean break (#716) removed. The binding resolution path needs only the
// reverse direction (`From<ChannelMode> for ChainInputMode` and
// `TryFrom<ChannelMode> for ChainOutputMode`), kept above.

#[cfg(test)]
#[path = "channel_mode_conv_tests.rs"]
mod tests;
