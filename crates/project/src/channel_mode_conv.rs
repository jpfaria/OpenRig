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

// ── From<ChainInputMode> for ChannelMode ────────────────────────────────────
// Lossless: every ChainInputMode has a ChannelMode counterpart.

impl From<ChainInputMode> for ChannelMode {
    fn from(mode: ChainInputMode) -> Self {
        match mode {
            ChainInputMode::Mono => ChannelMode::Mono,
            ChainInputMode::Stereo => ChannelMode::Stereo,
            ChainInputMode::DualMono => ChannelMode::DualMono,
        }
    }
}

// ── From<ChainOutputMode> for ChannelMode ───────────────────────────────────
// Lossless: ChainOutputMode::Mono → ChannelMode::Mono, Stereo → Stereo.
// (ChainOutputMode has no DualMono variant; that lives only on inputs.)

impl From<ChainOutputMode> for ChannelMode {
    fn from(mode: ChainOutputMode) -> Self {
        match mode {
            ChainOutputMode::Mono => ChannelMode::Mono,
            ChainOutputMode::Stereo => ChannelMode::Stereo,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::io_binding::ChannelMode;

    // ── From<ChannelMode> for ChainInputMode ────────────────────────────────

    #[test]
    fn channel_mode_mono_converts_to_chain_input_mono() {
        assert_eq!(ChainInputMode::from(ChannelMode::Mono), ChainInputMode::Mono);
    }

    #[test]
    fn channel_mode_stereo_converts_to_chain_input_stereo() {
        assert_eq!(
            ChainInputMode::from(ChannelMode::Stereo),
            ChainInputMode::Stereo
        );
    }

    #[test]
    fn channel_mode_dual_mono_converts_to_chain_input_dual_mono() {
        assert_eq!(
            ChainInputMode::from(ChannelMode::DualMono),
            ChainInputMode::DualMono
        );
    }

    // ── TryFrom<ChannelMode> for ChainOutputMode ────────────────────────────

    #[test]
    fn channel_mode_mono_converts_to_chain_output_mono() {
        assert_eq!(
            ChainOutputMode::try_from(ChannelMode::Mono).unwrap(),
            ChainOutputMode::Mono
        );
    }

    #[test]
    fn channel_mode_stereo_converts_to_chain_output_stereo() {
        assert_eq!(
            ChainOutputMode::try_from(ChannelMode::Stereo).unwrap(),
            ChainOutputMode::Stereo
        );
    }

    #[test]
    fn channel_mode_dual_mono_fails_for_chain_output_with_clear_error() {
        let err = ChainOutputMode::try_from(ChannelMode::DualMono).unwrap_err();
        assert_eq!(err.source, ChannelMode::DualMono);
        assert!(
            err.to_string().contains("dual-mono"),
            "error message should mention dual-mono: {}",
            err
        );
    }

    // ── From<ChainInputMode> for ChannelMode ────────────────────────────────

    #[test]
    fn chain_input_mono_converts_to_channel_mode_mono() {
        assert_eq!(ChannelMode::from(ChainInputMode::Mono), ChannelMode::Mono);
    }

    #[test]
    fn chain_input_stereo_converts_to_channel_mode_stereo() {
        assert_eq!(
            ChannelMode::from(ChainInputMode::Stereo),
            ChannelMode::Stereo
        );
    }

    #[test]
    fn chain_input_dual_mono_converts_to_channel_mode_dual_mono() {
        assert_eq!(
            ChannelMode::from(ChainInputMode::DualMono),
            ChannelMode::DualMono
        );
    }

    // ── From<ChainOutputMode> for ChannelMode ───────────────────────────────

    #[test]
    fn chain_output_mono_converts_to_channel_mode_mono() {
        assert_eq!(ChannelMode::from(ChainOutputMode::Mono), ChannelMode::Mono);
    }

    #[test]
    fn chain_output_stereo_converts_to_channel_mode_stereo() {
        assert_eq!(
            ChannelMode::from(ChainOutputMode::Stereo),
            ChannelMode::Stereo
        );
    }
}
