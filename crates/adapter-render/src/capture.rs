//! Live capture from a cpal input device into a mono WAV file.
//!
//! Used by `adapter_render::render()` when the input WAV path does not
//! exist yet: capture once, save to disk, reuse on every subsequent run.
//!
//! cpal is only initialised when capture is actually requested — the
//! headless file-mode render path stays device-free.

use anyhow::{anyhow, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::Sample;
use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::wav::{write_wav_stereo, BitDepth};

/// Capture from the named input device (substring match on the device
/// name; `None` → default) for `duration_s` seconds at `sample_rate_hz`,
/// write the dry capture to `out_path` as a stereo WAV.
///
/// Mono interfaces are broadcast to stereo per CLAUDE.md invariant 5.
/// Multi-channel interfaces have their first two channels taken.
pub fn capture_to_wav(
    out_path: &Path,
    device_name: Option<&str>,
    duration_s: f32,
    sample_rate_hz: u32,
) -> Result<()> {
    let host = cpal::default_host();
    let device = pick_input_device(&host, device_name)?;

    let config = device
        .default_input_config()
        .context("failed to query default input config")?;
    let channels = config.channels() as usize;
    let device_sr: u32 = config.sample_rate();

    log::info!(
        "openrig-render: capturing {} s from '{}' (sr {}, {} ch) → {}",
        duration_s,
        device_label(&device),
        device_sr,
        channels,
        out_path.display()
    );

    let captured: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::with_capacity(
        (duration_s * device_sr as f32) as usize * channels,
    )));
    let err_fn = |err| log::error!("cpal capture stream error: {err}");

    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => {
            build_stream::<f32>(&device, &config.into(), captured.clone(), err_fn)?
        }
        cpal::SampleFormat::I16 => {
            build_stream::<i16>(&device, &config.into(), captured.clone(), err_fn)?
        }
        cpal::SampleFormat::U16 => {
            build_stream::<u16>(&device, &config.into(), captured.clone(), err_fn)?
        }
        other => return Err(anyhow!("unsupported cpal sample format: {other:?}")),
    };

    stream.play().context("failed to start capture stream")?;
    std::thread::sleep(std::time::Duration::from_secs_f32(duration_s));
    drop(stream); // drop closes the stream — cpal docs

    let samples = captured.lock().expect("capture mutex poisoned").clone();
    let frames = interleaved_to_stereo(&samples, channels);

    // Save at the engine's sample rate. We trust cpal's device SR matches
    // sample_rate_hz; if not, this is a future resampling task tracked in
    // the issue body. Today the rest of the pipeline assumes both are
    // equal, so warn but proceed.
    if device_sr != sample_rate_hz {
        log::warn!(
            "openrig-render: capture device SR {device_sr} != requested {sample_rate_hz}; \
             saving at device SR — resampling is not yet implemented"
        );
    }
    write_wav_stereo(out_path, &frames, device_sr, BitDepth::Bits24)
        .map_err(|e| anyhow!("failed to write capture wav: {e}"))?;
    Ok(())
}

fn build_stream<S>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    captured: Arc<Mutex<Vec<f32>>>,
    err_fn: impl FnMut(cpal::StreamError) + Send + 'static,
) -> Result<cpal::Stream>
where
    S: cpal::Sample + cpal::SizedSample,
    f32: cpal::FromSample<S>,
{
    device
        .build_input_stream(
            config,
            move |data: &[S], _| {
                if let Ok(mut buf) = captured.lock() {
                    buf.extend(data.iter().map(|s| f32::from_sample(*s)));
                }
            },
            err_fn,
            None,
        )
        .map_err(|e| anyhow!("failed to build input stream: {e}"))
}

// cpal 0.17 deprecated `name()` in favour of `description()` and `id()`.
// We use `name()` for a single human-readable log line; the cpal team
// will not remove the trait method without bumping the major, and the
// alternatives return richer structs we don't need here. Scoped allow.
#[allow(deprecated)]
fn pick_input_device(host: &cpal::Host, name_filter: Option<&str>) -> Result<cpal::Device> {
    match name_filter {
        Some(name) => host
            .input_devices()
            .context("failed to enumerate input devices")?
            .find(|d| d.name().map(|n| n.contains(name)).unwrap_or(false))
            .ok_or_else(|| anyhow!("input device not found: {name}")),
        None => host
            .default_input_device()
            .ok_or_else(|| anyhow!("no default input device available")),
    }
}

#[allow(deprecated)]
fn device_label(device: &cpal::Device) -> String {
    device.name().unwrap_or_else(|_| "<unknown>".into())
}

fn interleaved_to_stereo(samples: &[f32], channels: usize) -> Vec<[f32; 2]> {
    if channels == 0 {
        return Vec::new();
    }
    match channels {
        1 => samples.iter().map(|&s| [s, s]).collect(),
        2 => samples.chunks_exact(2).map(|c| [c[0], c[1]]).collect(),
        n => samples.chunks_exact(n).map(|c| [c[0], c[1]]).collect(),
    }
}
