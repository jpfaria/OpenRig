//! Test helpers + re-exported types for `application::validate` tests.
//!
//! Submodule of `validate::tests` — siblings (`main`, `unit`) bring everything
//! in via `use super::helpers::*`.

pub(super) use super::super::*;
pub(super) use domain::ids::{BlockId, ChainId, DeviceId};
pub(super) use project::block::{
    AudioBlock, AudioBlockKind, CoreBlock, InputBlock, InputEntry, InsertBlock, InsertEndpoint,
    OutputBlock, OutputEntry,
};
pub(super) use project::chain::{Chain, ChainInputMode, ChainOutputMode};
pub(super) use project::device::DeviceSettings;
pub(super) use project::param::ParameterSet;
pub(super) use project::project::Project;

pub(super) fn test_input_entry(_name: &str, device_id: &str, channels: Vec<usize>) -> InputEntry {
    InputEntry {
        device_id: DeviceId(device_id.to_string()),
        mode: ChainInputMode::Mono,
        channels,
    }
}

pub(super) fn test_input_block(device_id: &str, channels: Vec<usize>) -> AudioBlock {
    AudioBlock {
        id: BlockId("block:input".to_string()),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".to_string(),
            entries: vec![test_input_entry("Input 1", device_id, channels)],
        }),
    }
}

pub(super) fn test_output_entry(_name: &str, device_id: &str, channels: Vec<usize>) -> OutputEntry {
    OutputEntry {
        device_id: DeviceId(device_id.to_string()),
        mode: ChainOutputMode::Stereo,
        channels,
    }
}

pub(super) fn test_output_block(device_id: &str, channels: Vec<usize>) -> AudioBlock {
    AudioBlock {
        id: BlockId("block:output".to_string()),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".to_string(),
            entries: vec![test_output_entry("Output 1", device_id, channels)],
        }),
    }
}

pub(super) fn test_chain(id: &str, blocks: Vec<AudioBlock>) -> Chain {
    Chain {
        id: ChainId(id.to_string()),
        description: None,
        instrument: "electric_guitar".to_string(),
        enabled: true,
        blocks,
    }
}

pub(super) fn test_device_settings(device_id: &str) -> DeviceSettings {
    DeviceSettings {
        device_id: DeviceId(device_id.to_string()),
        sample_rate: 48000,
        buffer_size_frames: 256,
        bit_depth: 32,
        #[cfg(target_os = "linux")]
        realtime: true,
        #[cfg(target_os = "linux")]
        rt_priority: 70,
        #[cfg(target_os = "linux")]
        nperiods: 3,
    }
}

pub(super) fn test_project(chains: Vec<Chain>) -> Project {
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
        device_settings: device_ids
            .iter()
            .map(|id| test_device_settings(id))
            .collect(),
        chains,
    }
}

pub(super) fn valid_chain(id: &str) -> Chain {
    test_chain(
        id,
        vec![
            test_input_block("dev-in", vec![0]),
            test_output_block("dev-out", vec![0, 1]),
        ],
    )
}

pub(super) fn valid_project() -> Project {
    test_project(vec![valid_chain("chain:0")])
}
