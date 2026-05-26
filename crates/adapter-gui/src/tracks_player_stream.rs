//! #553 — minimal cpal output stream for track playback.
//!
//! Opens the system default output device and drains a
//! [`feature_tracks::MultiStemPlayer`] into it. Lives outside the main
//! rig engine so toggling Tracks playback never touches the RT chain
//! used by the guitar signal: stem playback is its own stream against
//! the OS host, and the user routes them together at the hardware
//! mixer level if they want a single bus.
//!
//! The stream is built around the player's own `process(&mut [f32])`
//! call, so every existing invariant in `MultiStemPlayer` (atomic
//! per-stem state, zero alloc / lock / syscall on the audio thread,
//! solo precedence, linear pan) is preserved.

use std::sync::Arc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Stream, StreamConfig};
use feature_tracks::MultiStemPlayer;

/// Wraps an open output stream tied to a [`MultiStemPlayer`].
///
/// Drop the wrapper to stop and release the audio device.
pub(crate) struct TrackPlaybackStream {
    _stream: Stream,
}

impl TrackPlaybackStream {
    /// Open the OS default output device, build a stereo `f32` stream
    /// that drains `player`, and start it.
    ///
    /// # Errors
    ///
    /// Surfaces any cpal failure as a plain string (no device, no
    /// stereo support, build failure, etc.). The caller decides
    /// whether to surface this to the user.
    pub fn start(player: Arc<MultiStemPlayer>) -> Result<Self, String> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| "no default output device".to_string())?;
        let supported = device
            .default_output_config()
            .map_err(|err| err.to_string())?;

        // Force a stereo `f32` config; every modern OS supports it.
        let mut config: StreamConfig = supported.clone().into();
        config.channels = 2;

        let err_fn = |err| eprintln!("cpal: stream error: {err}");
        let stream = device
            .build_output_stream(
                &config,
                move |buf: &mut [f32], _info| {
                    player.process(buf);
                },
                err_fn,
                None,
            )
            .map_err(|err| err.to_string())?;

        stream.play().map_err(|err| err.to_string())?;
        Ok(Self { _stream: stream })
    }
}
