//! Effective endpoint resolution for a chain.
//!
//! Model A (#716): a chain no longer embeds device endpoints. The device /
//! channels / mode of every input and output come from the per-machine I/O
//! binding registry, resolved via [`project::binding_discovery::resolve_chain_ports`].
//! This module owns the engine's *runtime* endpoint types ([`InputEntry`] /
//! [`OutputEntry`]) — they are NOT persisted; they are the resolved view the
//! runtime builds streams from. The pinned volume/golden math operates on
//! these exactly as before; only the SOURCE changed (binding, not the chain).
//!
//! What lives here:
//!   - `InputEntry` / `OutputEntry` — the engine's resolved device endpoints.
//!   - `resolve_chain_io` — chain + registry → `(Vec<InputEntry>, Vec<OutputEntry>)`.
//!   - `effective_inputs` — split mono entries with N>1 channels into N
//!     streams; append Insert-return shims.
//!   - `effective_outputs` — flatten resolved outputs; append Insert-send shims.

use std::collections::HashMap;

use domain::ids::DeviceId;
use domain::io_binding::IoBinding;
use project::binding_discovery::{resolve_chain_ports, PortDirection};
use project::block::{AudioBlockKind, InsertBlock};
use project::chain::{Chain, ChainInputMode, ChainOutputMode};

/// A resolved input endpoint the runtime reads from. Not persisted — built
/// from the chain's selected I/O binding(s). `mode`/`channels`/`device_id`
/// come from the binding's `IoEndpoint`.
#[derive(Debug, Clone, PartialEq)]
pub struct InputEntry {
    pub device_id: DeviceId,
    pub mode: ChainInputMode,
    pub channels: Vec<usize>,
}

/// A resolved output endpoint the runtime writes to. Not persisted.
#[derive(Debug, Clone, PartialEq)]
pub struct OutputEntry {
    pub device_id: DeviceId,
    pub mode: ChainOutputMode,
    pub channels: Vec<usize>,
}

/// Resolve a chain's input and output device endpoints from the binding
/// `registry`. Head/tail come from `chain.io_binding_ids`; mid `Input`/`Output`
/// blocks resolve their `io`/`endpoint`. The device data lives only in the
/// registry — never in the chain (#716, model A).
pub fn resolve_chain_io(chain: &Chain, registry: &[IoBinding]) -> (Vec<InputEntry>, Vec<OutputEntry>) {
    let ports = resolve_chain_ports(chain, registry);
    let mut inputs = Vec::new();
    let mut outputs = Vec::new();
    for port in ports {
        match port.direction {
            PortDirection::Input => inputs.push(InputEntry {
                device_id: port.endpoint.device_id,
                mode: ChainInputMode::from(port.endpoint.mode),
                channels: port.endpoint.channels,
            }),
            PortDirection::Output => outputs.push(OutputEntry {
                device_id: port.endpoint.device_id,
                mode: ChainOutputMode::try_from(port.endpoint.mode)
                    .unwrap_or(ChainOutputMode::Stereo),
                channels: port.endpoint.channels,
            }),
        }
    }
    (inputs, outputs)
}

