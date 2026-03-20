use anyhow::{anyhow, bail, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{
    BufferSize, SampleFormat, Stream, StreamConfig, SupportedBufferSize, SupportedStreamConfig,
    SupportedStreamConfigRange,
};
use engine::runtime::{process_input_f32, process_output_f32, RuntimeGraph, TrackRuntimeState};
use domain::ids::TrackId;
use project::device::DeviceSettings;
use project::project::Project;
use project::track::Track;
use std::collections::HashMap;
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct TrackStreamSignature {
    input_device_id: String,
    input_channels: Vec<usize>,
    input_stream_channels: u16,
    input_sample_rate: u32,
    input_buffer_size_frames: u32,
    output_device_id: String,
    output_channels: Vec<usize>,
    output_stream_channels: u16,
    output_sample_rate: u32,
    output_buffer_size_frames: u32,
}

struct ResolvedTrackAudioConfig {
    input: ResolvedInputDevice,
    output: ResolvedOutputDevice,
    sample_rate: f32,
    stream_signature: TrackStreamSignature,
}

struct ActiveTrackRuntime {
    stream_signature: TrackStreamSignature,
    input_stream: Stream,
    output_stream: Stream,
}

pub struct ProjectRuntimeController {
    runtime_graph: RuntimeGraph,
    active_tracks: HashMap<TrackId, ActiveTrackRuntime>,
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
            channels: max_supported_input_channels(&device).unwrap_or(0),
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
            channels: max_supported_output_channels(&device).unwrap_or(0),
        });
    }
    devices.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
    Ok(devices)
}
pub fn build_streams_for_project(
    project: &Project,
    runtime_graph: &RuntimeGraph,
) -> Result<Vec<Stream>> {
    let host = cpal::default_host();
    validate_channels_against_devices(project, &host)?;
    let mut resolved_tracks = resolve_enabled_track_audio_configs(&host, project)?;
    let mut streams = Vec::new();
    for track in &project.tracks {
        if !track.enabled {
            continue;
        }
        let runtime = runtime_graph
            .tracks
            .get(&track.id)
            .cloned()
            .ok_or_else(|| anyhow!("track '{}' has no runtime state", track.id.0))?;
        let resolved = resolved_tracks
            .remove(&track.id)
            .ok_or_else(|| anyhow!("track '{}' missing resolved audio config", track.id.0))?;
        let (input_stream, output_stream) = build_track_streams(&track.id, resolved, runtime)?;
        streams.push(input_stream);
        streams.push(output_stream);
    }
    Ok(streams)
}

impl ProjectRuntimeController {
    pub fn start(project: &Project) -> Result<Self> {
        let mut controller = Self {
            runtime_graph: RuntimeGraph {
                tracks: HashMap::new(),
            },
            active_tracks: HashMap::new(),
        };
        controller.sync_project(project)?;
        Ok(controller)
    }

    pub fn sync_project(&mut self, project: &Project) -> Result<()> {
        let host = cpal::default_host();
        validate_channels_against_devices(project, &host)?;
        let mut resolved_tracks = resolve_enabled_track_audio_configs(&host, project)?;

        let removed_track_ids = self
            .active_tracks
            .keys()
            .filter(|track_id| !resolved_tracks.contains_key(*track_id))
            .cloned()
            .collect::<Vec<_>>();
        for track_id in removed_track_ids {
            self.active_tracks.remove(&track_id);
            self.runtime_graph.remove_track(&track_id);
        }

        for track in &project.tracks {
            if !track.enabled {
                continue;
            }

            let resolved = resolved_tracks
                .remove(&track.id)
                .ok_or_else(|| anyhow!("track '{}' missing resolved audio config", track.id.0))?;
            let needs_stream_rebuild = self
                .active_tracks
                .get(&track.id)
                .map(|active| active.stream_signature != resolved.stream_signature)
                .unwrap_or(true);

            let runtime = self
                .runtime_graph
                .upsert_track(track, resolved.sample_rate, needs_stream_rebuild)?;

            if needs_stream_rebuild {
                let active = build_active_track_runtime(&track.id, resolved, runtime)?;
                self.active_tracks.insert(track.id.clone(), active);
            }
        }

        Ok(())
    }

