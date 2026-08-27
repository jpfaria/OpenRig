//! Responsibility: builds an endpoint out of what the editor collected.

use domain::ids::DeviceId;
use domain::io_binding::{ChannelMode, IoEndpoint};

/// Build an input `IoEndpoint` from the structured picker inputs.
pub(crate) fn build_input_endpoint(
    name: &str,
    device_id: &str,
    channels: Vec<usize>,
    mode: ChannelMode,
) -> IoEndpoint {
    IoEndpoint {
        name: name.to_string(),
        device_id: DeviceId(device_id.to_string()),
        mode,
        channels,
    }
}

/// Build an output `IoEndpoint` from the structured picker inputs. Symmetric
/// to [`build_input_endpoint`]; the output picker constrains `mode` to
/// mono/stereo at the UI layer.
pub(crate) fn build_output_endpoint(
    name: &str,
    device_id: &str,
    channels: Vec<usize>,
    mode: ChannelMode,
) -> IoEndpoint {
    IoEndpoint {
        name: name.to_string(),
        device_id: DeviceId(device_id.to_string()),
        mode,
        channels,
    }
}