/// Expand the resolved input endpoints into the flat per-stream list.
///
/// Returns `(entries, cpal_indices, split_positions, entry_groups)` — see the
/// per-field docs below. `resolved` are the chain's input endpoints (from the
/// binding registry); Insert-return shims are appended from the chain's enabled
/// Insert blocks. The split-mono / cpal-index / group math is byte-identical to
/// the legacy entries-based path (pinned volume invariants depend on it).
///
/// - `entries[i]` — the `i`-th effective input, one per processing stream.
/// - `cpal_indices[i]` — the CPAL stream index (inputs sharing a device share
///   the index; infra-cpal dedupes by device).
/// - `split_positions[i]` — `Some(N)` when this entry came from a split-mono
///   original (one mono endpoint with N channels) owning one of N positions;
///   the runtime scales its fan-out contribution by `1/N`. `None` otherwise.
/// - `entry_groups[i]` — the RAW input index this entry came from (#703):
///   split-mono siblings share a group (sum before the per-runtime limiter,
///   g02/g03); distinct raw endpoints get distinct groups (own isolated
///   runtime) even on the same device.
pub(crate) fn effective_inputs(
    chain: &Chain,
    resolved: &[InputEntry],
) -> (Vec<InputEntry>, Vec<usize>, Vec<Option<usize>>, Vec<usize>) {
    let raw_entries: Vec<InputEntry> = resolved.to_vec();

    let mut entries: Vec<InputEntry> = Vec::new();
    let mut cpal_indices: Vec<usize> = Vec::new();
    let mut split_positions: Vec<Option<usize>> = Vec::new();
    let mut entry_groups: Vec<usize> = Vec::new();
    let mut device_to_cpal: HashMap<String, usize> = HashMap::new();
    let mut next_cpal_idx: usize = 0;

    for (raw_idx, entry) in raw_entries.iter().enumerate() {
        let device_key = entry.device_id.0.clone();
        let cpal_idx = *device_to_cpal.entry(device_key).or_insert_with(|| {
            let idx = next_cpal_idx;
            next_cpal_idx += 1;
            idx
        });

        if matches!(entry.mode, ChainInputMode::Mono) && entry.channels.len() > 1 {
            let n = entry.channels.len();
            for &ch in entry.channels.iter() {
                entries.push(InputEntry {
                    device_id: entry.device_id.clone(),
                    mode: ChainInputMode::Mono,
                    channels: vec![ch],
                });
                cpal_indices.push(cpal_idx);
                split_positions.push(Some(n));
                entry_groups.push(raw_idx);
            }
        } else {
            entries.push(entry.clone());
            cpal_indices.push(cpal_idx);
            split_positions.push(None);
            entry_groups.push(raw_idx);
        }
    }

    // Append Insert return entries (as inputs for segments after each Insert).
    let insert_return_base = raw_entries.len();
    let insert_returns: Vec<InputEntry> = chain
        .blocks
        .iter()
        .filter(|b| b.enabled)
        .filter_map(|b| match &b.kind {
            AudioBlockKind::Insert(ib) => Some(insert_return_as_input_entry(ib)),
            _ => None,
        })
        .collect();
    for (i, ret) in insert_returns.into_iter().enumerate() {
        cpal_indices.push(insert_return_base + i);
        split_positions.push(None);
        entry_groups.push(insert_return_base + i);
        entries.push(ret);
    }

    if !entries.is_empty() {
        return (entries, cpal_indices, split_positions, entry_groups);
    }
    // Fallback — chain has no resolved inputs.
    (
        vec![InputEntry {
            device_id: DeviceId("".to_string()),
            mode: ChainInputMode::Mono,
            channels: vec![0],
        }],
        vec![0],
        vec![None],
        vec![0],
    )
}

/// Build effective output entries from the resolved outputs, plus Insert send
/// entries. Order: resolved outputs first, then Insert sends (matches CPAL
/// stream order). Falls back to a single mono output on channel 0 if neither.
pub(crate) fn effective_outputs(chain: &Chain, resolved: &[OutputEntry]) -> Vec<OutputEntry> {
    let mut entries: Vec<OutputEntry> = resolved.to_vec();

    // Append Insert send entries (as outputs for segments before each Insert).
    let insert_sends: Vec<OutputEntry> = chain
        .blocks
        .iter()
        .filter(|b| b.enabled)
        .filter_map(|b| match &b.kind {
            AudioBlockKind::Insert(ib) => Some(insert_send_as_output_entry(ib)),
            _ => None,
        })
        .collect();
    entries.extend(insert_sends);

    if !entries.is_empty() {
        return entries;
    }
    // Fallback — no resolved outputs and no Inserts.
    vec![OutputEntry {
        device_id: DeviceId("".to_string()),
        mode: ChainOutputMode::Mono,
        channels: vec![0],
    }]
}

/// Convert an `InsertBlock`'s return endpoint to an `InputEntry`.
pub(crate) fn insert_return_as_input_entry(insert: &InsertBlock) -> InputEntry {
    InputEntry {
        device_id: insert.return_.device_id.clone(),
        mode: insert.return_.mode,
        channels: insert.return_.channels.clone(),
    }
}

/// Convert an `InsertBlock`'s send endpoint to an `OutputEntry`.
pub(crate) fn insert_send_as_output_entry(insert: &InsertBlock) -> OutputEntry {
    OutputEntry {
        device_id: insert.send.device_id.clone(),
        mode: match insert.send.mode {
            ChainInputMode::Mono => ChainOutputMode::Mono,
            _ => ChainOutputMode::Stereo,
        },
        channels: insert.send.channels.clone(),
    }
}
