//! Responsibility: routes the domain crate's public surface.

pub mod audio_device;
pub mod ids;
pub mod io_binding;
pub mod parameter_value;
pub mod units;
pub mod value_objects;

pub use audio_device::AudioDeviceDescriptor;
pub use io_binding::{ChannelMode, IoBinding, IoEndpoint};

#[cfg(test)]
mod lib_tests;
