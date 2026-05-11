//! `AudioChannelLayout` type helpers — conversion + diagnostics.
//!
//! Setup-time helpers, NOT audio thread. `layout_from_channels` is
//! used during chain construction to turn a CPAL channel count into a
//! typed layout; `layout_label` is used by structured logging /
//! diagnostics.

use anyhow::{anyhow, Result};
use block_core::AudioChannelLayout;

/// Map a raw channel count from a CPAL device or chain config to the
/// typed `AudioChannelLayout`. We only support mono and stereo today;
/// 5.1 / 7.1 / multichannel layouts would need separate handling.
#[allow(dead_code)]
pub(crate) fn layout_from_channels(channel_count: usize) -> Result<AudioChannelLayout> {
    match channel_count {
        1 => Ok(AudioChannelLayout::Mono),
        2 => Ok(AudioChannelLayout::Stereo),
        other => Err(anyhow!(
            "only mono and stereo are supported right now; got {} channels",
            other
        )),
    }
}

pub(crate) fn layout_label(layout: AudioChannelLayout) -> &'static str {
    match layout {
        AudioChannelLayout::Mono => "mono",
        AudioChannelLayout::Stereo => "stereo",
    }
}
