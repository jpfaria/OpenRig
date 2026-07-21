use super::*;
use domain::io_binding::ChannelMode;

// ── From<ChannelMode> for ChainInputMode ────────────────────────────────

#[test]
fn channel_mode_mono_converts_to_chain_input_mono() {
    assert_eq!(
        ChainInputMode::from(ChannelMode::Mono),
        ChainInputMode::Mono
    );
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
