//! Responsibility: picks the stream format an auxiliary output opens its device with.
//!
//! On CoreAudio the buffer size is a device property shared by every stream
//! of the process, so an auxiliary output (backing-track player, metronome,
//! drums) asking for any other size re-sizes the device under the live chain
//! (#979). It asks for exactly what the chain's own streams ask for.

use project::device::DeviceSettings;

use crate::stream_rates::UNSET_BUFFER_SIZE_FRAMES;

/// The format an auxiliary output asks its device for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AuxStreamFormat {
    pub(crate) sample_rate: u32,
    pub(crate) buffer_frames: u32,
}

/// The project's settings for `device_id` when it has them, otherwise the
/// device's own rate and the buffer a chain stream falls back to.
pub(crate) fn aux_stream_format(
    device_settings: &[DeviceSettings],
    device_id: &str,
    device_default_rate: u32,
) -> AuxStreamFormat {
    match device_settings.iter().find(|s| s.device_id.0 == device_id) {
        Some(settings) => AuxStreamFormat {
            sample_rate: settings.sample_rate,
            buffer_frames: settings.buffer_size_frames,
        },
        None => AuxStreamFormat {
            sample_rate: device_default_rate,
            buffer_frames: UNSET_BUFFER_SIZE_FRAMES,
        },
    }
}
