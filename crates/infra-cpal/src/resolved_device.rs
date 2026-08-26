//! Responsibility: describes one device after the backend picked a config for it.
//!
//! Split out of `resolved.rs` (#873): the old file did three things — the
//! resolved devices, the stream signatures that decide a rebuild, and the
//! whole-chain config that carries both.

use cpal::SupportedStreamConfig;
use project::device::DeviceSettings;

// Fields are read by non-JACK code (CPAL stream construction, sample-rate
// resolution, buffer-size resolution); JACK direct backend resolves the same
// info from the JACK client and ignores these structs' contents, but they're
// still constructed so the same project plumbing works.
#[allow(dead_code)]
#[derive(Clone)]
pub(crate) struct ResolvedInputDevice {
    pub(crate) settings: Option<DeviceSettings>,
    pub(crate) device: cpal::Device,
    pub(crate) supported: SupportedStreamConfig,
}
#[allow(dead_code)]
#[derive(Clone)]
pub(crate) struct ResolvedOutputDevice {
    /// Logical device id of the binding output endpoint this was resolved from
    /// — the same id the isolation map keys on, so an output stream mixes only
    /// the runtimes whose binding feeds THIS device.
    pub(crate) device_id: String,
    pub(crate) settings: Option<DeviceSettings>,
    pub(crate) device: cpal::Device,
    pub(crate) supported: SupportedStreamConfig,
}
