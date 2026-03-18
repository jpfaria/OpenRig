use anyhow::{anyhow, bail, Result};
use setup::block::{AudioBlockKind, CoreBlockKind};
use setup::setup::Setup;
use setup::track::Track;
use std::collections::HashSet;
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
    for input_device in &setup.input_devices {
        if input_device.id.0.trim().is_empty() {
            bail!("invalid setup: input device with empty id");
        }
        if input_device.match_name.trim().is_empty() {
            bail!("invalid setup: input device '{}' missing match_name", input_device.id.0);
        }
        if input_device.sample_rate == 0 {
            bail!("invalid setup: input device '{}' has invalid sample_rate", input_device.id.0);
        }
        if input_device.buffer_size_frames == 0 {
            bail!("invalid setup: input device '{}' has invalid buffer_size_frames", input_device.id.0);
        }
        if !input_device_ids.insert(input_device.id.clone()) {
            bail!("invalid setup: duplicated input device id '{}'", input_device.id.0);
        }
    }
    let mut output_device_ids = HashSet::new();
    for output_device in &setup.output_devices {
        if output_device.id.0.trim().is_empty() {
            bail!("invalid setup: output device with empty id");
        }
        if output_device.match_name.trim().is_empty() {
            bail!("invalid setup: output device '{}' missing match_name", output_device.id.0);
        }
        if output_device.sample_rate == 0 {
            bail!("invalid setup: output device '{}' has invalid sample_rate", output_device.id.0);
        }
        if output_device.buffer_size_frames == 0 {
            bail!("invalid setup: output device '{}' has invalid buffer_size_frames", output_device.id.0);
        }
        if !output_device_ids.insert(output_device.id.clone()) {
            bail!("invalid setup: duplicated output device id '{}'", output_device.id.0);
        }
    }
    let mut input_ids = HashSet::new();
    for input in &setup.inputs {
        if input.id.0.trim().is_empty() {
            bail!("invalid setup: input with empty id");
        }
        if input.device_id.0.trim().is_empty() {
            bail!("invalid setup: input '{}' missing device_id", input.id.0);
        }
        if input.channels.is_empty() {
            bail!("invalid setup: input '{}' has no channels", input.id.0);
        }
        validate_unique_channels(&input.channels)
            .map_err(|error| anyhow!("invalid setup: input '{}': {}", input.id.0, error))?;
        if !input_ids.insert(input.id.clone()) {
            bail!("invalid setup: duplicated input id '{}'", input.id.0);
        }
    }
    let mut output_ids = HashSet::new();
    for output in &setup.outputs {
        if output.id.0.trim().is_empty() {
            bail!("invalid setup: output with empty id");
        }
        if output.device_id.0.trim().is_empty() {
            bail!("invalid setup: output '{}' missing device_id", output.id.0);
        }
        if output.channels.is_empty() {
            bail!("invalid setup: output '{}' has no channels", output.id.0);
        }
        validate_unique_channels(&output.channels)
            .map_err(|error| anyhow!("invalid setup: output '{}': {}", output.id.0, error))?;
        if !output_ids.insert(output.id.clone()) {
            bail!("invalid setup: duplicated output id '{}'", output.id.0);
        }
    }
    let mut track_ids = HashSet::new();
    for track in &setup.tracks {
        if track.id.0.trim().is_empty() {
            bail!("invalid setup: track with empty id");
        }
        if track.input_id.0.trim().is_empty() {
            bail!("invalid setup: track '{}' missing input_id", track.id.0);
        }
        if track.output_ids.is_empty() {
            bail!("invalid setup: track '{}' has no outputs", track.id.0);
        }
        if !input_ids.contains(&track.input_id) {
            bail!("invalid setup: track '{}' references missing input '{}'", track.id.0, track.input_id.0);
        }
        for output_id in &track.output_ids {
            if !output_ids.contains(output_id) {
                bail!("invalid setup: track '{}' references missing output '{}'", track.id.0, output_id.0);
            }
        }
        validate_track_blocks(track)?;
        if !track_ids.insert(track.id.clone()) {
            bail!("invalid setup: duplicated track id '{}'", track.id.0);
        }
    }
    Ok(())
}
fn validate_track_blocks(track: &Track) -> Result<()> {
    let mut block_ids = HashSet::new();
    for block in &track.blocks {
        if block.id.0.trim().is_empty() {
            bail!("block with empty id");
        }
        if !block_ids.insert(block.id.clone()) {
            bail!("duplicated block id '{}'", block.id.0);
        }
        match &block.kind {
            AudioBlockKind::Nam(stage) => {
                if stage.model.trim().is_empty() {
                    bail!("block '{}' missing nam model", block.id.0);
                }
                if stage.params.model_path.trim().is_empty() {
                    bail!("block '{}' missing model_path", block.id.0);
                }
            }
            AudioBlockKind::Core(core) => match &core.kind {
                CoreBlockKind::Delay(stage) => {
                    if stage.model.trim().is_empty() {
                        bail!("block '{}' missing delay model", block.id.0);
                    }
                }
                CoreBlockKind::Reverb(stage) => {
                    if stage.model.trim().is_empty() {
                        bail!("block '{}' missing reverb model", block.id.0);
                    }
                }
                CoreBlockKind::Tuner(stage) => {
                    if stage.model.trim().is_empty() {
                        bail!("block '{}' missing tuner model", block.id.0);
                    }
                }
                CoreBlockKind::Compressor(stage) => {
                    if stage.model.trim().is_empty() {
                        bail!("block '{}' missing compressor model", block.id.0);
                    }
                }
                CoreBlockKind::Gate(stage) => {
                    if stage.model.trim().is_empty() {
                        bail!("block '{}' missing gate model", block.id.0);
                    }
                }
                CoreBlockKind::Eq(stage) => {
                    if stage.model.trim().is_empty() {
                        bail!("block '{}' missing eq model", block.id.0);
                    }
                }
                CoreBlockKind::Tremolo(stage) => {
                    if stage.model.trim().is_empty() {
                        bail!("block '{}' missing tremolo model", block.id.0);
                    }
                }
                _ => {}
            },
            AudioBlockKind::Select(select) => {
                if select.options.is_empty() {
                    bail!("block '{}' has no select options", block.id.0);
                }
                if !select.options.iter().any(|option| option.id == select.selected_block_id) {
                    bail!("block '{}' selected option does not exist", block.id.0);
                }
            }
            _ => {}
        }
    }
    Ok(())
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
