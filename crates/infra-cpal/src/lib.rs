use anyhow::{anyhow, bail, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{
    BufferSize, SampleFormat, Stream, StreamConfig, SupportedBufferSize, SupportedStreamConfig,
};
use engine::engine::PedalboardEngine;
use engine::runtime::{process_input_f32, process_output_f32, TrackRuntimeState};
use setup::device::{InputDevice, OutputDevice};
use setup::io::{Input, Output};
use setup::setup::Setup;
use setup::track::Track;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioDeviceDescriptor {
    pub id: String,
    pub name: String,
}
#[derive(Clone)]
struct ResolvedInputDevice {
    config: InputDevice,
    device: cpal::Device,
    supported: SupportedStreamConfig,
}
#[derive(Clone)]
struct ResolvedOutputDevice {
    config: OutputDevice,
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
        devices.push(AudioDeviceDescriptor {
            id: device.id()?.to_string(),
            name: description.name().to_string(),
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
        devices.push(AudioDeviceDescriptor {
            id: device.id()?.to_string(),
            name: description.name().to_string(),
        });
    }
    devices.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
    Ok(devices)
}
pub fn build_streams_for_setup(setup: &Setup, engine: &PedalboardEngine) -> Result<Vec<Stream>> {
    let host = cpal::default_host();
    let resolved_input_devices = resolve_input_devices(&host, &setup.input_devices)?;
    let resolved_output_devices = resolve_output_devices(&host, &setup.output_devices)?;
    validate_channels_against_devices(setup, &resolved_input_devices, &resolved_output_devices)?;
    let input_defs_by_id: HashMap<_, _> = setup
        .inputs
        .iter()
        .cloned()
        .map(|input| (input.id.clone(), input))
        .collect();
    let output_defs_by_id: HashMap<_, _> = setup
        .outputs
        .iter()
        .cloned()
        .map(|output| (output.id.clone(), output))
        .collect();
    let mut streams = Vec::new();
    for track in &setup.tracks {
        if !track.enabled {
            continue;
        }
        let input_cfg = input_defs_by_id
            .get(&track.input_id)
            .cloned()
            .ok_or_else(|| {
                anyhow!(
                    "track '{}' references missing input '{}'",
                    track.id.0,
                    track.input_id.0
                )
            })?;
        let resolved_input = resolved_input_devices
            .get(input_cfg.device)
            .cloned()
            .ok_or_else(|| {
                anyhow!(
                    "input '{}' references missing input device index {}",
                    input_cfg.id.0,
                    input_cfg.device
                )
            })?;
        let runtime = engine
            .runtime_for_track(&track.id)
            .ok_or_else(|| anyhow!("track '{}' has no runtime state", track.id.0))?;
        streams.push(build_input_stream_for_track(
            track.clone(),
            input_cfg,
            resolved_input,
            runtime.clone(),
        )?);
        for output_id in &track.output_ids {
            let output_cfg = output_defs_by_id.get(output_id).cloned().ok_or_else(|| {
                anyhow!(
                    "track '{}' references missing output '{}'",
                    track.id.0,
                    output_id.0
                )
            })?;
            let resolved_output = resolved_output_devices
                .get(output_cfg.device)
                .cloned()
                .ok_or_else(|| {
                    anyhow!(
                        "output '{}' references missing output device index {}",
                        output_cfg.id.0,
                        output_cfg.device
                    )
                })?;
            streams.push(build_output_stream_for_track(
                track.clone(),
                output_cfg,
                resolved_output,
                runtime.clone(),
            )?);
        }
    }
    Ok(streams)
}
fn resolve_input_devices(
    host: &cpal::Host,
    input_devices: &[InputDevice],
) -> Result<Vec<ResolvedInputDevice>> {
    let mut resolved = Vec::new();
    for input_device in input_devices {
        let device = find_input_device_by_id(host, &input_device.device_id.0)?.ok_or_else(|| {
            anyhow!("input device '{}' not found by device_id", input_device.device_id.0)
        })?;
        let supported = device.default_input_config().with_context(|| {
            format!("failed to get default input config for '{}'", input_device.device_id.0)
        })?;
        validate_sample_rate(
            input_device.sample_rate,
            supported.sample_rate(),
            &input_device.device_id.0,
        )?;
        validate_buffer_size(
            input_device.buffer_size_frames,
            supported.buffer_size(),
            &input_device.device_id.0,
        )?;
        resolved.push(ResolvedInputDevice {
            config: input_device.clone(),
            device,
            supported,
        });
    }
    Ok(resolved)
}
fn resolve_output_devices(
    host: &cpal::Host,
    output_devices: &[OutputDevice],
) -> Result<Vec<ResolvedOutputDevice>> {
    let mut resolved = Vec::new();
    for output_device in output_devices {
        let device = find_output_device_by_id(host, &output_device.device_id.0)?.ok_or_else(|| {
            anyhow!("output device '{}' not found by device_id", output_device.device_id.0)
        })?;
        let supported = device.default_output_config().with_context(|| {
            format!("failed to get default output config for '{}'", output_device.device_id.0)
        })?;
        validate_sample_rate(
            output_device.sample_rate,
            supported.sample_rate(),
            &output_device.device_id.0,
        )?;
        validate_buffer_size(
            output_device.buffer_size_frames,
            supported.buffer_size(),
            &output_device.device_id.0,
        )?;
        resolved.push(ResolvedOutputDevice {
            config: output_device.clone(),
            device,
            supported,
        });
    }
    Ok(resolved)
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
    input_devices: &[ResolvedInputDevice],
    output_devices: &[ResolvedOutputDevice],
) -> Result<()> {
    for input in &setup.inputs {
        let resolved = input_devices
            .get(input.device)
            .ok_or_else(|| anyhow!("input '{}' missing resolved device", input.id.0))?;
        let total_channels = resolved.supported.channels() as usize;
        for channel in &input.channels {
            if *channel >= total_channels {
                bail!(
                    "input '{}' invalid: channel '{}' outside device range (channels={})",
                    input.id.0,
                    channel,
                    total_channels
                );
            }
        }
    }
    for output in &setup.outputs {
        let resolved = output_devices
            .get(output.device)
            .ok_or_else(|| anyhow!("output '{}' missing resolved device", output.id.0))?;
        let total_channels = resolved.supported.channels() as usize;
        for channel in &output.channels {
            if *channel >= total_channels {
                bail!(
                    "output '{}' invalid: channel '{}' outside device range (channels={})",
                    output.id.0,
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
    input_cfg: Input,
    resolved_input_device: ResolvedInputDevice,
    runtime: Arc<Mutex<TrackRuntimeState>>,
) -> Result<Stream> {
    let sample_format = resolved_input_device.supported.sample_format();
    let stream_config = build_stream_config(
        resolved_input_device.supported.channels(),
        resolved_input_device.config.sample_rate,
        resolved_input_device.config.buffer_size_frames,
    );
    let device = resolved_input_device.device;
    let stream = match sample_format {
        SampleFormat::F32 => {
            let track_for_data = track.clone();
            let input_cfg_for_data = input_cfg.clone();
            let runtime_for_data = runtime.clone();
            let channels = stream_config.channels as usize;
            let error_track_id = track.id.0.clone();
            device.build_input_stream(
                &stream_config,
                move |data: &[f32], _| {
                    process_input_f32(
                        &track_for_data,
                        &input_cfg_for_data,
                        &runtime_for_data,
                        data,
                        channels,
                    );
                },
                move |err| eprintln!("[{}] input error: {}", error_track_id, err),
                None,
            )?
        }
        SampleFormat::I16 => {
            let track_for_data = track.clone();
            let input_cfg_for_data = input_cfg.clone();
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
                    process_input_f32(
                        &track_for_data,
                        &input_cfg_for_data,
                        &runtime_for_data,
                        &converted,
                        channels,
                    );
                },
                move |err| eprintln!("[{}] input error: {}", error_track_id, err),
                None,
            )?
        }
        SampleFormat::U16 => {
            let track_for_data = track.clone();
            let input_cfg_for_data = input_cfg.clone();
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
                    process_input_f32(
                        &track_for_data,
                        &input_cfg_for_data,
                        &runtime_for_data,
                        &converted,
                        channels,
                    );
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
    output_cfg: Output,
    resolved_output_device: ResolvedOutputDevice,
    runtime: Arc<Mutex<TrackRuntimeState>>,
) -> Result<Stream> {
    let sample_format = resolved_output_device.supported.sample_format();
    let stream_config = build_stream_config(
        resolved_output_device.supported.channels(),
        resolved_output_device.config.sample_rate,
        resolved_output_device.config.buffer_size_frames,
    );
    let device = resolved_output_device.device;
    let stream = match sample_format {
        SampleFormat::F32 => {
            let track_for_data = track.clone();
            let output_cfg_for_data = output_cfg.clone();
            let runtime_for_data = runtime.clone();
            let channels = stream_config.channels as usize;
            let error_track_id = track.id.0.clone();
            device.build_output_stream(
                &stream_config,
                move |out: &mut [f32], _| {
                    process_output_f32(
                        &track_for_data,
                        &output_cfg_for_data,
                        &runtime_for_data,
                        out,
                        channels,
                    );
                },
                move |err| eprintln!("[{}] output error: {}", error_track_id, err),
                None,
            )?
        }
        SampleFormat::I16 => {
            let track_for_data = track.clone();
            let output_cfg_for_data = output_cfg.clone();
            let runtime_for_data = runtime.clone();
            let channels = stream_config.channels as usize;
            let error_track_id = track.id.0.clone();
            device.build_output_stream(
                &stream_config,
                move |out: &mut [i16], _| {
                    let mut temp = vec![0.0f32; out.len()];
                    process_output_f32(
                        &track_for_data,
                        &output_cfg_for_data,
                        &runtime_for_data,
                        &mut temp,
                        channels,
                    );
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
            let output_cfg_for_data = output_cfg.clone();
            let runtime_for_data = runtime.clone();
            let channels = stream_config.channels as usize;
            let error_track_id = track.id.0.clone();
            device.build_output_stream(
                &stream_config,
                move |out: &mut [u16], _| {
                    let mut temp = vec![0.0f32; out.len()];
                    process_output_f32(
                        &track_for_data,
                        &output_cfg_for_data,
                        &runtime_for_data,
                        &mut temp,
                        channels,
                    );
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
