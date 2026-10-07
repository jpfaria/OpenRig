//! Responsibility: opens an independent pipeline's own cpal output stream.

#![cfg(not(all(target_os = "linux", feature = "jack")))]

use anyhow::Result;
use cpal::traits::{DeviceTrait, StreamTrait};

use crate::aux_cpal_native::{aux_cpal_config, build_native_output};

use crate::aux_output::{AuxOutputLayout, AuxRender};
use crate::aux_stream_format::aux_stream_format;

/// Largest buffer a device may hand back regardless of the request; the render
/// pre-allocates for it so the callback never grows a buffer.
pub(crate) const AUX_MAX_FRAMES: usize = 8192;

/// An open auxiliary stream. Dropping it closes the stream.
pub(crate) struct AuxOutputHandle {
    device_id: String,
    targets: Vec<usize>,
    sample_rate: u32,
    _stream: cpal::Stream,
}

impl AuxOutputHandle {
    /// Whether this stream already plays to `device_id` on `targets`.
    pub(crate) fn serves(&self, device_id: &str, targets: &[usize]) -> bool {
        self.device_id == device_id && self.targets == targets
    }

    pub(crate) fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// #1081: whether this stream plays on one of `devices`.
    pub(crate) fn plays_on(&self, devices: &[String]) -> bool {
        devices.contains(&self.device_id)
    }

    /// #1081: the device and channels this stream plays to, to reopen it there.
    pub(crate) fn endpoint(&self) -> (String, Vec<usize>) {
        (self.device_id.clone(), self.targets.clone())
    }
}

/// Opens and starts an output on `device_id` whose callback is the render
/// `make_render` builds for the stream's real layout, with the project's
/// settings for that device.
pub(crate) fn open_aux_output(
    device_settings: &[project::device::DeviceSettings],
    device_id: &str,
    targets: &[usize],
    label: &str,
    make_render: impl FnOnce(&AuxOutputLayout) -> AuxRender,
) -> Result<AuxOutputHandle> {
    let host = crate::host::get_host();
    let device = crate::find_output_device_by_id(host, device_id)?
        .ok_or_else(|| anyhow::anyhow!("{label} output device '{device_id}' not found"))?;
    let supported = device.default_output_config()?;
    let format = aux_stream_format(device_settings, device_id, supported.sample_rate());
    let sample_rate = format.sample_rate;
    let host_is_asio = crate::host::is_asio_host(host);
    let native = aux_cpal_config(host_is_asio, &supported, sample_rate, format.buffer_frames);
    let layout = AuxOutputLayout {
        sample_rate,
        channels: supported.channels() as usize,
        targets: targets.to_vec(),
        max_frames: native.max_frames,
    };
    let mut render =
        crate::output_fader::faded_render(device_id, targets, &layout, make_render(&layout));
    let stream = build_native_output(
        &device,
        &supported,
        native,
        move |out: &mut [f32]| render(out),
        format!("[{label}:{device_id}] output stream error"),
    )?;
    stream.play()?;
    Ok(AuxOutputHandle {
        device_id: device_id.to_string(),
        targets: targets.to_vec(),
        sample_rate,
        _stream: stream,
    })
}
