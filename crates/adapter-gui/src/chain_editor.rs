use crate::state::{ChainDraft, ChainEditorMode};
use crate::AppWindow;
use infra_cpal::AudioDeviceDescriptor;
use project::chain::{Chain, ChainInputMode};
use project::project::Project;

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
    // #716: a fresh chain starts with no bindings selected. Its I/O is
    // discovered from the binding checklist; the save handler rejects an
    // empty selection with a toast.
    ChainDraft {
        editing_index: None,
        name: rust_i18n::t!("default-chain-name", n = project.chains.len() + 1).to_string(),
        instrument: block_core::DEFAULT_INSTRUMENT.to_string(),
        io_binding_ids: Vec::new(),
    }
}

pub(crate) fn chain_draft_from_chain(index: usize, chain: &Chain) -> ChainDraft {
    ChainDraft {
        editing_index: Some(index),
        name: chain
            .description
            .clone()
            .unwrap_or_else(|| rust_i18n::t!("default-chain-name", n = index + 1).to_string()),
        instrument: chain.instrument.clone(),
        io_binding_ids: chain.io_binding_ids.clone(),
    }
}

pub(crate) fn chain_from_draft(draft: &ChainDraft, existing_chain: Option<&Chain>) -> Chain {
    if let Some(existing) = existing_chain {
        // Edit mode: update name, instrument, and the selected I/O bindings;
        // preserve all blocks (effects, DSP, I/O) as-is.
        Chain {
            id: existing.id.clone(),
            description: normalized_chain_description(&draft.name),
            instrument: draft.instrument.clone(),
            enabled: existing.enabled,
            volume: existing.volume,
            io_binding_ids: draft.io_binding_ids.clone(),
            blocks: existing.blocks.clone(),
        }
    } else {
        // Create mode: a new chain has no blocks. Its input/output is
        // resolved at runtime from the selected I/O bindings (#716).
        use domain::ids::ChainId;

        Chain {
            id: ChainId::generate(),
            description: normalized_chain_description(&draft.name),
            instrument: draft.instrument.clone(),
            enabled: false,
            volume: 100.0,
            io_binding_ids: draft.io_binding_ids.clone(),
            blocks: Vec::new(),
        }
    }
}

pub(crate) fn instrument_index_to_string(index: i32) -> &'static str {
    INSTRUMENT_KEYS
        .get(index as usize)
        .copied()
        .unwrap_or(block_core::DEFAULT_INSTRUMENT)
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
            window.set_chain_editor_title(rust_i18n::t!("title-new-chain").as_ref().into());
            window.set_chain_editor_save_label(rust_i18n::t!("btn-create-chain").as_ref().into());
        }
        ChainEditorMode::Edit => {
            window.set_chain_editor_title(rust_i18n::t!("title-configure-chain").as_ref().into());
            window.set_chain_editor_save_label(rust_i18n::t!("btn-save-chain").as_ref().into());
        }
    }
}

pub(crate) fn normalized_chain_description(name: &str) -> Option<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}
