use infra_cpal::AudioDeviceDescriptor;
use project::block::{AudioBlockKind, InputEntry, OutputEntry};
use project::chain::{Chain, ChainInputMode, ChainOutputMode};
use project::project::Project;
use domain::ids::DeviceId;
use crate::AppWindow;
use crate::state::{ChainDraft, ChainEditorMode, InputGroupDraft, OutputGroupDraft};

const INSTRUMENT_KEYS: &[&str] = &[
    block_core::INST_ELECTRIC_GUITAR,
    block_core::INST_ACOUSTIC_GUITAR,
    block_core::INST_BASS,
    block_core::INST_VOICE,
    block_core::INST_KEYS,
    block_core::INST_DRUMS,
    block_core::INST_GENERIC,
];

pub(crate) fn create_chain_draft(
    project: &Project,
    _input_devices: &[AudioDeviceDescriptor],
    _output_devices: &[AudioDeviceDescriptor],
) -> ChainDraft {
    // Empty I/O fields force the user to consciously pick the device + channels
    // for the chain instead of accepting whichever device happens to be first.
    let default_input = InputGroupDraft {
        device_id: None,
        channels: Vec::new(),
        mode: ChainInputMode::Mono,
    };
    let default_output = OutputGroupDraft {
        device_id: None,
        channels: Vec::new(),
        mode: ChainOutputMode::Stereo,
    };
    ChainDraft {
        editing_index: None,
        name: format!("Chain {}", project.chains.len() + 1),
        instrument: block_core::DEFAULT_INSTRUMENT.to_string(),
        inputs: vec![default_input],
        outputs: vec![default_output],
        editing_io_block_index: None,
        editing_input_index: None,
        editing_output_index: None,
        adding_new_input: false,
        adding_new_output: false,
    }
}

pub(crate) fn chain_draft_from_chain(index: usize, chain: &Chain) -> ChainDraft {
    // Only show the first InputBlock (fixed, position 0) in the chain editor
    let first_input = chain.input_blocks().into_iter().next();
    let inputs: Vec<InputGroupDraft> = match first_input {
        Some((_, input)) => input
            .entries
            .iter()
            .map(|entry| InputGroupDraft {
                device_id: if entry.device_id.0.is_empty() { None } else { Some(entry.device_id.0.clone()) },
                channels: entry.channels.clone(),
                mode: entry.mode,
            })
            .collect(),
        None => vec![InputGroupDraft {
            device_id: None,
            channels: Vec::new(),
            mode: ChainInputMode::Mono,
        }],
    };
    // Only show the last OutputBlock (fixed, last position) in the chain editor
    let last_output = chain.output_blocks().into_iter().last();
    let outputs: Vec<OutputGroupDraft> = match last_output {
        Some((_, output)) => output
            .entries
            .iter()
            .map(|entry| OutputGroupDraft {
                device_id: if entry.device_id.0.is_empty() { None } else { Some(entry.device_id.0.clone()) },
                channels: entry.channels.clone(),
                mode: entry.mode,
            })
            .collect(),
        None => vec![OutputGroupDraft {
            device_id: None,
            channels: Vec::new(),
            mode: ChainOutputMode::Stereo,
        }],
    };
    ChainDraft {
        editing_index: Some(index),
        name: chain
            .description
            .clone()
            .unwrap_or_else(|| format!("Chain {}", index + 1)),
        instrument: chain.instrument.clone(),
        inputs,
        editing_io_block_index: None,
        outputs,
        editing_input_index: None,
        editing_output_index: None,
        adding_new_input: false,
        adding_new_output: false,
    }
}

