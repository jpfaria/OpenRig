//! Responsibility: opens an independent pipeline's own cpal output stream.

#![cfg(not(all(target_os = "linux", feature = "jack")))]

use anyhow::Result;
use cpal::traits::{DeviceTrait, StreamTrait};

use crate::aux_output::{AuxOutputLayout, AuxRender};

/// Frames per buffer the stream asks the device for.
const AUX_BUFFER_FRAMES: u32 = 512;
/// Largest buffer a device may hand back regardless of the request; the render
/// pre-allocates for it so the callback never grows a buffer.
const AUX_MAX_FRAMES: usize = 8192;

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
}

/// Opens and starts an output on `device_id` whose callback is the render
/// `make_render` builds for the stream's real layout.
pub(crate) fn open_aux_output(
    device_id: &str,
    targets: &[usize],
    label: &str,
    make_render: impl FnOnce(&AuxOutputLayout) -> AuxRender,
) -> Result<AuxOutputHandle> {
    let host = crate::host::get_host();
    let device = crate::find_output_device_by_id(host, device_id)?
        .ok_or_else(|| anyhow::anyhow!("{label} output device '{device_id}' not found"))?;
    let supported = device.default_output_config()?;
    let sample_rate = supported.sample_rate();
    let config = crate::stream_config::build_stream_config(
        supported.channels(),
        sample_rate,
        AUX_BUFFER_FRAMES,
    );
    let layout = AuxOutputLayout {
        sample_rate,
        channels: supported.channels() as usize,
        targets: targets.to_vec(),
        max_frames: AUX_MAX_FRAMES,
    };
    let mut render = make_render(&layout);
    let error_label = format!("{label}:{device_id}");
    let stream = device.build_output_stream(
        &config,
        move |out: &mut [f32], _| {
            if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| render(out))).is_err() {
                out.fill(0.0);
            }
        },
        move |err| log::error!("[{error_label}] output stream error: {err}"),
        None,
    )?;
    stream.play()?;
    Ok(AuxOutputHandle {
        device_id: device_id.to_string(),
        targets: targets.to_vec(),
        sample_rate,
        _stream: stream,
    })
}
