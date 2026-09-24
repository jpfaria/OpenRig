//! Responsibility: sizes a stream's buffer request for the host that will serve it.

use cpal::{BufferSize, SupportedBufferSize};

/// The buffer a stream asks for. ASIO accepts only sizes inside, and on the
/// step of, the range its control panel allows (cpal 0.18), and the owner of
/// that setting is the vendor panel, so an ASIO stream takes the driver's own
/// size (#978). Every other host gets the project's size, as before.
pub(crate) fn requested_buffer(host_is_asio: bool, frames: u32) -> BufferSize {
    if host_is_asio {
        BufferSize::Default
    } else {
        BufferSize::Fixed(frames)
    }
}

/// Frames to preallocate for one callback, so the callback never has to grow
/// its buffers (invariant #8). On ASIO the driver picks the size, up to the
/// maximum it reports.
pub(crate) fn callback_capacity_frames(
    host_is_asio: bool,
    requested: u32,
    supported: &SupportedBufferSize,
) -> u32 {
    match (host_is_asio, supported) {
        (true, SupportedBufferSize::Range { max, .. }) => requested.max(*max),
        _ => requested,
    }
}

#[cfg(test)]
#[path = "driver_buffer_tests.rs"]
mod tests;
