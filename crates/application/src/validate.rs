use anyhow::{anyhow, bail, Result};
use block_core::AudioChannelLayout;
use project::block::{schema_for_block_model, AudioBlock, AudioBlockKind};
use project::chain::Chain;
use project::device::DeviceSettings;
use project::project::Project;
use std::collections::{HashMap, HashSet};

pub fn validate_project(project: &Project) -> Result<()> {
    if project.chains.is_empty() {
        bail!("invalid project: no chains configured");
    }

    let device_settings_by_id: HashMap<_, _> = project
        .device_settings
        .iter()
        .map(|settings| (settings.device_id.0.clone(), settings))
        .collect();
    validate_device_settings(project, &device_settings_by_id)?;

    for chain in &project.chains {
        let input_blocks = chain.input_blocks();
        let output_blocks = chain.output_blocks();

        if input_blocks.is_empty() {
            bail!(
                "invalid project: chain '{}' has no input blocks",
                chain.id.0
            );
        }
        if output_blocks.is_empty() {
            bail!(
                "invalid project: chain '{}' has no output blocks",
                chain.id.0
            );
        }

        // Validate each input block's entries
        for (_, input) in &input_blocks {
            for (entry_idx, entry) in input.entries.iter().enumerate() {
                let entry_label = format!("{}[{}]", input.model, entry_idx);
                if entry.device_id.0.trim().is_empty() {
                    bail!(
                        "invalid project: chain '{}' input '{}' missing device_id",
                        chain.id.0,
                        entry_label
                    );
                }
                if entry.channels.is_empty() {
                    bail!(
                        "invalid project: chain '{}' input '{}' has no channels",
                        chain.id.0,
                        entry_label
                    );
                }
                validate_unique_channels(&entry.channels).map_err(|error| {
                    anyhow!(
                        "invalid project: chain '{}' input '{}': {}",
                        chain.id.0,
                        entry_label,
                        error
                    )
                })?;
            }
        }

        // Validate each output block's entries
        for (_, output) in &output_blocks {
            for (entry_idx, entry) in output.entries.iter().enumerate() {
                let entry_label = format!("{}[{}]", output.model, entry_idx);
                if entry.device_id.0.trim().is_empty() {
                    bail!(
                        "invalid project: chain '{}' output '{}' missing device_id",
                        chain.id.0,
                        entry_label
                    );
                }
                if entry.channels.is_empty() {
                    bail!(
                        "invalid project: chain '{}' output '{}' has no channels",
                        chain.id.0,
                        entry_label
                    );
                }
                validate_unique_channels(&entry.channels).map_err(|error| {
                    anyhow!(
                        "invalid project: chain '{}' output '{}': {}",
                        chain.id.0,
                        entry_label,
                        error
                    )
                })?;
            }
        }

        // Use first input entry's channel count for layout determination
        let first_input = input_blocks.first().expect("validated non-empty");
        let first_input_entry = first_input.1.entries.first().ok_or_else(|| {
            anyhow!(
                "invalid project: chain '{}' input '{}' has no entries",
                chain.id.0,
                first_input.1.model
            )
        })?;
        let input_layout = layout_from_channel_count(
            "chain input",
            &chain.id.0,
            first_input_entry.channels.len(),
        )?;
        let first_output = output_blocks.first().expect("validated non-empty");
        let first_output_entry = first_output.1.entries.first().ok_or_else(|| {
            anyhow!(
                "invalid project: chain '{}' output '{}' has no entries",
                chain.id.0,
                first_output.1.model
            )
        })?;
        layout_from_channel_count(
            "chain output",
            &chain.id.0,
            first_output_entry.channels.len(),
        )?;

        // Validate only audio blocks (non-I/O, non-Insert)
        let audio_blocks: Vec<&AudioBlock> = chain
            .blocks
            .iter()
            .filter(|b| {
                !matches!(
                    &b.kind,
                    AudioBlockKind::Input(_)
                        | AudioBlockKind::Output(_)
                        | AudioBlockKind::Insert(_)
                )
            })
            .collect();
        validate_chain_blocks(chain, &audio_blocks, input_layout)?;
    }

    validate_active_chain_input_channel_conflicts(&project.chains)?;

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

fn validate_chain_blocks(
    chain: &Chain,
    blocks: &[&AudioBlock],
    input_layout: AudioChannelLayout,
) -> Result<()> {
    if !chain.enabled {
        return Ok(());
    }
    let mut current_layout = input_layout;

    for block in blocks {
        if !block.enabled {
            continue;
        }
        current_layout = resolve_block_output_layout(chain, block, current_layout)?;
    }

    Ok(())
}

fn validate_active_chain_input_channel_conflicts(chains: &[Chain]) -> Result<()> {
    let mut claimed_channels: HashMap<(String, usize), String> = HashMap::new();
    for chain in chains.iter().filter(|chain| chain.enabled) {
        for (_, input) in chain.input_blocks() {
            for entry in &input.entries {
                for channel in &entry.channels {
                    let key = (entry.device_id.0.clone(), *channel);
                    if let Some(existing_chain) =
                        claimed_channels.insert(key.clone(), chain.id.0.clone())
                    {
                        bail!(
                            "invalid project: active chains '{}' and '{}' both use input device '{}' channel {}",
                            existing_chain,
                            chain.id.0,
                            key.0,
                            key.1
                        );
                    }
                }
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
    chain: &Chain,
    block: &AudioBlock,
    input_layout: AudioChannelLayout,
) -> Result<AudioChannelLayout> {
    block
        .validate_params()
        .map_err(|error| anyhow!("block '{}': {}", block.id.0, error))?;

    match &block.kind {
        AudioBlockKind::Select(select) => {
            let selected = select
                .selected_option()
                .ok_or_else(|| anyhow!("block '{}' selected option does not exist", block.id.0))?;

            let mut resolved_layout = None;
            for option in &select.options {
                let option_layout = resolve_block_output_layout(chain, option, input_layout)
                    .map_err(|error| anyhow!("block '{}': {}", block.id.0, error))?;
                if let Some(existing) = resolved_layout {
                    if existing != option_layout {
                        bail!(
                            "chain '{}' select block '{}' mixes incompatible output layouts across options",
                            chain.id.0,
                            block.id.0
                        );
                    }
                } else {
                    resolved_layout = Some(option_layout);
                }
            }

            resolve_block_output_layout(chain, selected, input_layout)
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
                    "chain '{}' block '{}' uses {} model '{}' with audio mode '{}' that does not accept a {} input bus",
                    chain.id.0,
                    block.id.0,
                    stage.effect_type,
                    stage.model,
                    schema.audio_mode.as_str(),
                    layout_label(input_layout)
                )
            })
        }
        // Input/Output/Insert blocks don't affect audio processing layout
        AudioBlockKind::Input(_) | AudioBlockKind::Output(_) | AudioBlockKind::Insert(_) => {
            Ok(input_layout)
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

#[cfg(test)]
#[path = "validate_tests.rs"]
mod tests;
