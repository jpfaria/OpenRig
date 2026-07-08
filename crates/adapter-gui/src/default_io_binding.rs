//! Helper for building the "default" I/O binding from a chosen input and
//! output device pair (#716, Task 13).
//!
//! Called by:
//! - `device_settings_wiring` (wizard finish / set-default-device flow)
//!
//! Task 20 (project-side default binding) can reuse `build_default_io_binding`
//! directly — it is `pub(crate)` here but can be re-exported or moved to a
//! shared crate if the dependency boundary requires it.

use domain::ids::DeviceId;
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};

/// The canonical id used for the system-default I/O binding.
pub(crate) const DEFAULT_BINDING_ID: &str = "default";

/// Build an [`IoBinding`] with id `"default"` from the given input and output
/// device ids.
///
/// Endpoint conventions:
/// - One input endpoint `"In1"` on `input_device_id`, channel 0, Mono (single
///   guitar channel — the most common first-run case).
/// - One output endpoint `"Out1"` on `output_device_id`, channels [0, 1],
///   Stereo (monitor-ready).
///
/// Both Task 13 (wizard finish) and Task 20 (project-side default) call this
/// to derive the binding from whatever the audio wizard or system settings
/// currently have selected.
pub(crate) fn build_default_io_binding(input_device_id: &str, output_device_id: &str) -> IoBinding {
    IoBinding {
        id: DEFAULT_BINDING_ID.to_string(),
        name: "Default".to_string(),
        inputs: vec![IoEndpoint {
            name: "In1".to_string(),
            device_id: DeviceId(input_device_id.to_string()),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs: vec![IoEndpoint {
            name: "Out1".to_string(),
            device_id: DeviceId(output_device_id.to_string()),
            mode: ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    }
}
