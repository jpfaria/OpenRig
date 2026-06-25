//! Test helpers + re-exported types for `application::validate` tests.
//!
//! Submodule of `validate::tests` — siblings (`main`, `unit`) bring everything
//! in via `use super::helpers::*`.
//!
//! #716 (model A): Input/Output blocks no longer embed device endpoints
//! (`entries` are gone). Device data is resolved from the per-machine I/O
//! binding registry, which `validate_project` does not see — so the helpers
//! build unbound I/O blocks (`io`/`endpoint` empty). The `device_id`/`channels`
//! arguments are retained for call-site compatibility but are no longer stored
//! on the block; they only drive the synthesized `device_settings`.

pub(super) use super::super::*;
pub(super) use domain::ids::{BlockId, ChainId, DeviceId};
pub(super) use project::block::{
    AudioBlock, AudioBlockKind, CoreBlock, InputBlock, InsertBlock, InsertEndpoint, OutputBlock,
};
pub(super) use project::chain::{Chain, ChainInputMode};
pub(super) use project::device::DeviceSettings;
pub(super) use project::param::ParameterSet;
pub(super) use project::project::Project;

pub(super) fn test_input_block(_device_id: &str, _channels: Vec<usize>) -> AudioBlock {
    AudioBlock {
        id: BlockId("block:input".to_string()),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".to_string(),
            io: String::new(),
            endpoint: String::new(),
        }),
    }
}

pub(super) fn test_output_block(_device_id: &str, _channels: Vec<usize>) -> AudioBlock {
    AudioBlock {
        id: BlockId("block:output".to_string()),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".to_string(),
            io: String::new(),
            endpoint: String::new(),
        }),
    }
}

pub(super) fn test_chain(id: &str, blocks: Vec<AudioBlock>) -> Chain {
    Chain {
        id: ChainId(id.to_string()),
        description: None,
        instrument: "electric_guitar".to_string(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec![],
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
    Project {
        name: Some("test".to_string()),
        device_settings: vec![test_device_settings("dev-test")],
        chains,
        midi: None,
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
