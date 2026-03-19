use anyhow::{anyhow, bail, Result};
use setup::block::{schema_for_block_model, AudioBlock, AudioBlockKind, CoreBlockKind};
use setup::device::InputDevice;
use setup::io::Input;
use setup::setup::Setup;
use setup::track::Track;
use stage_core::AudioChannelLayout;
use std::collections::{HashMap, HashSet};

pub fn validate_setup(setup: &Setup) -> Result<()> {
    if setup.input_devices.is_empty() {
        bail!("invalid setup: no input devices configured");
    }
    if setup.output_devices.is_empty() {
        bail!("invalid setup: no output devices configured");
    }
    if setup.inputs.is_empty() {
        bail!("invalid setup: no inputs configured");
    }
    if setup.outputs.is_empty() {
        bail!("invalid setup: no outputs configured");
    }
    if setup.tracks.is_empty() {
        bail!("invalid setup: no tracks configured");
    }

    let mut input_device_ids = HashSet::new();
    for (index, input_device) in setup.input_devices.iter().enumerate() {
        if input_device.device_id.0.trim().is_empty() {
            bail!("invalid setup: input device at index {} missing device_id", index);
        }
        if input_device.sample_rate == 0 {
            bail!("invalid setup: input device at index {} has invalid sample_rate", index);
        }
        if input_device.buffer_size_frames == 0 {
            bail!(
                "invalid setup: input device at index {} has invalid buffer_size_frames",
                index
            );
        }
        if !input_device_ids.insert(input_device.device_id.clone()) {
            bail!(
                "invalid setup: duplicated input device_id '{}'",
                input_device.device_id.0
            );
        }
    }

    let mut output_device_ids = HashSet::new();
    for (index, output_device) in setup.output_devices.iter().enumerate() {
        if output_device.device_id.0.trim().is_empty() {
            bail!("invalid setup: output device at index {} missing device_id", index);
        }
        if output_device.sample_rate == 0 {
            bail!("invalid setup: output device at index {} has invalid sample_rate", index);
        }
        if output_device.buffer_size_frames == 0 {
            bail!(
                "invalid setup: output device at index {} has invalid buffer_size_frames",
                index
            );
        }
        if !output_device_ids.insert(output_device.device_id.clone()) {
            bail!(
                "invalid setup: duplicated output device_id '{}'",
                output_device.device_id.0
            );
        }
    }

    let mut input_ids = HashSet::new();
    let mut input_layouts = HashMap::new();
    for input in &setup.inputs {
        if input.id.0.trim().is_empty() {
            bail!("invalid setup: input with empty id");
        }
        if input.device >= setup.input_devices.len() {
            bail!(
                "invalid setup: input '{}' references missing input device index {}",
                input.id.0,
                input.device
            );
        }
        if input.channels.is_empty() {
            bail!("invalid setup: input '{}' has no channels", input.id.0);
        }
        validate_unique_channels(&input.channels)
            .map_err(|error| anyhow!("invalid setup: input '{}': {}", input.id.0, error))?;
        if !input_ids.insert(input.id.clone()) {
            bail!("invalid setup: duplicated input id '{}'", input.id.0);
        }
        input_layouts.insert(
            input.id.0.clone(),
            layout_from_channel_count("input", &input.id.0, input.channels.len())?,
        );
    }

    let mut output_ids = HashSet::new();
    for output in &setup.outputs {
        if output.id.0.trim().is_empty() {
            bail!("invalid setup: output with empty id");
        }
        if output.device >= setup.output_devices.len() {
            bail!(
                "invalid setup: output '{}' references missing output device index {}",
                output.id.0,
                output.device
            );
        }
        if output.channels.is_empty() {
            bail!("invalid setup: output '{}' has no channels", output.id.0);
        }
        validate_unique_channels(&output.channels)
            .map_err(|error| anyhow!("invalid setup: output '{}': {}", output.id.0, error))?;
        if !output_ids.insert(output.id.clone()) {
            bail!("invalid setup: duplicated output id '{}'", output.id.0);
        }
        layout_from_channel_count("output", &output.id.0, output.channels.len())?;
    }

    let mut track_ids = HashSet::new();
    let inputs_by_id: HashMap<String, &Input> = setup
        .inputs
        .iter()
        .map(|input| (input.id.0.clone(), input))
        .collect();
    for track in &setup.tracks {
        if track.input_id.0.trim().is_empty() {
            bail!("invalid setup: track '{}' missing input_id", track.id.0);
        }
        if track.output_ids.is_empty() {
            bail!("invalid setup: track '{}' has no outputs", track.id.0);
        }
        if !input_ids.contains(&track.input_id) {
            bail!(
                "invalid setup: track '{}' references missing input '{}'",
                track.id.0,
                track.input_id.0
            );
        }
        for output_id in &track.output_ids {
            if !output_ids.contains(output_id) {
                bail!(
                    "invalid setup: track '{}' references missing output '{}'",
                    track.id.0,
                    output_id.0
                );
            }
        }

        let input_layout = *input_layouts
            .get(&track.input_id.0)
            .expect("validated input id must exist");
        validate_track_blocks(track, input_layout)?;

        if !track_ids.insert(track.id.clone()) {
            bail!("invalid setup: duplicated track id '{}'", track.id.0);
        }
    }

    validate_active_track_input_channel_conflicts(&setup.tracks, &inputs_by_id, &setup.input_devices)?;

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

fn validate_track_blocks(track: &Track, input_layout: AudioChannelLayout) -> Result<()> {
    if !track.enabled {
        return Ok(());
    }
    let mut block_ids = HashSet::new();
    let mut current_layout = input_layout;

    for block in &track.blocks {
        if !block.enabled {
            continue;
        }
        if block.id.0.trim().is_empty() {
            bail!("block with empty id");
        }
        if !block_ids.insert(block.id.clone()) {
            bail!("duplicated block id '{}'", block.id.0);
        }
        block
            .validate_params()
            .map_err(|error| anyhow!("block '{}': {}", block.id.0, error))?;
        current_layout = resolve_block_output_layout(track, block, current_layout)?;
    }

    Ok(())
}

fn validate_active_track_input_channel_conflicts(
    tracks: &[Track],
    inputs_by_id: &HashMap<String, &Input>,
    input_devices: &[InputDevice],
) -> Result<()> {
    let mut claimed_channels: HashMap<(String, usize), String> = HashMap::new();
    for track in tracks.iter().filter(|track| track.enabled) {
        let input = inputs_by_id
            .get(&track.input_id.0)
            .copied()
            .ok_or_else(|| anyhow!("track '{}' references missing input '{}'", track.id.0, track.input_id.0))?;
        let device = input_devices
            .get(input.device)
            .ok_or_else(|| anyhow!("track '{}' references invalid input device index {}", track.id.0, input.device))?;
        for channel in &input.channels {
            let key = (device.device_id.0.clone(), *channel);
            if let Some(existing_track) = claimed_channels.insert(key.clone(), track.id.0.clone()) {
                bail!(
                    "invalid setup: active tracks '{}' and '{}' both use input device '{}' channel {}",
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
        _ => {
            let Some((effect_type, model)) = model_ref_for_block(block) else {
                return Ok(input_layout);
            };

            let schema = schema_for_block_model(effect_type, model)
                .map_err(|error| anyhow!("block '{}': {}", block.id.0, error))?;

            schema.audio_mode.output_layout(input_layout).ok_or_else(|| {
                anyhow!(
                    "track '{}' block '{}' uses {} model '{}' with audio mode '{}' that does not accept a {} input bus",
                    track.id.0,
                    block.id.0,
                    effect_type,
                    model,
                    schema.audio_mode.as_str(),
                    layout_label(input_layout)
                )
            })
        }
    }
}

fn model_ref_for_block(block: &AudioBlock) -> Option<(&str, &str)> {
    match &block.kind {
        AudioBlockKind::Nam(stage) => Some(("nam", stage.model.as_str())),
        AudioBlockKind::Core(core) => match &core.kind {
            CoreBlockKind::AmpHead(stage) => Some(("amp_head", stage.model.as_str())),
            CoreBlockKind::AmpCombo(stage) => Some(("amp_combo", stage.model.as_str())),
            CoreBlockKind::Drive(stage) => Some(("drive", stage.model.as_str())),
            CoreBlockKind::Delay(stage) => Some(("delay", stage.model.as_str())),
            CoreBlockKind::Reverb(stage) => Some(("reverb", stage.model.as_str())),
            CoreBlockKind::Tuner(stage) => Some(("tuner", stage.model.as_str())),
            CoreBlockKind::Compressor(stage) => Some(("compressor", stage.model.as_str())),
            CoreBlockKind::Gate(stage) => Some(("gate", stage.model.as_str())),
            CoreBlockKind::Eq(stage) => Some(("eq", stage.model.as_str())),
            CoreBlockKind::Tremolo(stage) => Some(("tremolo", stage.model.as_str())),
            _ => None,
        },
        _ => None,
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
