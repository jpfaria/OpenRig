use anyhow::{anyhow, bail, Result};
use project::block::{schema_for_block_model, AudioBlock, AudioBlockKind};
use project::device::DeviceSettings;
use project::project::Project;
use project::chain::Chain;
use block_core::AudioChannelLayout;
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
                validate_unique_channels(&entry.channels)
                    .map_err(|error| anyhow!("invalid project: chain '{}' input '{}': {}", chain.id.0, entry_label, error))?;
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
                validate_unique_channels(&entry.channels)
                    .map_err(|error| anyhow!("invalid project: chain '{}' output '{}': {}", chain.id.0, entry_label, error))?;
            }
        }

        // Use first input entry's channel count for layout determination
        let first_input = input_blocks.first().expect("validated non-empty");
        let first_input_entry = first_input.1.entries.first()
            .ok_or_else(|| anyhow!("invalid project: chain '{}' input '{}' has no entries", chain.id.0, first_input.1.model))?;
        let input_layout =
            layout_from_channel_count("chain input", &chain.id.0, first_input_entry.channels.len())?;
        let first_output = output_blocks.first().expect("validated non-empty");
        let first_output_entry = first_output.1.entries.first()
            .ok_or_else(|| anyhow!("invalid project: chain '{}' output '{}' has no entries", chain.id.0, first_output.1.model))?;
        layout_from_channel_count("chain output", &chain.id.0, first_output_entry.channels.len())?;

        // Validate only audio blocks (non-I/O, non-Insert)
        let audio_blocks: Vec<&AudioBlock> = chain.blocks.iter()
            .filter(|b| !matches!(&b.kind, AudioBlockKind::Input(_) | AudioBlockKind::Output(_) | AudioBlockKind::Insert(_)))
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
                    if let Some(existing_chain) = claimed_channels.insert(key.clone(), chain.id.0.clone()) {
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
            let selected = select.selected_option().ok_or_else(|| {
                anyhow!("block '{}' selected option does not exist", block.id.0)
            })?;

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
        AudioBlockKind::Input(_) | AudioBlockKind::Output(_) | AudioBlockKind::Insert(_) => Ok(input_layout),
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
mod tests {
    use super::*;
    use project::block::{
        AudioBlock, AudioBlockKind, CoreBlock, InputBlock, InputEntry, InsertBlock,
        InsertEndpoint, OutputBlock, OutputEntry,
    };
    use project::chain::{Chain, ChainInputMode, ChainOutputMode};
    use project::device::DeviceSettings;
    use project::project::Project;
    use domain::ids::{BlockId, ChainId, DeviceId};
    use project::param::ParameterSet;

    // -----------------------------------------------------------------------
    // Helper functions
    // -----------------------------------------------------------------------

    fn test_input_entry(_name: &str, device_id: &str, channels: Vec<usize>) -> InputEntry {
        InputEntry {
            device_id: DeviceId(device_id.to_string()),
            mode: ChainInputMode::Mono,
            channels,
        }
    }

    fn test_input_block(device_id: &str, channels: Vec<usize>) -> AudioBlock {
        AudioBlock {
            id: BlockId("block:input".to_string()),
            enabled: true,
            kind: AudioBlockKind::Input(InputBlock {
                model: "standard".to_string(),
                entries: vec![test_input_entry("Input 1", device_id, channels)],
            }),
        }
    }

    fn test_output_entry(_name: &str, device_id: &str, channels: Vec<usize>) -> OutputEntry {
        OutputEntry {
            device_id: DeviceId(device_id.to_string()),
            mode: ChainOutputMode::Stereo,
            channels,
        }
    }

    fn test_output_block(device_id: &str, channels: Vec<usize>) -> AudioBlock {
        AudioBlock {
            id: BlockId("block:output".to_string()),
            enabled: true,
            kind: AudioBlockKind::Output(OutputBlock {
                model: "standard".to_string(),
                entries: vec![test_output_entry("Output 1", device_id, channels)],
            }),
        }
    }

    fn test_chain(id: &str, blocks: Vec<AudioBlock>) -> Chain {
        Chain {
            id: ChainId(id.to_string()),
            description: None,
            instrument: "electric_guitar".to_string(),
            enabled: true,
            blocks,
        }
    }

    fn test_device_settings(device_id: &str) -> DeviceSettings {
        DeviceSettings {
            device_id: DeviceId(device_id.to_string()),
            sample_rate: 48000,
            buffer_size_frames: 256,
        }
    }

    fn test_project(chains: Vec<Chain>) -> Project {
        // Collect unique device_ids from all I/O entries
        let mut device_ids: Vec<String> = Vec::new();
        for chain in &chains {
            for block in &chain.blocks {
                match &block.kind {
                    AudioBlockKind::Input(input) => {
                        for entry in &input.entries {
                            if !entry.device_id.0.trim().is_empty()
                                && !device_ids.contains(&entry.device_id.0)
                            {
                                device_ids.push(entry.device_id.0.clone());
                            }
                        }
                    }
                    AudioBlockKind::Output(output) => {
                        for entry in &output.entries {
                            if !entry.device_id.0.trim().is_empty()
                                && !device_ids.contains(&entry.device_id.0)
                            {
                                device_ids.push(entry.device_id.0.clone());
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        Project {
            name: Some("test".to_string()),
            device_settings: device_ids.iter().map(|id| test_device_settings(id)).collect(),
            chains,
        }
    }

    fn valid_chain(id: &str) -> Chain {
        test_chain(
            id,
            vec![
                test_input_block("dev-in", vec![0]),
                test_output_block("dev-out", vec![0, 1]),
            ],
        )
    }

    fn valid_project() -> Project {
        test_project(vec![valid_chain("chain:0")])
    }

    // -----------------------------------------------------------------------
    // validate_project — happy path
    // -----------------------------------------------------------------------

    #[test]
    fn validate_project_valid_project_succeeds() {
        let project = valid_project();
        assert!(validate_project(&project).is_ok());
    }

    #[test]
    fn validate_project_mono_input_mono_output_succeeds() {
        let project = test_project(vec![test_chain(
            "chain:0",
            vec![
                test_input_block("dev-in", vec![0]),
                test_output_block("dev-out", vec![0]),
            ],
        )]);
        assert!(validate_project(&project).is_ok());
    }

    #[test]
    fn validate_project_stereo_input_stereo_output_succeeds() {
        let project = test_project(vec![test_chain(
            "chain:0",
            vec![
                test_input_block("dev-in", vec![0, 1]),
                test_output_block("dev-out", vec![0, 1]),
            ],
        )]);
        assert!(validate_project(&project).is_ok());
    }

    #[test]
    fn validate_project_multiple_chains_succeeds() {
        let chain0 = test_chain(
            "chain:0",
            vec![
                test_input_block("dev-in-a", vec![0]),
                test_output_block("dev-out-a", vec![0, 1]),
            ],
        );
        let chain1 = test_chain(
            "chain:1",
            vec![
                test_input_block("dev-in-b", vec![0]),
                test_output_block("dev-out-b", vec![0, 1]),
            ],
        );
        let project = test_project(vec![chain0, chain1]);
        assert!(validate_project(&project).is_ok());
    }

    // -----------------------------------------------------------------------
    // validate_project — empty chains
    // -----------------------------------------------------------------------

    #[test]
    fn validate_project_empty_chains_fails() {
        let project = Project {
            name: Some("test".to_string()),
            device_settings: Vec::new(),
            chains: Vec::new(),
        };
        let err = validate_project(&project).unwrap_err();
        assert!(err.to_string().contains("no chains configured"));
    }

    // -----------------------------------------------------------------------
    // validate_project — missing input/output blocks
    // -----------------------------------------------------------------------

    #[test]
    fn validate_project_no_input_block_fails() {
        let chain = test_chain("chain:0", vec![test_output_block("dev-out", vec![0, 1])]);
        let project = test_project(vec![chain]);
        let err = validate_project(&project).unwrap_err();
        assert!(err.to_string().contains("no input blocks"));
    }

    #[test]
    fn validate_project_no_output_block_fails() {
        let chain = test_chain("chain:0", vec![test_input_block("dev-in", vec![0])]);
        let project = test_project(vec![chain]);
        let err = validate_project(&project).unwrap_err();
        assert!(err.to_string().contains("no output blocks"));
    }

    // -----------------------------------------------------------------------
    // validate_project — empty device_id in entries
    // -----------------------------------------------------------------------

    #[test]
    fn validate_project_empty_input_device_id_fails() {
        let chain = test_chain(
            "chain:0",
            vec![
                test_input_block("", vec![0]),
                test_output_block("dev-out", vec![0, 1]),
            ],
        );
        let project = test_project(vec![chain]);
        let err = validate_project(&project).unwrap_err();
        assert!(err.to_string().contains("missing device_id"));
    }

    #[test]
    fn validate_project_whitespace_input_device_id_fails() {
        let chain = test_chain(
            "chain:0",
            vec![
                test_input_block("  ", vec![0]),
                test_output_block("dev-out", vec![0, 1]),
            ],
        );
        // Fix up the device settings since whitespace device_id won't auto-generate settings
        let project = Project {
            name: Some("test".to_string()),
            device_settings: vec![test_device_settings("dev-out")],
            chains: vec![chain],
        };
        let err = validate_project(&project).unwrap_err();
        assert!(err.to_string().contains("missing device_id"));
    }

    #[test]
    fn validate_project_empty_output_device_id_fails() {
        let chain = test_chain(
            "chain:0",
            vec![
                test_input_block("dev-in", vec![0]),
                test_output_block("", vec![0, 1]),
            ],
        );
        let project = test_project(vec![chain]);
        let err = validate_project(&project).unwrap_err();
        assert!(err.to_string().contains("missing device_id"));
    }

    // -----------------------------------------------------------------------
    // validate_project — empty channels
    // -----------------------------------------------------------------------

    #[test]
    fn validate_project_empty_input_channels_fails() {
        let chain = test_chain(
            "chain:0",
            vec![
                test_input_block("dev-in", vec![]),
                test_output_block("dev-out", vec![0, 1]),
            ],
        );
        let project = test_project(vec![chain]);
        let err = validate_project(&project).unwrap_err();
        assert!(err.to_string().contains("has no channels"));
    }

    #[test]
    fn validate_project_empty_output_channels_fails() {
        let chain = test_chain(
            "chain:0",
            vec![
                test_input_block("dev-in", vec![0]),
                test_output_block("dev-out", vec![]),
            ],
        );
        let project = test_project(vec![chain]);
        let err = validate_project(&project).unwrap_err();
        assert!(err.to_string().contains("has no channels"));
    }

    // -----------------------------------------------------------------------
    // validate_project — duplicate channels
    // -----------------------------------------------------------------------

    #[test]
    fn validate_project_duplicate_input_channels_fails() {
        let chain = test_chain(
            "chain:0",
            vec![
                test_input_block("dev-in", vec![0, 0]),
                test_output_block("dev-out", vec![0, 1]),
            ],
        );
        let project = test_project(vec![chain]);
        let err = validate_project(&project).unwrap_err();
        assert!(err.to_string().contains("duplicated channel"));
    }

    #[test]
    fn validate_project_duplicate_output_channels_fails() {
        let chain = test_chain(
            "chain:0",
            vec![
                test_input_block("dev-in", vec![0]),
                test_output_block("dev-out", vec![1, 1]),
            ],
        );
        let project = test_project(vec![chain]);
        let err = validate_project(&project).unwrap_err();
        assert!(err.to_string().contains("duplicated channel"));
    }

    // -----------------------------------------------------------------------
    // validate_project — device settings validation
    // -----------------------------------------------------------------------

    #[test]
    fn validate_project_zero_sample_rate_fails() {
        let mut project = valid_project();
        project.device_settings[0].sample_rate = 0;
        let err = validate_project(&project).unwrap_err();
        assert!(err.to_string().contains("invalid sample_rate"));
    }

    #[test]
    fn validate_project_zero_buffer_size_fails() {
        let mut project = valid_project();
        project.device_settings[0].buffer_size_frames = 0;
        let err = validate_project(&project).unwrap_err();
        assert!(err.to_string().contains("invalid buffer_size_frames"));
    }

    #[test]
    fn validate_project_empty_device_settings_device_id_fails() {
        let mut project = valid_project();
        project.device_settings.push(DeviceSettings {
            device_id: DeviceId("".to_string()),
            sample_rate: 48000,
            buffer_size_frames: 256,
        });
        let err = validate_project(&project).unwrap_err();
        assert!(err.to_string().contains("missing device_id"));
    }

    #[test]
    fn validate_project_duplicate_device_settings_fails() {
        let mut project = valid_project();
        let dup = project.device_settings[0].clone();
        project.device_settings.push(dup);
        let err = validate_project(&project).unwrap_err();
        assert!(err.to_string().contains("duplicated device_settings"));
    }

    // -----------------------------------------------------------------------
    // validate_project — channel conflicts between active chains
    // -----------------------------------------------------------------------

    #[test]
    fn validate_project_active_chains_same_input_channel_fails() {
        let chain0 = test_chain(
            "chain:0",
            vec![
                test_input_block("dev-in", vec![0]),
                test_output_block("dev-out-a", vec![0, 1]),
            ],
        );
        let chain1 = test_chain(
            "chain:1",
            vec![
                test_input_block("dev-in", vec![0]), // same device+channel
                test_output_block("dev-out-b", vec![0, 1]),
            ],
        );
        let project = test_project(vec![chain0, chain1]);
        let err = validate_project(&project).unwrap_err();
        assert!(err.to_string().contains("both use input device"));
    }

    #[test]
    fn validate_project_active_chains_different_channels_succeeds() {
        let chain0 = test_chain(
            "chain:0",
            vec![
                test_input_block("dev-in", vec![0]),
                test_output_block("dev-out", vec![0, 1]),
            ],
        );
        let chain1 = test_chain(
            "chain:1",
            vec![
                test_input_block("dev-in", vec![1]), // different channel, same device
                test_output_block("dev-out", vec![0, 1]),
            ],
        );
        let project = test_project(vec![chain0, chain1]);
        assert!(validate_project(&project).is_ok());
    }

    #[test]
    fn validate_project_disabled_chain_skips_conflict_check() {
        let chain0 = test_chain(
            "chain:0",
            vec![
                test_input_block("dev-in", vec![0]),
                test_output_block("dev-out", vec![0, 1]),
            ],
        );
        let mut chain1 = test_chain(
            "chain:1",
            vec![
                test_input_block("dev-in", vec![0]),
                test_output_block("dev-out", vec![0, 1]),
            ],
        );
        chain1.enabled = false; // disabled chain should be ignored for conflict
        let project = test_project(vec![chain0, chain1]);
        assert!(validate_project(&project).is_ok());
    }

    // -----------------------------------------------------------------------
    // validate_project — input entry with empty name uses model as label
    // -----------------------------------------------------------------------

    #[test]
    fn validate_project_input_entry_empty_name_uses_model_in_error() {
        let input = AudioBlock {
            id: BlockId("block:input".to_string()),
            enabled: true,
            kind: AudioBlockKind::Input(InputBlock {
                model: "standard".to_string(),
                entries: vec![InputEntry {
                    device_id: DeviceId("".to_string()),
                    mode: ChainInputMode::Mono,
                    channels: vec![0],
                }],
            }),
        };
        let chain = test_chain("chain:0", vec![input, test_output_block("dev-out", vec![0, 1])]);
        let project = test_project(vec![chain]);
        let err = validate_project(&project).unwrap_err();
        assert!(err.to_string().contains("standard"));
    }

    #[test]
    fn validate_project_output_entry_empty_name_uses_model_in_error() {
        let output = AudioBlock {
            id: BlockId("block:output".to_string()),
            enabled: true,
            kind: AudioBlockKind::Output(OutputBlock {
                model: "standard".to_string(),
                entries: vec![OutputEntry {
                    device_id: DeviceId("".to_string()),
                    mode: ChainOutputMode::Stereo,
                    channels: vec![0, 1],
                }],
            }),
        };
        let chain = test_chain("chain:0", vec![test_input_block("dev-in", vec![0]), output]);
        let project = test_project(vec![chain]);
        let err = validate_project(&project).unwrap_err();
        assert!(err.to_string().contains("standard"));
    }

    // -----------------------------------------------------------------------
    // validate_project — input with no entries
    // -----------------------------------------------------------------------

    #[test]
    fn validate_project_input_no_entries_fails() {
        let input = AudioBlock {
            id: BlockId("block:input".to_string()),
            enabled: true,
            kind: AudioBlockKind::Input(InputBlock {
                model: "standard".to_string(),
                entries: vec![],
            }),
        };
        let chain = test_chain("chain:0", vec![input, test_output_block("dev-out", vec![0, 1])]);
        let project = test_project(vec![chain]);
        let err = validate_project(&project).unwrap_err();
        assert!(err.to_string().contains("has no entries"));
    }

    #[test]
    fn validate_project_output_no_entries_fails() {
        let output = AudioBlock {
            id: BlockId("block:output".to_string()),
            enabled: true,
            kind: AudioBlockKind::Output(OutputBlock {
                model: "standard".to_string(),
                entries: vec![],
            }),
        };
        let chain = test_chain("chain:0", vec![test_input_block("dev-in", vec![0]), output]);
        let project = test_project(vec![chain]);
        let err = validate_project(&project).unwrap_err();
        assert!(err.to_string().contains("has no entries"));
    }

    // -----------------------------------------------------------------------
    // validate_project — unsupported channel count (e.g. 3 channels)
    // -----------------------------------------------------------------------

    #[test]
    fn validate_project_three_input_channels_fails() {
        let chain = test_chain(
            "chain:0",
            vec![
                test_input_block("dev-in", vec![0, 1, 2]),
                test_output_block("dev-out", vec![0, 1]),
            ],
        );
        let project = test_project(vec![chain]);
        let err = validate_project(&project).unwrap_err();
        assert!(err.to_string().contains("3 channels"));
    }

    #[test]
    fn validate_project_three_output_channels_fails() {
        let chain = test_chain(
            "chain:0",
            vec![
                test_input_block("dev-in", vec![0]),
                test_output_block("dev-out", vec![0, 1, 2]),
            ],
        );
        let project = test_project(vec![chain]);
        let err = validate_project(&project).unwrap_err();
        assert!(err.to_string().contains("3 channels"));
    }

    // -----------------------------------------------------------------------
    // validate_project — disabled chain skips block validation
    // -----------------------------------------------------------------------

    #[test]
    fn validate_project_disabled_chain_skips_block_validation() {
        // A chain with a Core block referencing a non-existent model should be
        // fine if the chain is disabled (validate_chain_blocks returns early).
        let bad_core = AudioBlock {
            id: BlockId("block:core".to_string()),
            enabled: true,
            kind: AudioBlockKind::Core(CoreBlock {
                effect_type: "delay".to_string(),
                model: "nonexistent_model".to_string(),
                params: ParameterSet::default(),
            }),
        };
        let mut chain = test_chain(
            "chain:0",
            vec![
                test_input_block("dev-in", vec![0]),
                bad_core,
                test_output_block("dev-out", vec![0, 1]),
            ],
        );
        chain.enabled = false;
        let project = test_project(vec![chain]);
        assert!(validate_project(&project).is_ok());
    }

    // -----------------------------------------------------------------------
    // validate_project — disabled block skips layout propagation
    // -----------------------------------------------------------------------

    #[test]
    fn validate_project_disabled_block_skipped_in_layout_propagation() {
        let disabled_core = AudioBlock {
            id: BlockId("block:core".to_string()),
            enabled: false,
            kind: AudioBlockKind::Core(CoreBlock {
                effect_type: "delay".to_string(),
                model: "nonexistent_model".to_string(),
                params: ParameterSet::default(),
            }),
        };
        let chain = test_chain(
            "chain:0",
            vec![
                test_input_block("dev-in", vec![0]),
                disabled_core,
                test_output_block("dev-out", vec![0, 1]),
            ],
        );
        let project = test_project(vec![chain]);
        assert!(validate_project(&project).is_ok());
    }

    // -----------------------------------------------------------------------
    // validate_project — layout propagation with real block types
    // -----------------------------------------------------------------------

    #[test]
    fn validate_project_with_delay_block_succeeds() {
        let delay_model = block_delay::supported_models()
            .first()
            .expect("block-delay must expose at least one model");
        let schema = project::block::schema_for_block_model("delay", delay_model)
            .expect("delay schema should exist");
        let params = ParameterSet::default()
            .normalized_against(&schema)
            .expect("delay defaults should normalize");

        let delay_block = AudioBlock {
            id: BlockId("block:delay".to_string()),
            enabled: true,
            kind: AudioBlockKind::Core(CoreBlock {
                effect_type: "delay".to_string(),
                model: delay_model.to_string(),
                params,
            }),
        };
        let chain = test_chain(
            "chain:0",
            vec![
                test_input_block("dev-in", vec![0]),
                delay_block,
                test_output_block("dev-out", vec![0, 1]),
            ],
        );
        let project = test_project(vec![chain]);
        assert!(validate_project(&project).is_ok());
    }

    #[test]
    fn validate_project_with_reverb_block_succeeds() {
        let reverb_model = block_reverb::supported_models()
            .first()
            .expect("block-reverb must expose at least one model");
        let schema = project::block::schema_for_block_model("reverb", reverb_model)
            .expect("reverb schema should exist");
        let params = ParameterSet::default()
            .normalized_against(&schema)
            .expect("reverb defaults should normalize");

        let reverb_block = AudioBlock {
            id: BlockId("block:reverb".to_string()),
            enabled: true,
            kind: AudioBlockKind::Core(CoreBlock {
                effect_type: "reverb".to_string(),
                model: reverb_model.to_string(),
                params,
            }),
        };
        let chain = test_chain(
            "chain:0",
            vec![
                test_input_block("dev-in", vec![0]),
                reverb_block,
                test_output_block("dev-out", vec![0, 1]),
            ],
        );
        let project = test_project(vec![chain]);
        assert!(validate_project(&project).is_ok());
    }

    // -----------------------------------------------------------------------
    // validate_project — Insert blocks are skipped in layout propagation
    // -----------------------------------------------------------------------

    #[test]
    fn validate_project_with_insert_block_succeeds() {
        let insert = AudioBlock {
            id: BlockId("block:insert".to_string()),
            enabled: true,
            kind: AudioBlockKind::Insert(InsertBlock {
                model: "external_loop".to_string(),
                send: InsertEndpoint {
                    device_id: DeviceId("send-dev".to_string()),
                    mode: ChainInputMode::Stereo,
                    channels: vec![0, 1],
                },
                return_: InsertEndpoint {
                    device_id: DeviceId("return-dev".to_string()),
                    mode: ChainInputMode::Stereo,
                    channels: vec![0, 1],
                },
            }),
        };
        let chain = test_chain(
            "chain:0",
            vec![
                test_input_block("dev-in", vec![0]),
                insert,
                test_output_block("dev-out", vec![0, 1]),
            ],
        );
        let project = test_project(vec![chain]);
        assert!(validate_project(&project).is_ok());
    }

    // -----------------------------------------------------------------------
    // layout_from_channel_count — unit tests
    // -----------------------------------------------------------------------

    #[test]
    fn layout_from_channel_count_mono_returns_mono() {
        let layout = layout_from_channel_count("test", "id", 1).unwrap();
        assert_eq!(layout, AudioChannelLayout::Mono);
    }

    #[test]
    fn layout_from_channel_count_stereo_returns_stereo() {
        let layout = layout_from_channel_count("test", "id", 2).unwrap();
        assert_eq!(layout, AudioChannelLayout::Stereo);
    }

    #[test]
    fn layout_from_channel_count_zero_channels_fails() {
        let err = layout_from_channel_count("test", "id", 0).unwrap_err();
        assert!(err.to_string().contains("0 channels"));
    }

    #[test]
    fn layout_from_channel_count_four_channels_fails() {
        let err = layout_from_channel_count("test", "id", 4).unwrap_err();
        assert!(err.to_string().contains("4 channels"));
    }

    // -----------------------------------------------------------------------
    // validate_unique_channels — unit tests
    // -----------------------------------------------------------------------

    #[test]
    fn validate_unique_channels_no_duplicates_succeeds() {
        assert!(validate_unique_channels(&[0, 1, 2]).is_ok());
    }

    #[test]
    fn validate_unique_channels_empty_succeeds() {
        assert!(validate_unique_channels(&[]).is_ok());
    }

    #[test]
    fn validate_unique_channels_single_succeeds() {
        assert!(validate_unique_channels(&[5]).is_ok());
    }

    #[test]
    fn validate_unique_channels_duplicate_fails() {
        let err = validate_unique_channels(&[0, 1, 0]).unwrap_err();
        assert!(err.to_string().contains("duplicated channel '0'"));
    }

    // -----------------------------------------------------------------------
    // layout_label — unit tests
    // -----------------------------------------------------------------------

    #[test]
    fn layout_label_mono_returns_mono() {
        assert_eq!(layout_label(AudioChannelLayout::Mono), "mono");
    }

    #[test]
    fn layout_label_stereo_returns_stereo() {
        assert_eq!(layout_label(AudioChannelLayout::Stereo), "stereo");
    }

    // -----------------------------------------------------------------------
    // validate_active_chain_input_channel_conflicts — unit tests
    // -----------------------------------------------------------------------

    #[test]
    fn channel_conflicts_no_chains_succeeds() {
        assert!(validate_active_chain_input_channel_conflicts(&[]).is_ok());
    }

    #[test]
    fn channel_conflicts_single_chain_succeeds() {
        let chain = valid_chain("chain:0");
        assert!(validate_active_chain_input_channel_conflicts(&[chain]).is_ok());
    }

    #[test]
    fn channel_conflicts_different_devices_succeeds() {
        let chain0 = test_chain(
            "chain:0",
            vec![
                test_input_block("dev-a", vec![0]),
                test_output_block("dev-out", vec![0, 1]),
            ],
        );
        let chain1 = test_chain(
            "chain:1",
            vec![
                test_input_block("dev-b", vec![0]),
                test_output_block("dev-out", vec![0, 1]),
            ],
        );
        assert!(validate_active_chain_input_channel_conflicts(&[chain0, chain1]).is_ok());
    }

    #[test]
    fn channel_conflicts_same_device_same_channel_fails() {
        let chain0 = test_chain(
            "chain:0",
            vec![
                test_input_block("dev-in", vec![0]),
                test_output_block("dev-out", vec![0, 1]),
            ],
        );
        let chain1 = test_chain(
            "chain:1",
            vec![
                test_input_block("dev-in", vec![0]),
                test_output_block("dev-out", vec![0, 1]),
            ],
        );
        let err = validate_active_chain_input_channel_conflicts(&[chain0, chain1]).unwrap_err();
        assert!(err.to_string().contains("both use input device"));
    }

    #[test]
    fn channel_conflicts_disabled_chains_ignored() {
        let chain0 = valid_chain("chain:0");
        let mut chain1 = test_chain(
            "chain:1",
            vec![
                test_input_block("dev-in", vec![0]),
                test_output_block("dev-out", vec![0, 1]),
            ],
        );
        chain1.enabled = false;
        assert!(validate_active_chain_input_channel_conflicts(&[chain0, chain1]).is_ok());
    }

    // -----------------------------------------------------------------------
    // validate_device_settings — unit tests
    // -----------------------------------------------------------------------

    #[test]
    fn validate_device_settings_valid_succeeds() {
        let project = valid_project();
        let map: HashMap<_, _> = project
            .device_settings
            .iter()
            .map(|s| (s.device_id.0.clone(), s))
            .collect();
        assert!(validate_device_settings(&project, &map).is_ok());
    }

    #[test]
    fn validate_device_settings_empty_device_id_fails() {
        let settings = vec![DeviceSettings {
            device_id: DeviceId("  ".to_string()),
            sample_rate: 48000,
            buffer_size_frames: 256,
        }];
        let project = Project {
            name: Some("test".to_string()),
            device_settings: settings,
            chains: vec![valid_chain("chain:0")],
        };
        let map: HashMap<_, _> = project
            .device_settings
            .iter()
            .map(|s| (s.device_id.0.clone(), s))
            .collect();
        let err = validate_device_settings(&project, &map).unwrap_err();
        assert!(err.to_string().contains("missing device_id"));
    }

    #[test]
    fn validate_device_settings_duplicate_device_id_fails() {
        let settings = vec![
            test_device_settings("dev-a"),
            test_device_settings("dev-a"),
        ];
        let project = Project {
            name: Some("test".to_string()),
            device_settings: settings.clone(),
            chains: vec![valid_chain("chain:0")],
        };
        // HashMap will deduplicate, so len != original len
        let map: HashMap<_, _> = project
            .device_settings
            .iter()
            .map(|s| (s.device_id.0.clone(), s))
            .collect();
        let err = validate_device_settings(&project, &map).unwrap_err();
        assert!(err.to_string().contains("duplicated device_settings"));
    }
}
