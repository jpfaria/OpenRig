//! Pure helpers that turn the in-memory `ChainDraft` I/O groups into the
//! corresponding `AudioBlock` representation persisted on the chain.
//!
//! Each helper produces AT MOST one block carrying every device as a separate
//! `entry`. Building one block per device is wrong — it makes the canvas
//! render extra IN/OUT icons in the middle of the chain (issue #377).

use domain::ids::{BlockId, ChainId, DeviceId};
use project::block::{
    AudioBlock, AudioBlockKind, InputBlock, InputEntry, OutputBlock, OutputEntry,
};

use crate::state::{InputGroupDraft, OutputGroupDraft};

const STANDARD_IO_MODEL: &str = "standard";

pub(crate) fn build_input_block_from_draft(
    chain_id: &ChainId,
    drafts: &[InputGroupDraft],
) -> Option<AudioBlock> {
    if drafts.is_empty() {
        return None;
    }
    let entries: Vec<InputEntry> = drafts
        .iter()
        .map(|ig| InputEntry {
            device_id: DeviceId(ig.device_id.clone().unwrap_or_default()),
            mode: ig.mode,
            channels: ig.channels.clone(),
        })
        .collect();
    Some(AudioBlock {
        id: BlockId(format!("{}:input", chain_id.0)),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: STANDARD_IO_MODEL.to_string(),
            entries,
        }),
    })
}

pub(crate) fn build_output_block_from_draft(
    chain_id: &ChainId,
    drafts: &[OutputGroupDraft],
) -> Option<AudioBlock> {
    if drafts.is_empty() {
        return None;
    }
    let entries: Vec<OutputEntry> = drafts
        .iter()
        .map(|og| OutputEntry {
            device_id: DeviceId(og.device_id.clone().unwrap_or_default()),
            mode: og.mode,
            channels: og.channels.clone(),
        })
        .collect();
    Some(AudioBlock {
        id: BlockId(format!("{}:output", chain_id.0)),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: STANDARD_IO_MODEL.to_string(),
            entries,
        }),
    })
}

#[cfg(test)]
#[path = "chain_io_block_builders_tests.rs"]
mod tests;
