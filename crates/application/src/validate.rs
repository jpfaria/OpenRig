use anyhow::{anyhow, bail, Result};
use project::block::{schema_for_block_model, AudioBlock, AudioBlockKind};
use project::device::DeviceSettings;
use project::project::Project;
use project::track::Track;
use stage_core::AudioChannelLayout;
use std::collections::{HashMap, HashSet};

pub fn validate_project(project: &Project) -> Result<()> {
    if project.tracks.is_empty() {
        bail!("invalid project: no tracks configured");
    }

    let device_settings_by_id: HashMap<_, _> = project
        .device_settings
        .iter()
        .map(|settings| (settings.device_id.0.clone(), settings))
        .collect();
    validate_device_settings(project, &device_settings_by_id)?;

    for track in &project.tracks {
        if track.input_device_id.0.trim().is_empty() {
            bail!("invalid project: track '{}' missing input_device_id", track.id.0);
        }
        if track.output_device_id.0.trim().is_empty() {
            bail!("invalid project: track '{}' missing output_device_id", track.id.0);
        }
        if track.input_channels.is_empty() {
            bail!("invalid project: track '{}' has no input channels", track.id.0);
        }
        if track.output_channels.is_empty() {
            bail!("invalid project: track '{}' has no output channels", track.id.0);
        }
        validate_unique_channels(&track.input_channels)
            .map_err(|error| anyhow!("invalid project: track '{}': {}", track.id.0, error))?;
        validate_unique_channels(&track.output_channels)
            .map_err(|error| anyhow!("invalid project: track '{}': {}", track.id.0, error))?;

        let input_layout =
            layout_from_channel_count("track input", &track.id.0, track.input_channels.len())?;
        layout_from_channel_count("track output", &track.id.0, track.output_channels.len())?;
        validate_track_blocks(track, track.blocks.as_slice(), input_layout)?;
    }

    validate_active_track_input_channel_conflicts(&project.tracks)?;

    Ok(())
}

fn layout_from_channel_count(
    kind: &str,
    id: &str,
    channel_count: usize,
) -> Result<AudioChannelLayout> {
    match channel_count {
        1 => Ok(AudioChannelLayout::Mono),
        2 => Ok(AudioChannelLayout::Stereo),
        other => bail!(
            "{} '{}' exposes {} channels; only mono (1) and stereo (2) are currently supported",
            kind,
            id,
            other
        ),
    }
}

fn validate_track_blocks(
    track: &Track,
    blocks: &[AudioBlock],
    input_layout: AudioChannelLayout,
) -> Result<()> {
    if !track.enabled {
        return Ok(());
    }
    let mut current_layout = input_layout;

    for block in blocks {
        if !block.enabled {
            continue;
        }
        current_layout = resolve_block_output_layout(track, block, current_layout)?;
    }

    Ok(())
}

fn validate_active_track_input_channel_conflicts(
    tracks: &[Track],
) -> Result<()> {
    let mut claimed_channels: HashMap<(String, usize), String> = HashMap::new();
    for track in tracks.iter().filter(|track| track.enabled) {
        for channel in &track.input_channels {
            let key = (track.input_device_id.0.clone(), *channel);
            if let Some(existing_track) = claimed_channels.insert(key.clone(), track.id.0.clone()) {
                bail!(
                    "invalid project: active tracks '{}' and '{}' both use input device '{}' channel {}",
                    existing_track,
                    track.id.0,
                    key.0,
                    key.1
                );
            }
        }
    }
    Ok(())
}

fn validate_device_settings(
    project: &Project,
    device_settings_by_id: &HashMap<String, &DeviceSettings>,
) -> Result<()> {
    if device_settings_by_id.len() != project.device_settings.len() {
        bail!("invalid project: duplicated device_settings device_id");
    }

    for settings in &project.device_settings {
        if settings.device_id.0.trim().is_empty() {
            bail!("invalid project: device_settings entry missing device_id");
        }
        if settings.sample_rate == 0 {
            bail!(
                "invalid project: device_settings '{}' has invalid sample_rate",
                settings.device_id.0
            );
        }
        if settings.buffer_size_frames == 0 {
            bail!(
                "invalid project: device_settings '{}' has invalid buffer_size_frames",
                settings.device_id.0
            );
        }
    }

    Ok(())
}

fn resolve_block_output_layout(
    track: &Track,
    block: &AudioBlock,
    input_layout: AudioChannelLayout,
) -> Result<AudioChannelLayout> {
    match &block.kind {
        AudioBlockKind::Select(select) => {
            if select.options.is_empty() {
                bail!("block '{}' has no select options", block.id.0);
            }

            let selected = select
                .options
                .iter()
                .find(|option| option.id == select.selected_block_id)
                .ok_or_else(|| anyhow!("block '{}' selected option does not exist", block.id.0))?;

            let mut resolved_layout = None;
            for option in &select.options {
                let option_layout = resolve_block_output_layout(track, option, input_layout)
                    .map_err(|error| anyhow!("block '{}': {}", block.id.0, error))?;
                if let Some(existing) = resolved_layout {
                    if existing != option_layout {
                        bail!(
                            "track '{}' select block '{}' mixes incompatible output layouts across options",
                            track.id.0,
                            block.id.0
                        );
                    }
                } else {
                    resolved_layout = Some(option_layout);
                }
            }

            resolve_block_output_layout(track, selected, input_layout)
                .map_err(|error| anyhow!("block '{}': {}", block.id.0, error))
        }
        AudioBlockKind::Nam(_) | AudioBlockKind::Core(_) => {
            let Some(stage) = block.model_ref() else {
                return Ok(input_layout);
            };

            let schema = schema_for_block_model(stage.effect_type, stage.model)
                .map_err(|error| anyhow!("block '{}': {}", block.id.0, error))?;

            schema.audio_mode.output_layout(input_layout).ok_or_else(|| {
                anyhow!(
                    "track '{}' block '{}' uses {} model '{}' with audio mode '{}' that does not accept a {} input bus",
                    track.id.0,
                    block.id.0,
                    stage.effect_type,
                    stage.model,
                    schema.audio_mode.as_str(),
                    layout_label(input_layout)
                )
            })
        }
    }
}

fn layout_label(layout: AudioChannelLayout) -> &'static str {
    match layout {
        AudioChannelLayout::Mono => "mono",
        AudioChannelLayout::Stereo => "stereo",
    }
}

fn validate_unique_channels(channels: &[usize]) -> Result<()> {
    let mut seen = HashSet::new();
    for channel in channels {
        if !seen.insert(*channel) {
            bail!("duplicated channel '{}'", channel);
        }
    }
    Ok(())
}
