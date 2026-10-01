//! Responsibility: reports the state of one audio device as the OS sees it.
//!
//! Read off the audio thread when a chain's input trips as stepped, and again
//! after its restart, so a mark shows what the device and its other clients
//! were doing. Every field is `None`/empty when the OS does not report it; on
//! systems without a probe only `device_id` is filled.

use serde::Serialize;

/// A stream format (`AudioStreamBasicDescription` on macOS).
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub struct StreamFormat {
    pub sample_rate: f64,
    /// FourCC of the format (`lpcm`).
    pub format_id: String,
    pub flags: u32,
    pub bytes_per_frame: u32,
    pub channels: u32,
    pub bits: u32,
}

/// One input stream of the device.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub struct ProbedStream {
    pub starting_channel: Option<u32>,
    pub latency: Option<u32>,
    /// Format the HAL hands its clients.
    pub virtual_format: Option<StreamFormat>,
    /// Format the hardware runs at.
    pub physical_format: Option<StreamFormat>,
}

/// A process that has the device open.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub struct DeviceClient {
    pub pid: i32,
    pub bundle_id: Option<String>,
    /// Uses the device for input / for output.
    pub input: bool,
    pub output: bool,
    pub running_input: Option<bool>,
    pub running_output: Option<bool>,
}

/// The device at one instant.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub struct DeviceProbe {
    /// The cpal device id that was asked for.
    pub device_id: String,
    pub found: bool,
    pub name: Option<String>,
    /// FourCC of the transport (`usb `, `grup` for an aggregate).
    pub transport: Option<String>,
    pub nominal_rate: Option<f64>,
    pub actual_rate: Option<f64>,
    pub buffer_frames: Option<u32>,
    pub running_somewhere: Option<bool>,
    pub running_here: Option<bool>,
    /// Process holding the device in hog mode; -1 when none.
    pub hog_pid: Option<i32>,
    pub clock_domain: Option<u32>,
    pub clock_source: Option<u32>,
    /// UID of the device this one takes its clock from.
    pub clock_device: Option<String>,
    pub input_latency: Option<u32>,
    pub input_safety_offset: Option<u32>,
    pub io_cycle_usage: Option<f32>,
    /// Largest buffer the device may use when it varies its size; 0 when it
    /// does not.
    pub variable_buffer_frames: Option<u32>,
    pub input_streams: Vec<ProbedStream>,
    /// Every process that has the device open, OpenRig included.
    pub clients: Vec<DeviceClient>,
    /// UIDs of the aggregate devices that include this one.
    pub aggregates: Vec<String>,
}

/// Read the device behind a cpal device id.
pub fn probe_input_device(device_id: &str) -> DeviceProbe {
    #[cfg(target_os = "macos")]
    {
        crate::coreaudio_device_probe::probe(device_id)
    }
    #[cfg(not(target_os = "macos"))]
    {
        DeviceProbe {
            device_id: device_id.to_string(),
            ..Default::default()
        }
    }
}

#[cfg(test)]
#[path = "device_probe_tests.rs"]
mod tests;
