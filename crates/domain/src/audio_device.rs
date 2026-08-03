//! Single source of truth for the shape of an audio device, as a frontend
//! sees it.
//!
//! It lives in `domain` for the same reason [`crate::io_binding`] does: two
//! layers need one definition. `infra-cpal` PRODUCES these by enumerating the
//! host (`list_input_device_descriptors` / `list_output_device_descriptors`),
//! and every frontend CONSUMES them — device pickers, endpoint channel counts,
//! binding resolution after a hot-swap. Before #127 the definition sat in
//! `infra-cpal`, so a UI that only wanted to render a device name had to link
//! the CPAL backend, and a remote or Flutter frontend (#43) could not implement
//! the wiring at all. Nothing here is CPAL-specific — it is an id, a label and
//! a channel count — so it belongs on the neutral side of that seam.

/// One audio device the host reported: what to call it, and how wide it is.
///
/// `id` is the stable identifier a binding stores (`IoEndpoint::device_id`);
/// `name` is only ever shown to a human. `channels` is the maximum channel
/// count the device offers in the direction it was enumerated for — an input
/// listing counts input channels, an output listing output channels — so an
/// endpoint's channel indices can be validated against it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioDeviceDescriptor {
    /// Stable identifier of the physical device.
    pub id: String,
    /// Human-readable label as the host reports it.
    pub name: String,
    /// Channel count in the enumerated direction.
    pub channels: usize,
}
