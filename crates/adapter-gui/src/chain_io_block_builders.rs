//! Pure helpers that turn the in-memory `ChainDraft` I/O groups into the
//! corresponding `AudioBlock` representation persisted on the chain, and that
//! build the reshaped Task-7 commands when saving I/O binding references.
//!
//! Each draft-to-block helper produces AT MOST one block carrying every device
//! as a separate `entry`. Building one block per device is wrong — it makes
//! the canvas render extra IN/OUT icons in the middle of the chain (#377).

use application::command::Command;
use domain::ids::{BlockId, ChainId, DeviceId};
use project::block::{
    AudioBlock, AudioBlockKind, InputBlock, InputEntry, OutputBlock, OutputEntry,
};

use crate::state::{InputGroupDraft, OutputGroupDraft};

// ── Binding-reference port types (test-only until the picker UI is wired) ─────

/// A single input port described as a binding + endpoint reference.
/// Replaces raw device-id when the I/O binding layer is the source of truth.
#[cfg(test)]
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct InputPortRef {
    pub binding_id: String,
    pub endpoint: String,
}

/// A group of `InputPortRef` values that all share the same binding.
/// Produced by [`group_input_ports_by_binding`].
#[cfg(test)]
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct InputPortGroup {
    pub binding_id: String,
    pub ports: Vec<InputPortRef>,
}

/// Groups a flat list of [`InputPortRef`] values by their `binding_id`,
/// preserving insertion order for both groups and ports within each group.
///
/// This is a pure projector: no side-effects, no `AppWindow`, testable alone.
#[cfg(test)]
pub(crate) fn group_input_ports_by_binding(ports: &[InputPortRef]) -> Vec<InputPortGroup> {
    let mut groups: Vec<InputPortGroup> = Vec::new();
    for port in ports {
        if let Some(group) = groups.iter_mut().find(|g| g.binding_id == port.binding_id) {
            group.ports.push(port.clone());
        } else {
            groups.push(InputPortGroup {
                binding_id: port.binding_id.clone(),
                ports: vec![port.clone()],
            });
        }
    }
    groups
}

// ── Reshaped Task-7 command builders ─────────────────────────────────────────

/// Returns `Command::SaveChainInputEndpoints` from a binding-reference
/// `(io, endpoint)` selection. This is the authoritative save path for the
/// fullscreen I/O editor after #716 — never `SaveChain` as a stopgap.
pub(crate) fn build_input_endpoint_cmd(
    chain: ChainId,
    block_index: usize,
    io: &str,
    endpoint: &str,
) -> Command {
    Command::SaveChainInputEndpoints {
        chain,
        block_index,
        io: io.to_string(),
        endpoint: endpoint.to_string(),
    }
}

/// Returns `Command::SaveChainOutputEndpoints` from a binding-reference
/// `(io, endpoint)` selection. Mirror of [`build_input_endpoint_cmd`].
pub(crate) fn build_output_endpoint_cmd(
    chain: ChainId,
    block_index: usize,
    io: &str,
    endpoint: &str,
) -> Command {
    Command::SaveChainOutputEndpoints {
        chain,
        block_index,
        io: io.to_string(),
        endpoint: endpoint.to_string(),
    }
}

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
            io: String::new(),
            endpoint: String::new(),
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
            io: String::new(),
            endpoint: String::new(),
        }),
    })
}

#[cfg(test)]
#[path = "chain_io_block_builders_tests.rs"]
mod tests;
