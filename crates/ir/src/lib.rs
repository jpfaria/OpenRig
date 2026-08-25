//! Responsibility: routes the impulse response crate's public surface.

pub mod from_package;

mod fft_convolver;
mod ir_asset;
mod ir_builders;
mod ir_prepare;
mod ir_processors;

pub use from_package::{build_from_package, register_builder};

pub use fft_convolver::PARTITION_SIZE;
pub use ir_asset::{IrAsset, IrChannelData};
pub use ir_builders::{build_mono_ir_processor_from_wav, build_stereo_ir_processor_from_wav};
pub use ir_processors::{MonoIrProcessor, StereoIrProcessor};

#[cfg(test)]
pub(crate) use ir_prepare::{
    lanczos_kernel, resample_if_needed, truncate_with_fade, FADE_OUT_SAMPLES, MAX_IR_SAMPLES,
};

#[cfg(test)]
#[path = "test_support.rs"]
pub mod test_support;

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
