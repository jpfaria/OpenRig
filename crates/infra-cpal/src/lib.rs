use anyhow::{anyhow, bail, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{
    BufferSize, SampleFormat, Stream, StreamConfig, SupportedBufferSize, SupportedStreamConfig,
};
use engine::engine::PedalboardEngine;
use engine::runtime::{process_input_f32, process_output_f32, TrackRuntimeState};
use setup::device::DeviceSettings;
use setup::setup::Setup;
use setup::track::Track;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioDeviceDescriptor {
    pub id: String,
    pub name: String,
    pub channels: usize,
}
#[derive(Clone)]
struct ResolvedInputDevice {
    settings: Option<DeviceSettings>,
    device: cpal::Device,
    supported: SupportedStreamConfig,
}
#[derive(Clone)]
struct ResolvedOutputDevice {
    settings: Option<DeviceSettings>,
    device: cpal::Device,
    supported: SupportedStreamConfig,
}
pub fn list_devices() -> Result<Vec<String>> {
    let host = cpal::default_host();
    let mut devices = Vec::new();
    for device in host.input_devices()? {
        let description = device.description()?;
        devices.push(format!(
            "input: {} | device_id: {}",
            description,
            device.id()?
        ));
    }
    for device in host.output_devices()? {
        let description = device.description()?;
        devices.push(format!(
            "output: {} | device_id: {}",
            description,
            device.id()?
        ));
    }
    Ok(devices)
}

pub fn list_input_device_descriptors() -> Result<Vec<AudioDeviceDescriptor>> {
    let host = cpal::default_host();
    let mut devices = Vec::new();
    for device in host.input_devices()? {
        let description = device.description()?;
        let channels = device
            .default_input_config()
            .map(|config| config.channels() as usize)
            .unwrap_or(0);
        devices.push(AudioDeviceDescriptor {
            id: device.id()?.to_string(),
            name: description.name().to_string(),
            channels,
        });
    }
    devices.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
    Ok(devices)
}

pub fn list_output_device_descriptors() -> Result<Vec<AudioDeviceDescriptor>> {
    let host = cpal::default_host();
    let mut devices = Vec::new();
    for device in host.output_devices()? {
        let description = device.description()?;
        let channels = device
            .default_output_config()
            .map(|config| config.channels() as usize)
            .unwrap_or(0);
        devices.push(AudioDeviceDescriptor {
            id: device.id()?.to_string(),
            name: description.name().to_string(),
            channels,
        });
    }
    devices.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
    Ok(devices)
}
pub fn build_streams_for_setup(setup: &Setup, engine: &PedalboardEngine) -> Result<Vec<Stream>> {
    let host = cpal::default_host();
    validate_channels_against_devices(setup, &host)?;
    let mut streams = Vec::new();
    for track in &setup.tracks {
        if !track.enabled {
            continue;
        }
        let resolved_input = resolve_input_device_for_track(&host, setup, track)?;
        let runtime = engine
            .runtime_for_track(&track.id)
            .ok_or_else(|| anyhow!("track '{}' has no runtime state", track.id.0))?;
        streams.push(build_input_stream_for_track(
            track.clone(),
            resolved_input,
            runtime.clone(),
        )?);
        let resolved_output = resolve_output_device_for_track(&host, setup, track)?;
        streams.push(build_output_stream_for_track(
            track.clone(),
            resolved_output,
            runtime.clone(),
        )?);
    }
    Ok(streams)
}
fn resolve_input_device_for_track(host: &cpal::Host, setup: &Setup, track: &Track) -> Result<ResolvedInputDevice> {
    let settings = setup
        .device_settings
        .iter()
        .find(|settings| settings.device_id == track.input_device_id)
        .cloned();
    let device = find_input_device_by_id(host, &track.input_device_id.0)?.ok_or_else(|| {
        anyhow!("input device '{}' not found by device_id", track.input_device_id.0)
    })?;
    let supported = device.default_input_config().with_context(|| {
        format!(
            "failed to get default input config for '{}'",
            track.input_device_id.0
        )
    })?;
    if let Some(settings) = &settings {
        validate_sample_rate(settings.sample_rate, supported.sample_rate(), &settings.device_id.0)?;
        validate_buffer_size(
            settings.buffer_size_frames,
            supported.buffer_size(),
            &settings.device_id.0,
        )?;
    }
    Ok(ResolvedInputDevice {
        settings,
        device,
        supported,
    })
}

fn resolve_output_device_for_track(host: &cpal::Host, setup: &Setup, track: &Track) -> Result<ResolvedOutputDevice> {
    let settings = setup
        .device_settings
        .iter()
        .find(|settings| settings.device_id == track.output_device_id)
        .cloned();
    let device = find_output_device_by_id(host, &track.output_device_id.0)?.ok_or_else(|| {
        anyhow!("output device '{}' not found by device_id", track.output_device_id.0)
    })?;
    let supported = device.default_output_config().with_context(|| {
        format!(
            "failed to get default output config for '{}'",
            track.output_device_id.0
        )
    })?;
    if let Some(settings) = &settings {
        validate_sample_rate(settings.sample_rate, supported.sample_rate(), &settings.device_id.0)?;
        validate_buffer_size(
            settings.buffer_size_frames,
            supported.buffer_size(),
            &settings.device_id.0,
        )?;
    }
    Ok(ResolvedOutputDevice {
        settings,
        device,
        supported,
    })
}
fn validate_sample_rate(requested: u32, supported_default: u32, context: &str) -> Result<()> {
    if requested != supported_default {
        bail!(
            "{} invalid: configured sample_rate={} but device default sample_rate={}",
            context,
            requested,
            supported_default
        );
    }
    Ok(())
}
fn validate_buffer_size(
    requested: u32,
    supported: &SupportedBufferSize,
    context: &str,
) -> Result<()> {
    match supported {
        SupportedBufferSize::Range { min, max } => {
            if requested < *min || requested > *max {
                bail!(
                    "{} invalid: buffer_size_frames={} outside supported range [{}..={}]",
                    context,
                    requested,
                    min,
                    max
                );
            }
        }
        SupportedBufferSize::Unknown => {}
    }
    Ok(())
}
fn validate_channels_against_devices(
    setup: &Setup,
    host: &cpal::Host,
) -> Result<()> {
    for track in &setup.tracks {
        if !track.enabled {
            continue;
        }
        let input_device = find_input_device_by_id(host, &track.input_device_id.0)?
            .ok_or_else(|| anyhow!("track '{}' missing input device '{}'", track.id.0, track.input_device_id.0))?;
        let resolved = input_device.default_input_config().with_context(|| {
            format!(
                "failed to get default input config for '{}'",
                track.input_device_id.0
            )
        })?;
        let total_channels = resolved.channels() as usize;
        for channel in &track.input_channels {
            if *channel >= total_channels {
                bail!(
                    "track '{}' invalid: input channel '{}' outside device range (channels={})",
                    track.id.0,
                    channel,
                    total_channels
                );
            }
        }
        let output_device = find_output_device_by_id(host, &track.output_device_id.0)?
            .ok_or_else(|| anyhow!("track '{}' missing output device '{}'", track.id.0, track.output_device_id.0))?;
        let resolved = output_device.default_output_config().with_context(|| {
            format!(
                "failed to get default output config for '{}'",
                track.output_device_id.0
            )
        })?;
        let total_channels = resolved.channels() as usize;
        for channel in &track.output_channels {
            if *channel >= total_channels {
                bail!(
                    "track '{}' invalid: output channel '{}' outside device range (channels={})",
                    track.id.0,
                    channel,
                    total_channels
                );
            }
        }
    }
    Ok(())
}
fn find_input_device_by_id(host: &cpal::Host, device_id: &str) -> Result<Option<cpal::Device>> {
    for device in host.input_devices()? {
        if device.id()?.to_string() == device_id {
            return Ok(Some(device));
        }
    }
    Ok(None)
}
fn find_output_device_by_id(host: &cpal::Host, device_id: &str) -> Result<Option<cpal::Device>> {
    for device in host.output_devices()? {
        if device.id()?.to_string() == device_id {
            return Ok(Some(device));
        }
    }
    Ok(None)
}
fn build_input_stream_for_track(
    track: Track,
    resolved_input_device: ResolvedInputDevice,
    runtime: Arc<Mutex<TrackRuntimeState>>,
) -> Result<Stream> {
    let sample_format = resolved_input_device.supported.sample_format();
    let sample_rate = resolved_input_device
        .settings
        .as_ref()
        .map(|settings| settings.sample_rate)
        .unwrap_or_else(|| resolved_input_device.supported.sample_rate());
    let buffer_size_frames = resolved_input_device
        .settings
        .as_ref()
        .map(|settings| settings.buffer_size_frames)
        .unwrap_or(256);
    let stream_config = build_stream_config(
        resolved_input_device.supported.channels(),
        sample_rate,
        buffer_size_frames,
    );
    let device = resolved_input_device.device;
    let stream = match sample_format {
        SampleFormat::F32 => {
            let track_for_data = track.clone();
            let runtime_for_data = runtime.clone();
            let channels = stream_config.channels as usize;
            let error_track_id = track.id.0.clone();
            device.build_input_stream(
                &stream_config,
                move |data: &[f32], _| {
                    process_input_f32(&track_for_data, &runtime_for_data, data, channels);
                },
                move |err| eprintln!("[{}] input error: {}", error_track_id, err),
                None,
            )?
        }
        SampleFormat::I16 => {
            let track_for_data = track.clone();
            let runtime_for_data = runtime.clone();
            let channels = stream_config.channels as usize;
            let error_track_id = track.id.0.clone();
            device.build_input_stream(
                &stream_config,
                move |data: &[i16], _| {
                    let converted: Vec<f32> = data
                        .iter()
                        .map(|sample| *sample as f32 / i16::MAX as f32)
                        .collect();
                    process_input_f32(&track_for_data, &runtime_for_data, &converted, channels);
                },
                move |err| eprintln!("[{}] input error: {}", error_track_id, err),
                None,
            )?
        }
        SampleFormat::U16 => {
            let track_for_data = track.clone();
            let runtime_for_data = runtime.clone();
            let channels = stream_config.channels as usize;
            let error_track_id = track.id.0.clone();
            device.build_input_stream(
                &stream_config,
                move |data: &[u16], _| {
                    let converted: Vec<f32> = data
                        .iter()
                        .map(|sample| (*sample as f32 / u16::MAX as f32) * 2.0 - 1.0)
                        .collect();
                    process_input_f32(&track_for_data, &runtime_for_data, &converted, channels);
                },
                move |err| eprintln!("[{}] input error: {}", error_track_id, err),
                None,
            )?
        }
        other => {
            bail!(
                "unsupported input sample format for track '{}': {:?}",
                track.id.0,
                other
            );
        }
    };
    Ok(stream)
}
fn build_output_stream_for_track(
    track: Track,
    resolved_output_device: ResolvedOutputDevice,
    runtime: Arc<Mutex<TrackRuntimeState>>,
) -> Result<Stream> {
    let sample_format = resolved_output_device.supported.sample_format();
    let sample_rate = resolved_output_device
        .settings
        .as_ref()
        .map(|settings| settings.sample_rate)
        .unwrap_or_else(|| resolved_output_device.supported.sample_rate());
    let buffer_size_frames = resolved_output_device
        .settings
        .as_ref()
        .map(|settings| settings.buffer_size_frames)
        .unwrap_or(256);
    let stream_config = build_stream_config(
        resolved_output_device.supported.channels(),
        sample_rate,
        buffer_size_frames,
    );
    let device = resolved_output_device.device;
    let stream = match sample_format {
        SampleFormat::F32 => {
            let track_for_data = track.clone();
            let runtime_for_data = runtime.clone();
            let channels = stream_config.channels as usize;
            let error_track_id = track.id.0.clone();
            device.build_output_stream(
                &stream_config,
                move |out: &mut [f32], _| {
                    process_output_f32(&track_for_data, &runtime_for_data, out, channels);
                },
                move |err| eprintln!("[{}] output error: {}", error_track_id, err),
                None,
            )?
        }
        SampleFormat::I16 => {
            let track_for_data = track.clone();
            let runtime_for_data = runtime.clone();
            let channels = stream_config.channels as usize;
            let error_track_id = track.id.0.clone();
            device.build_output_stream(
                &stream_config,
                move |out: &mut [i16], _| {
                    let mut temp = vec![0.0f32; out.len()];
                    process_output_f32(&track_for_data, &runtime_for_data, &mut temp, channels);
                    for (dst, src) in out.iter_mut().zip(temp.iter()) {
                        *dst =
                            (*src * i16::MAX as f32).clamp(i16::MIN as f32, i16::MAX as f32) as i16;
                    }
                },
                move |err| eprintln!("[{}] output error: {}", error_track_id, err),
                None,
            )?
        }
        SampleFormat::U16 => {
            let track_for_data = track.clone();
            let runtime_for_data = runtime.clone();
            let channels = stream_config.channels as usize;
            let error_track_id = track.id.0.clone();
            device.build_output_stream(
                &stream_config,
                move |out: &mut [u16], _| {
                    let mut temp = vec![0.0f32; out.len()];
                    process_output_f32(&track_for_data, &runtime_for_data, &mut temp, channels);
                    for (dst, src) in out.iter_mut().zip(temp.iter()) {
                        let normalized =
                            ((*src + 1.0) * 0.5 * u16::MAX as f32).clamp(0.0, u16::MAX as f32);
                        *dst = normalized as u16;
                    }
                },
                move |err| eprintln!("[{}] output error: {}", error_track_id, err),
                None,
            )?
        }
        other => {
            bail!(
                "unsupported output sample format for track '{}': {:?}",
                track.id.0,
                other
            );
        }
    };
    Ok(stream)
}
fn build_stream_config(channels: u16, sample_rate: u32, buffer_size_frames: u32) -> StreamConfig {
    StreamConfig {
        channels,
        sample_rate,
        buffer_size: BufferSize::Fixed(buffer_size_frames),
    }
}
