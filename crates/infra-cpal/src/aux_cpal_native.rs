//! Responsibility: opens an auxiliary cpal output in the device's native sample format.

#![cfg(not(all(target_os = "linux", feature = "jack")))]

use anyhow::{bail, Result};
use cpal::traits::DeviceTrait;
use cpal::{SampleFormat, StreamConfig, SupportedStreamConfig};

use crate::aux_output_cpal::AUX_MAX_FRAMES;
use crate::output_sample_convert::{f32_to_i16, f32_to_i32, f32_to_u16, NativeOutputBuffer};

/// The stream config an auxiliary output asks for, plus the frames its
/// callback buffers are preallocated for.
pub(crate) struct AuxCpalConfig {
    pub(crate) config: StreamConfig,
    pub(crate) max_frames: usize,
}

/// On ASIO the driver owns the buffer size, so the stream takes the driver's
/// own and preallocates up to its maximum (#978); other hosts keep the
/// project's request and the aux floor.
pub(crate) fn aux_cpal_config(
    host_is_asio: bool,
    supported: &SupportedStreamConfig,
    sample_rate: u32,
    buffer_frames: u32,
) -> AuxCpalConfig {
    let config = StreamConfig {
        channels: supported.channels(),
        sample_rate,
        buffer_size: crate::driver_buffer::requested_buffer(host_is_asio, buffer_frames),
    };
    let max_frames = crate::driver_buffer::callback_capacity_frames(
        host_is_asio,
        AUX_MAX_FRAMES as u32,
        supported.buffer_size(),
    ) as usize;
    AuxCpalConfig { config, max_frames }
}

/// Builds an output stream on `device` in `supported`'s sample format whose
/// callback runs the f32 `render`. A panicking render plays silence.
pub(crate) fn build_native_output(
    device: &cpal::Device,
    supported: &SupportedStreamConfig,
    format: AuxCpalConfig,
    mut render: impl FnMut(&mut [f32]) + Send + 'static,
    error_context: String,
) -> Result<cpal::Stream> {
    let samples = format.max_frames * format.config.channels as usize;
    let on_error = crate::stream_error::stream_error_handler(error_context);
    let stream = match supported.sample_format() {
        SampleFormat::F32 => device.build_output_stream(
            format.config,
            move |out: &mut [f32], _: &cpal::OutputCallbackInfo| {
                if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| render(out))).is_err() {
                    out.fill(0.0);
                }
            },
            on_error,
            None,
        )?,
        SampleFormat::I16 => {
            native_stream(device, format.config, samples, f32_to_i16, render, on_error)?
        }
        SampleFormat::U16 => {
            native_stream(device, format.config, samples, f32_to_u16, render, on_error)?
        }
        SampleFormat::I32 => {
            native_stream(device, format.config, samples, f32_to_i32, render, on_error)?
        }
        other => bail!("unsupported output sample format {other:?}"),
    };
    Ok(stream)
}

fn native_stream<T: cpal::SizedSample + Send + 'static>(
    device: &cpal::Device,
    config: StreamConfig,
    samples: usize,
    convert: fn(f32) -> T,
    mut render: impl FnMut(&mut [f32]) + Send + 'static,
    on_error: impl FnMut(cpal::Error) + Send + 'static,
) -> Result<cpal::Stream> {
    let mut buffer = NativeOutputBuffer::with_capacity(samples);
    Ok(device.build_output_stream(
        config,
        move |out: &mut [T], _: &cpal::OutputCallbackInfo| {
            let filled = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                buffer.fill(out, convert, &mut render)
            }));
            if filled.is_err() {
                out.fill(convert(0.0));
            }
        },
        on_error,
        None,
    )?)
}

#[cfg(test)]
#[path = "aux_cpal_config_tests.rs"]
mod tests;
