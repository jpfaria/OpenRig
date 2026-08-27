//! Responsibility: summarises the stream shape a rebuild decision compares.
//!
//! Split out of `resolved.rs` (#873). Deriving a signature from a chain is
//! [`crate::stream_signature`]; this file is only the shape and the predicate
//! that reads it.

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InputStreamSignature {
    pub(crate) device_id: String,
    pub(crate) channels: Vec<usize>,
    pub(crate) stream_channels: u16,
    pub(crate) sample_rate: u32,
    pub(crate) buffer_size_frames: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OutputStreamSignature {
    pub(crate) device_id: String,
    pub(crate) channels: Vec<usize>,
    pub(crate) stream_channels: u16,
    pub(crate) sample_rate: u32,
    pub(crate) buffer_size_frames: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ChainStreamSignature {
    pub(crate) inputs: Vec<InputStreamSignature>,
    pub(crate) outputs: Vec<OutputStreamSignature>,
}

/// Linux/JACK decision: when does a stream-signature delta require tearing
/// down and rebuilding the JACK `AsyncClient`, versus just updating the
/// engine runtime in place?
///
/// Only port-shape changes (device id, sample rate, buffer size, device
/// total-channel count, or adding/removing I/O entries) demand a new
/// client. Selecting a different channel index on the same device is not
/// a port-shape change — ports are pre-registered for the whole device,
/// and the engine picks which index to read or write each callback.
///
/// Returning true here is what triggers `teardown_active_chain_for_rebuild`,
/// and each teardown+rebuild risks the libjack "Cannot open shm segment"
/// corruption documented in issue #294 / #308, so keep this predicate as
/// narrow as the ports actually require.
#[cfg(all(target_os = "linux", feature = "jack"))]
pub(crate) fn stream_signatures_require_client_rebuild(
    old: &ChainStreamSignature,
    new: &ChainStreamSignature,
) -> bool {
    if old.inputs.len() != new.inputs.len() || old.outputs.len() != new.outputs.len() {
        return true;
    }
    // buffer_size_frames intentionally excluded — a soft resize via
    // jack_set_buffer_size changes n_frames on the live client without
    // rebuilding. The JackProcessHandler pre-allocates its ring buffer and
    // scratch at MAX_JACK_FRAMES so larger callbacks don't require a realloc
    // in the RT path. sample_rate still forces a rebuild because jackd has
    // no live SR change API.
    for (a, b) in old.inputs.iter().zip(new.inputs.iter()) {
        if a.device_id != b.device_id
            || a.stream_channels != b.stream_channels
            || a.sample_rate != b.sample_rate
        {
            return true;
        }
    }
    for (a, b) in old.outputs.iter().zip(new.outputs.iter()) {
        if a.device_id != b.device_id
            || a.stream_channels != b.stream_channels
            || a.sample_rate != b.sample_rate
        {
            return true;
        }
    }
    false
}

/// Upper bound we size the JACK-side ring buffer + scratch against, so a
/// `jack_set_buffer_size` that grows `n_frames` at runtime does not force a
/// reallocation in the real-time process callback. The Settings UI today
/// exposes buffer sizes up to 1024 frames; 2048 gives a 2x headroom while
/// keeping the per-chain allocation bounded (~512 KB for 2048 × 8 ch ×
/// 4 bytes × 8 slots). This constant is the ONLY place the cap is
/// enforced — bump it if the UI ever exposes a larger selection.
#[cfg(all(target_os = "linux", feature = "jack"))]
pub(crate) const MAX_JACK_FRAMES: usize = 2048;
