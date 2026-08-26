//! Responsibility: expands the resolved endpoints into the streams the runtime opens.

use std::collections::HashMap;

use domain::ids::DeviceId;
use domain::io_binding::IoBinding;
use project::block::AudioBlockKind;
use project::chain::{Chain, ChainInputMode, ChainOutputMode};

use crate::endpoint_entry::{InputEntry, OutputEntry};
use crate::insert_endpoints::{
    insert_is_bound, insert_return_as_input_entry, insert_send_as_output_entry,
};

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
    registry: &[IoBinding],
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
        .filter(|b| b.enabled && insert_is_bound(&b.kind, registry))
        .filter_map(|b| match &b.kind {
            AudioBlockKind::Insert(ib) => insert_return_as_input_entry(ib, registry),
            _ => None,
        })
        .collect();
    for (i, ret) in insert_returns.into_iter().enumerate() {
        // The cpal index is the DEVICE's, from the same first-seen map the
        // regular inputs use (#881). infra-cpal opens one input stream per
        // device and binds every runtime fed by it (#703), so a return that
        // comes back on the interface the guitar already uses rides that
        // stream and picks its own channel. Giving it a private index named a
        // stream nobody opens — the post-insert segment was never fed and the
        // rig went silent.
        let device_key = ret.device_id.0.clone();
        let cpal_idx = *device_to_cpal.entry(device_key).or_insert_with(|| {
            let idx = next_cpal_idx;
            next_cpal_idx += 1;
            idx
        });
        cpal_indices.push(cpal_idx);
        split_positions.push(None);
        // Its own runtime, always: a return is never summed with the entry it
        // shares the device with (invariant #4).
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
pub(crate) fn effective_outputs(
    chain: &Chain,
    resolved: &[OutputEntry],
    registry: &[IoBinding],
) -> Vec<OutputEntry> {
    let mut entries: Vec<OutputEntry> = resolved.to_vec();

    // Append Insert send entries (as outputs for segments before each Insert).
    let insert_sends: Vec<OutputEntry> = chain
        .blocks
        .iter()
        .filter(|b| b.enabled && insert_is_bound(&b.kind, registry))
        .filter_map(|b| match &b.kind {
            AudioBlockKind::Insert(ib) => insert_send_as_output_entry(ib, registry),
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