    pub fn stop(&mut self) {
        self.active_tracks.clear();
        self.runtime_graph.tracks.clear();
    }

    pub fn is_running(&self) -> bool {
        !self.active_tracks.is_empty()
    }
}

pub fn resolve_project_track_sample_rates(project: &Project) -> Result<HashMap<TrackId, f32>> {
    let host = cpal::default_host();
    let mut sample_rates = HashMap::new();

    for track in &project.tracks {
        if !track.enabled {
            continue;
        }
        let resolved_input = resolve_input_device_for_track(&host, project, track)?;
        let resolved_output = resolve_output_device_for_track(&host, project, track)?;
        let sample_rate = resolve_track_runtime_sample_rate(
            &track.id.0,
            &resolved_input.supported,
            &resolved_output.supported,
        )?;
        sample_rates.insert(track.id.clone(), sample_rate);
    }

    Ok(sample_rates)
}

fn resolve_input_device_for_track(
    host: &cpal::Host,
    project: &Project,
    track: &Track,
) -> Result<ResolvedInputDevice> {
    let settings = project
        .device_settings
        .iter()
        .find(|settings| settings.device_id == track.input_device_id)
        .cloned();
    let device = find_input_device_by_id(host, &track.input_device_id.0)?.ok_or_else(|| {
        anyhow!("input device '{}' not found by device_id", track.input_device_id.0)
    })?;
    let default_config = device.default_input_config().with_context(|| {
        format!(
            "failed to get default input config for '{}'",
            track.input_device_id.0
        )
    })?;
    let supported_ranges = device
        .supported_input_configs()
        .with_context(|| format!("failed to enumerate input configs for '{}'", track.input_device_id.0))?
        .collect::<Vec<_>>();
    let required_channels = required_channel_count(&track.input_channels);
    let supported = select_supported_stream_config(
        &default_config,
        &supported_ranges,
        settings.as_ref().map(|settings| settings.sample_rate),
        required_channels,
        &track.input_device_id.0,
    )?;
    if let Some(settings) = &settings {
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

fn resolve_output_device_for_track(
    host: &cpal::Host,
    project: &Project,
    track: &Track,
) -> Result<ResolvedOutputDevice> {
    let settings = project
        .device_settings
        .iter()
        .find(|settings| settings.device_id == track.output_device_id)
        .cloned();
    let device = find_output_device_by_id(host, &track.output_device_id.0)?.ok_or_else(|| {
        anyhow!("output device '{}' not found by device_id", track.output_device_id.0)
    })?;
    let default_config = device.default_output_config().with_context(|| {
        format!(
            "failed to get default output config for '{}'",
            track.output_device_id.0
        )
    })?;
    let supported_ranges = device
        .supported_output_configs()
        .with_context(|| format!("failed to enumerate output configs for '{}'", track.output_device_id.0))?
        .collect::<Vec<_>>();
    let required_channels = required_channel_count(&track.output_channels);
    let supported = select_supported_stream_config(
        &default_config,
        &supported_ranges,
        settings.as_ref().map(|settings| settings.sample_rate),
        required_channels,
        &track.output_device_id.0,
    )?;
    if let Some(settings) = &settings {
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

fn resolve_enabled_track_audio_configs(
    host: &cpal::Host,
    project: &Project,
) -> Result<HashMap<TrackId, ResolvedTrackAudioConfig>> {
    let mut resolved = HashMap::new();

    for track in &project.tracks {
        if !track.enabled {
            continue;
        }

        let input = resolve_input_device_for_track(host, project, track)?;
        let output = resolve_output_device_for_track(host, project, track)?;
        let sample_rate =
            resolve_track_runtime_sample_rate(&track.id.0, &input.supported, &output.supported)?;

        resolved.insert(
            track.id.clone(),
            ResolvedTrackAudioConfig {
                stream_signature: build_track_stream_signature(track, &input, &output),
                input,
                output,
                sample_rate,
            },
        );
    }

    Ok(resolved)
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
    project: &Project,
    host: &cpal::Host,
) -> Result<()> {
    for track in &project.tracks {
        if !track.enabled {
            continue;
        }
        let input_device = find_input_device_by_id(host, &track.input_device_id.0)?
            .ok_or_else(|| anyhow!("track '{}' missing input device '{}'", track.id.0, track.input_device_id.0))?;
        let total_channels = max_supported_input_channels(&input_device).with_context(|| {
            format!(
                "failed to resolve input channel capacity for '{}'",
                track.input_device_id.0
            )
        })?;
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
        let total_channels = max_supported_output_channels(&output_device).with_context(|| {
            format!(
                "failed to resolve output channel capacity for '{}'",
                track.output_device_id.0
            )
        })?;
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
    track_id: &TrackId,
    resolved_input_device: ResolvedInputDevice,
    runtime: Arc<Mutex<TrackRuntimeState>>,
) -> Result<Stream> {
    let sample_format = resolved_input_device.supported.sample_format();
    let sample_rate = resolved_input_sample_rate(&resolved_input_device);
    let buffer_size_frames = resolved_input_buffer_size_frames(&resolved_input_device);
    let stream_config = build_stream_config(
        resolved_input_device.supported.channels(),
        sample_rate,
        buffer_size_frames,
    );
    let device = resolved_input_device.device;
    let stream = match sample_format {
        SampleFormat::F32 => {
            let runtime_for_data = runtime.clone();
            let channels = stream_config.channels as usize;
            let error_track_id = track_id.0.clone();
            device.build_input_stream(
                &stream_config,
                move |data: &[f32], _| {
                    process_input_f32(&runtime_for_data, data, channels);
                },
                move |err| eprintln!("[{}] input error: {}", error_track_id, err),
                None,
            )?
        }
        SampleFormat::I16 => {
            let runtime_for_data = runtime.clone();
            let channels = stream_config.channels as usize;
            let error_track_id = track_id.0.clone();
            device.build_input_stream(
                &stream_config,
                move |data: &[i16], _| {
                    let converted: Vec<f32> = data
                        .iter()
                        .map(|sample| *sample as f32 / i16::MAX as f32)
                        .collect();
                    process_input_f32(&runtime_for_data, &converted, channels);
                },
                move |err| eprintln!("[{}] input error: {}", error_track_id, err),
                None,
            )?
        }
        SampleFormat::U16 => {
            let runtime_for_data = runtime.clone();
            let channels = stream_config.channels as usize;
            let error_track_id = track_id.0.clone();
            device.build_input_stream(
                &stream_config,
                move |data: &[u16], _| {
                    let converted: Vec<f32> = data
                        .iter()
                        .map(|sample| (*sample as f32 / u16::MAX as f32) * 2.0 - 1.0)
                        .collect();
                    process_input_f32(&runtime_for_data, &converted, channels);
                },
                move |err| eprintln!("[{}] input error: {}", error_track_id, err),
                None,
            )?
        }
        other => {
            bail!(
                "unsupported input sample format for track '{}': {:?}",
                track_id.0,
                other
            );
        }
    };
    Ok(stream)
}
fn build_output_stream_for_track(
    track_id: &TrackId,
    resolved_output_device: ResolvedOutputDevice,
    runtime: Arc<Mutex<TrackRuntimeState>>,
) -> Result<Stream> {
    let sample_format = resolved_output_device.supported.sample_format();
    let sample_rate = resolved_output_sample_rate(&resolved_output_device);
    let buffer_size_frames = resolved_output_buffer_size_frames(&resolved_output_device);
    let stream_config = build_stream_config(
        resolved_output_device.supported.channels(),
        sample_rate,
        buffer_size_frames,
    );
    let device = resolved_output_device.device;
    let stream = match sample_format {
        SampleFormat::F32 => {
            let runtime_for_data = runtime.clone();
            let channels = stream_config.channels as usize;
            let error_track_id = track_id.0.clone();
            device.build_output_stream(
                &stream_config,
                move |out: &mut [f32], _| {
                    process_output_f32(&runtime_for_data, out, channels);
                },
                move |err| eprintln!("[{}] output error: {}", error_track_id, err),
                None,
            )?
        }
        SampleFormat::I16 => {
            let runtime_for_data = runtime.clone();
            let channels = stream_config.channels as usize;
            let error_track_id = track_id.0.clone();
            device.build_output_stream(
                &stream_config,
                move |out: &mut [i16], _| {
                    let mut temp = vec![0.0f32; out.len()];
                    process_output_f32(&runtime_for_data, &mut temp, channels);
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
            let runtime_for_data = runtime.clone();
            let channels = stream_config.channels as usize;
            let error_track_id = track_id.0.clone();
            device.build_output_stream(
                &stream_config,
                move |out: &mut [u16], _| {
                    let mut temp = vec![0.0f32; out.len()];
                    process_output_f32(&runtime_for_data, &mut temp, channels);
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
                track_id.0,
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

fn build_track_streams(
    track_id: &TrackId,
    resolved: ResolvedTrackAudioConfig,
    runtime: Arc<Mutex<TrackRuntimeState>>,
) -> Result<(Stream, Stream)> {
    let input_stream = build_input_stream_for_track(track_id, resolved.input, runtime.clone())?;
    let output_stream = build_output_stream_for_track(track_id, resolved.output, runtime)?;
    Ok((input_stream, output_stream))
}

fn build_active_track_runtime(
    track_id: &TrackId,
    resolved: ResolvedTrackAudioConfig,
    runtime: Arc<Mutex<TrackRuntimeState>>,
) -> Result<ActiveTrackRuntime> {
    let stream_signature = resolved.stream_signature.clone();
    let (input_stream, output_stream) = build_track_streams(track_id, resolved, runtime)?;
    input_stream.play()?;
    output_stream.play()?;
    Ok(ActiveTrackRuntime {
        stream_signature,
        input_stream,
        output_stream,
    })
}

fn build_track_stream_signature(
    track: &Track,
    input: &ResolvedInputDevice,
    output: &ResolvedOutputDevice,
) -> TrackStreamSignature {
    TrackStreamSignature {
        input_device_id: track.input_device_id.0.clone(),
        input_channels: track.input_channels.clone(),
        input_stream_channels: input.supported.channels(),
        input_sample_rate: resolved_input_sample_rate(input),
        input_buffer_size_frames: resolved_input_buffer_size_frames(input),
        output_device_id: track.output_device_id.0.clone(),
        output_channels: track.output_channels.clone(),
        output_stream_channels: output.supported.channels(),
        output_sample_rate: resolved_output_sample_rate(output),
        output_buffer_size_frames: resolved_output_buffer_size_frames(output),
    }
}

fn resolved_input_sample_rate(resolved: &ResolvedInputDevice) -> u32 {
    resolved
        .settings
        .as_ref()
        .map(|settings| settings.sample_rate)
        .unwrap_or_else(|| resolved.supported.sample_rate())
}

fn resolved_output_sample_rate(resolved: &ResolvedOutputDevice) -> u32 {
    resolved
        .settings
        .as_ref()
        .map(|settings| settings.sample_rate)
        .unwrap_or_else(|| resolved.supported.sample_rate())
}

fn resolved_input_buffer_size_frames(resolved: &ResolvedInputDevice) -> u32 {
    resolved
        .settings
        .as_ref()
        .map(|settings| settings.buffer_size_frames)
        .unwrap_or(256)
}

fn resolved_output_buffer_size_frames(resolved: &ResolvedOutputDevice) -> u32 {
    resolved
        .settings
        .as_ref()
        .map(|settings| settings.buffer_size_frames)
        .unwrap_or(256)
}

fn required_channel_count(channels: &[usize]) -> usize {
    channels.iter().copied().max().map(|channel| channel + 1).unwrap_or(0)
}

fn select_supported_stream_config(
    default_config: &SupportedStreamConfig,
    supported_ranges: &[SupportedStreamConfigRange],
    requested_sample_rate: Option<u32>,
    required_channels: usize,
    context: &str,
) -> Result<SupportedStreamConfig> {
    let target_sample_rate = requested_sample_rate.unwrap_or_else(|| default_config.sample_rate());
    let default_format = default_config.sample_format();

    let best = supported_ranges
        .iter()
        .filter(|range| range.channels() as usize >= required_channels)
        .filter_map(|range| range.try_with_sample_rate(target_sample_rate))
        .min_by_key(|config| {
            (
                (config.channels() as usize != required_channels) as u8,
                (config.sample_format() != default_format) as u8,
                (config.channels() as usize).saturating_sub(required_channels),
            )
        });

    best.ok_or_else(|| {
        anyhow!(
            "{} invalid: no supported config for sample_rate={} with at least {} channels",
            context,
            target_sample_rate,
            required_channels
        )
    })
}

fn resolve_track_runtime_sample_rate(
    track_id: &str,
    input: &SupportedStreamConfig,
    output: &SupportedStreamConfig,
) -> Result<f32> {
    if input.sample_rate() != output.sample_rate() {
        bail!(
            "track '{}' invalid: input sample_rate={} differs from output sample_rate={}",
            track_id,
            input.sample_rate(),
            output.sample_rate()
        );
    }

    Ok(input.sample_rate() as f32)
}

fn max_supported_input_channels(device: &cpal::Device) -> Result<usize> {
    let max_supported = device
        .supported_input_configs()?
        .map(|config| config.channels() as usize)
        .max();
    let default_channels = device.default_input_config().ok().map(|config| config.channels() as usize);
    max_supported_channels(default_channels, max_supported)
}

fn max_supported_output_channels(device: &cpal::Device) -> Result<usize> {
    let max_supported = device
        .supported_output_configs()?
        .map(|config| config.channels() as usize)
        .max();
    let default_channels = device.default_output_config().ok().map(|config| config.channels() as usize);
    max_supported_channels(default_channels, max_supported)
}

fn max_supported_channels(
    default_channels: Option<usize>,
    max_supported_channels: Option<usize>,
) -> Result<usize> {
    max_supported_channels
        .or(default_channels)
        .ok_or_else(|| anyhow!("device exposes no supported channels"))
}

#[cfg(test)]
mod tests {
    use super::{
        max_supported_channels, resolve_track_runtime_sample_rate, select_supported_stream_config,
    };
    use cpal::{SampleFormat, SupportedBufferSize, SupportedStreamConfigRange};

    fn supported_range(channels: u16, min_sample_rate: u32, max_sample_rate: u32) -> SupportedStreamConfigRange {
        SupportedStreamConfigRange::new(
            channels,
            min_sample_rate,
            max_sample_rate,
            SupportedBufferSize::Range { min: 64, max: 1024 },
            SampleFormat::F32,
        )
    }

    #[test]
    fn select_supported_stream_config_accepts_non_default_sample_rate_when_device_supports_it() {
        let default_config = supported_range(2, 48_000, 48_000).with_max_sample_rate();
        let supported = vec![
            supported_range(2, 44_100, 96_000),
            supported_range(1, 44_100, 96_000),
        ];

        let resolved = select_supported_stream_config(
            &default_config,
            &supported,
            Some(44_100),
            2,
            "test-device",
        )
        .expect("supported non-default sample rate should resolve");

        assert_eq!(resolved.sample_rate(), 44_100);
        assert_eq!(resolved.channels(), 2);
    }

    #[test]
    fn resolve_track_runtime_sample_rate_rejects_mismatched_input_and_output_sample_rates() {
        let input = supported_range(2, 48_000, 48_000).with_max_sample_rate();
        let output = supported_range(2, 44_100, 44_100).with_max_sample_rate();

        let error = resolve_track_runtime_sample_rate("track:0", &input, &output)
            .expect_err("mismatched rates should fail");

        assert!(error.to_string().contains("sample_rate"));
    }

    #[test]
    fn max_supported_channels_prefers_supported_capacity_over_default() {
        let resolved =
            max_supported_channels(Some(2), Some(8)).expect("supported channels should resolve");

        assert_eq!(resolved, 8);
    }

    #[test]
    fn max_supported_channels_uses_default_when_supported_list_is_empty() {
        let resolved =
            max_supported_channels(Some(2), None).expect("default channels should resolve");

        assert_eq!(resolved, 2);
    }
}
