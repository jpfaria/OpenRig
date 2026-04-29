//! Effective endpoint resolution for a chain — converts the user-facing
//! `InputBlock` / `OutputBlock` / `InsertBlock` entries into the flat lists
//! the runtime actually needs (one effective input per processing stream,
//! one effective output per route, plus Insert send/return shims).
//!
//! Lifted out of `runtime_graph.rs` (slice 7 of the Phase 2 split) so the
//! parent file gets back under the 600 LOC cap.
//!
//! What lives here:
//!   - `effective_inputs` — flattens enabled `InputBlock` entries, splits
//!     mono entries with N>1 channels into N separate streams, appends
//!     Insert-return shims so post-Insert segments have an "input".
//!   - `effective_outputs` — flattens enabled `OutputBlock` entries and
//!     appends Insert-send shims so pre-Insert segments have an "output".
//!   - `insert_return_as_input_entry` / `insert_send_as_output_entry` —
//!     the conversion helpers used by the two functions above.
//!
//! What's NOT here: the per-stream processing-state builder
//! (`build_input_processing_state`) and the per-route routing-state builder
//! (`build_output_routing_state`) — those construct runtime state from the
//! resolved endpoints and live with the rest of the graph assembly in
//! `runtime_graph.rs`.

use std::collections::HashMap;

use domain::ids::DeviceId;
use project::block::{AudioBlockKind, InputEntry, InsertBlock, OutputEntry};
use project::chain::{Chain, ChainInputMode, ChainOutputMode};

/// Resolve effective inputs for a chain.
///
/// Returns `(entries, cpal_indices, split_positions)` where:
/// - `entries[i]` is the `i`-th effective input — one per processing stream.
/// - `cpal_indices[i]` is the CPAL stream index for `entries[i]`. Multiple
///   effective inputs sharing the same device get the same CPAL stream
///   index (infra-cpal deduplicates streams by device).
/// - `split_positions[i]` is `Some(N)` when `entries[i]` came from a
///   split-mono original (one `InputBlock` with `mode: mono` and `N`
///   channels) and owns one output channel position out of `N`. The runtime
///   uses this to scale the segment's contribution by `1/N` at fan-out so
///   N loud guitars do not saturate the output limiter. Mono→stereo upmix
///   stays the historical broadcast — "mono in → stereo out is broadcast
///   to both channels" is preserved. `None` for stereo / dual-mono /
///   single-channel mono / Insert-return entries — they keep the historical
///   broadcast/sum behaviour.
pub(crate) fn effective_inputs(chain: &Chain) -> (Vec<InputEntry>, Vec<usize>, Vec<Option<usize>>) {
    let raw_entries: Vec<InputEntry> = chain
        .blocks
        .iter()
        .filter(|b| b.enabled)
        .filter_map(|b| match &b.kind {
            AudioBlockKind::Input(ib) => Some(ib),
            _ => None,
        })
        .flat_map(|ib| ib.entries.iter().cloned())
        .collect();

    let mut entries: Vec<InputEntry> = Vec::new();
    let mut cpal_indices: Vec<usize> = Vec::new();
    let mut split_positions: Vec<Option<usize>> = Vec::new();
    let mut device_to_cpal: HashMap<String, usize> = HashMap::new();
    let mut next_cpal_idx: usize = 0;

    for entry in raw_entries.iter() {
        let device_key = entry.device_id.0.clone();
        let cpal_idx = *device_to_cpal.entry(device_key).or_insert_with(|| {
            let idx = next_cpal_idx;
            next_cpal_idx += 1;
            idx
        });

        if matches!(entry.mode, ChainInputMode::Mono) && entry.channels.len() > 1 {
            // All split siblings get the SAME sibling count (total channels
            // split from the original mono entry). The runtime divides each
            // segment's contribution by this count at fan-out so N loud
            // guitars do not saturate the output limiter.
            let n = entry.channels.len();
            for &ch in entry.channels.iter() {
                entries.push(InputEntry {
                    device_id: entry.device_id.clone(),
                    mode: ChainInputMode::Mono,
                    channels: vec![ch],
                });
                cpal_indices.push(cpal_idx);
                split_positions.push(Some(n));
            }
        } else {
            entries.push(entry.clone());
            cpal_indices.push(cpal_idx);
            split_positions.push(None);
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
        entries.push(ret);
    }

    if !entries.is_empty() {
        return (entries, cpal_indices, split_positions);
    }
    // Fallback — no InputBlocks defined.
    (
        vec![InputEntry {
            device_id: DeviceId("".to_string()),
            mode: ChainInputMode::Mono,
            channels: vec![0],
        }],
        vec![0],
        vec![None],
    )
}

/// Build effective output entries from chain's `OutputBlock` entries, plus
/// Insert send entries. Order: `OutputBlock` entries first, then Insert
/// send entries (matches CPAL stream order). Falls back to a single mono
/// output on channel 0 if no `OutputBlock`s exist and no Inserts.
pub(crate) fn effective_outputs(chain: &Chain) -> Vec<OutputEntry> {
    let mut entries: Vec<OutputEntry> = chain
        .blocks
        .iter()
        .filter(|b| b.enabled)
        .filter_map(|b| match &b.kind {
            AudioBlockKind::Output(ob) => Some(ob),
            _ => None,
        })
        .flat_map(|ob| ob.entries.iter().cloned())
        .collect();

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
    // Fallback — no OutputBlocks defined.
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