pub(crate) fn chain_from_draft(draft: &ChainDraft, existing_chain: Option<&Chain>) -> Chain {
    if let Some(existing) = existing_chain {
        // Edit mode: update name, instrument, and I/O device entries from draft;
        // preserve all other blocks (effects, DSP) as-is.
        let mut blocks = existing.blocks.clone();
        // Update first InputBlock entries from draft
        if let Some(pos) = blocks.iter().position(|b| matches!(b.kind, AudioBlockKind::Input(_))) {
            let new_entries: Vec<InputEntry> = draft.inputs.iter()
                .filter(|ig| ig.device_id.is_some() && !ig.channels.is_empty())
                .map(|ig| InputEntry {
                    device_id: DeviceId(ig.device_id.clone().unwrap_or_default()),
                    mode: ig.mode,
                    channels: ig.channels.clone(),
                })
                .collect();
            if !new_entries.is_empty() {
                if let AudioBlockKind::Input(ref mut ib) = blocks[pos].kind {
                    ib.entries = new_entries;
                }
            }
        }
        // Update last OutputBlock entries from draft
        let last_output_pos = blocks.iter().enumerate().rev()
            .find(|(_, b)| matches!(b.kind, AudioBlockKind::Output(_)))
            .map(|(i, _)| i);
        if let Some(pos) = last_output_pos {
            let new_entries: Vec<OutputEntry> = draft.outputs.iter()
                .filter(|og| og.device_id.is_some() && !og.channels.is_empty())
                .map(|og| OutputEntry {
                    device_id: DeviceId(og.device_id.clone().unwrap_or_default()),
                    mode: og.mode,
                    channels: og.channels.clone(),
                })
                .collect();
            if !new_entries.is_empty() {
                if let AudioBlockKind::Output(ref mut ob) = blocks[pos].kind {
                    ob.entries = new_entries;
                }
            }
        }
        Chain {
            id: existing.id.clone(),
            description: normalized_chain_description(&draft.name),
            instrument: draft.instrument.clone(),
            enabled: existing.enabled,
            blocks,
        }
    } else {
        // Create mode: build initial I/O blocks from draft
        let input_entries: Vec<InputEntry> = draft
            .inputs
            .iter()
            .filter(|ig| ig.device_id.is_some() && !ig.channels.is_empty())
            .map(|ig| InputEntry {
                device_id: DeviceId(ig.device_id.clone().unwrap_or_default()),
                mode: ig.mode,
                channels: ig.channels.clone(),
            })
            .collect();
        let output_entries: Vec<OutputEntry> = draft
            .outputs
            .iter()
            .filter(|og| og.device_id.is_some() && !og.channels.is_empty())
            .map(|og| OutputEntry {
                device_id: DeviceId(og.device_id.clone().unwrap_or_default()),
                mode: og.mode,
                channels: og.channels.clone(),
            })
            .collect();
        use domain::ids::{BlockId, ChainId};
        use project::block::{AudioBlock, InputBlock, OutputBlock};

        let mut blocks = Vec::new();
        if !input_entries.is_empty() {
            blocks.push(AudioBlock {
                id: BlockId("input:0".into()),
                enabled: true,
                kind: AudioBlockKind::Input(InputBlock {
                    model: "standard".to_string(),
                    entries: input_entries,
                }),
            });
        }
        if !output_entries.is_empty() {
            blocks.push(AudioBlock {
                id: BlockId("output:0".into()),
                enabled: true,
                kind: AudioBlockKind::Output(OutputBlock {
                    model: "standard".to_string(),
                    entries: output_entries,
                }),
            });
        }

        Chain {
            id: ChainId::generate(),
            description: normalized_chain_description(&draft.name),
            instrument: draft.instrument.clone(),
            enabled: false,
            blocks,
        }
    }
}

pub(crate) fn instrument_index_to_string(index: i32) -> &'static str {
    INSTRUMENT_KEYS
        .get(index as usize)
        .copied()
        .unwrap_or(block_core::DEFAULT_INSTRUMENT)
}

pub(crate) fn input_mode_to_index(mode: ChainInputMode) -> i32 {
    match mode {
        ChainInputMode::Mono => 0,
        ChainInputMode::Stereo => 1,
        ChainInputMode::DualMono => 2,
    }
}

pub(crate) fn input_mode_from_index(index: i32) -> ChainInputMode {
    match index {
        1 => ChainInputMode::Stereo,
        2 => ChainInputMode::DualMono,
        _ => ChainInputMode::Mono,
    }
}

pub(crate) fn output_mode_to_index(mode: ChainOutputMode) -> i32 {
    match mode {
        ChainOutputMode::Mono => 0,
        ChainOutputMode::Stereo => 1,
    }
}

pub(crate) fn output_mode_from_index(index: i32) -> ChainOutputMode {
    match index {
        1 => ChainOutputMode::Stereo,
        _ => ChainOutputMode::Mono,
    }
}

pub(crate) fn insert_mode_to_index(mode: ChainInputMode) -> i32 {
    match mode {
        ChainInputMode::Mono => 0,
        ChainInputMode::Stereo => 1,
        ChainInputMode::DualMono => 0,
    }
}

pub(crate) fn insert_mode_from_index(index: i32) -> ChainInputMode {
    match index {
        1 => ChainInputMode::Stereo,
        _ => ChainInputMode::Mono,
    }
}

pub(crate) fn instrument_string_to_index(instrument: &str) -> i32 {
    INSTRUMENT_KEYS
        .iter()
        .position(|&key| key == instrument)
        .map(|i| i as i32)
        .unwrap_or(0)
}

pub(crate) fn chain_editor_mode(draft: &ChainDraft) -> ChainEditorMode {
    if draft.editing_index.is_some() {
        ChainEditorMode::Edit
    } else {
        ChainEditorMode::Create
    }
}

pub(crate) fn apply_chain_editor_labels(window: &AppWindow, draft: &ChainDraft) {
    match chain_editor_mode(draft) {
        ChainEditorMode::Create => {
            window.set_chain_editor_title("Nova chain".into());
            window.set_chain_editor_save_label("Criar chain".into());
        }
        ChainEditorMode::Edit => {
            window.set_chain_editor_title("Configurar chain".into());
            window.set_chain_editor_save_label("Salvar chain".into());
        }
    }
}

pub(crate) fn endpoint_summary(
    device_id: Option<&str>,
    channels: &[usize],
    devices: &[AudioDeviceDescriptor],
) -> String {
    let device_name = device_id
        .and_then(|id| devices.iter().find(|device| device.id == id).map(|device| device.name.clone()))
        .or_else(|| device_id.map(|id| id.to_string()))
        .unwrap_or_else(|| "Nenhum dispositivo".to_string());
    let channels = if channels.is_empty() {
        "-".to_string()
    } else {
        channels
            .iter()
            .map(|channel| format!("{}", channel + 1))
            .collect::<Vec<_>>()
            .join(", ")
    };
    format!("{device_name} · Ch {channels}")
}

pub(crate) fn normalized_chain_description(name: &str) -> Option<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}
