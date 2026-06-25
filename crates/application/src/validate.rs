use anyhow::{anyhow, bail, Result};
use block_core::AudioChannelLayout;
use project::block::{schema_for_block_model, AudioBlock, AudioBlockKind};
use project::chain::Chain;
use project::device::DeviceSettings;
use project::project::Project;
use std::collections::HashMap;

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
        // #716 (model A): a binding-bound chain carries only its effect blocks +
        // the selected `io_binding_ids`; its input/output is resolved from the
        // per-machine I/O binding registry at runtime, NOT stored on the chain.
        // The structural block checks below do not apply to it.
        if !chain.io_binding_ids.is_empty() {
            continue;
        }

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

        // #716 (model A): Input/Output blocks no longer embed device endpoints
        // (`entries` are gone). The device / channels / mode of every endpoint
        // are resolved from the per-machine I/O binding registry via
        // `engine::runtime_endpoints::resolve_chain_io`, which `validate_project`
        // does not have access to. The per-entry device_id / channel /
        // unique-channel validation and the cross-chain channel-conflict check
        // therefore move to the registry / activation layer (a separate task).
        // Only the registry-independent structural + layout checks remain here.
        //
        // The processing-layout bus is seeded as `Stereo`: invariant #5 — every
        // stream is ALWAYS stereo internally (a mono physical input is broadcast
        // to `Stereo([s, s])` before the block chain). Without the registry we
        // cannot know the physical channel count, but the internal bus the block
        // chain sees is always stereo, so a `true_stereo` model must validate
        // here regardless of the physical input width (#696).
        let input_layout = AudioChannelLayout::Stereo;

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

    Ok(())
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

#[cfg(test)]
#[path = "validate_tests.rs"]
mod tests;
