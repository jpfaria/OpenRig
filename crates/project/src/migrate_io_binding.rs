//! Task 6 — Migrate legacy chain I/O entries into the io_bindings registry.
//!
//! Chains that were created before the I/O binding registry (#716) store device
//! endpoints directly inside each `InputBlock`/`OutputBlock` as `entries`. This
//! module converts those legacy chains to the new schema in one pass:
//!
//! 1. For each chain whose input/output blocks still have `entries` (legacy):
//!    - Collect all input endpoints + all output endpoints from every block in
//!      the chain.
//!    - Convert each entry to an `IoEndpoint` (forward mode conversion).
//!    - Find or create ONE `IoBinding` that holds all those endpoints (dedup by
//!      content hash so identical bindings across chains are shared).
//!    - Rewrite each input/output block to `{ io: <id>, endpoint: <name> }`.
//!    - Drain `entries` from every migrated block.
//!
//! 2. Blocks that already have a non-empty `io` field are skipped — the
//!    function is idempotent: running it twice leaves the project unchanged.
//!
//! The caller (project load path) passes `&mut Vec<IoBinding>` from
//! `AppConfig::io_bindings`; new bindings are appended there.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};

use crate::block::{AudioBlockKind, InputBlock, OutputBlock};
use crate::project::Project;

/// Migrate all legacy chain I/O entries in `project` into `io_bindings`.
///
/// For each chain with legacy `entries` on its input/output blocks:
/// - Creates (or reuses) one `IoBinding` covering all that chain's endpoints.
/// - Rewrites the blocks to reference the binding by `io` + `endpoint` name.
/// - Drains `entries` so the migration is idempotent.
///
/// New bindings are appended to `io_bindings`. Bindings with an identical
/// endpoint set are deduplicated: if one already exists (matched by a
/// deterministic content hash), its `id` is reused instead.
pub fn migrate_legacy_io(project: &mut Project, io_bindings: &mut Vec<IoBinding>) {
    for chain in &mut project.chains {
        // Collect block indices and endpoint data for all legacy input/output blocks.
        // A block is "legacy" if its io field is empty AND it has entries.
        let mut input_migrations: Vec<(usize, Vec<IoEndpoint>)> = Vec::new();
        let mut output_migrations: Vec<(usize, Vec<IoEndpoint>)> = Vec::new();

        for (idx, block) in chain.blocks.iter().enumerate() {
            match &block.kind {
                AudioBlockKind::Input(ib) if ib.io.is_empty() && !ib.entries.is_empty() => {
                    let endpoints = ib
                        .entries
                        .iter()
                        .enumerate()
                        .map(|(entry_idx, entry)| IoEndpoint {
                            name: endpoint_name_for_input(&ib, entry_idx),
                            device_id: entry.device_id.clone(),
                            mode: ChannelMode::from(entry.mode),
                            channels: entry.channels.clone(),
                        })
                        .collect();
                    input_migrations.push((idx, endpoints));
                }
                AudioBlockKind::Output(ob) if ob.io.is_empty() && !ob.entries.is_empty() => {
                    let endpoints = ob
                        .entries
                        .iter()
                        .enumerate()
                        .map(|(entry_idx, entry)| IoEndpoint {
                            name: endpoint_name_for_output(&ob, entry_idx),
                            device_id: entry.device_id.clone(),
                            mode: ChannelMode::from(entry.mode),
                            channels: entry.channels.clone(),
                        })
                        .collect();
                    output_migrations.push((idx, endpoints));
                }
                _ => {}
            }
        }

        // Nothing to migrate in this chain.
        if input_migrations.is_empty() && output_migrations.is_empty() {
            continue;
        }

        // Gather all endpoints for this chain into a single binding.
        let all_inputs: Vec<IoEndpoint> = input_migrations
            .iter()
            .flat_map(|(_, eps)| eps.iter().cloned())
            .collect();
        let all_outputs: Vec<IoEndpoint> = output_migrations
            .iter()
            .flat_map(|(_, eps)| eps.iter().cloned())
            .collect();

        // Find or create the binding (deduplicated by content).
        let binding_id =
            find_or_create_binding(io_bindings, all_inputs.clone(), all_outputs.clone());

        // Rewrite input blocks: set io/endpoint, drain entries.
        for (idx, endpoints) in &input_migrations {
            // Each block contributed one or more endpoints. Each entry becomes
            // one endpoint; the block gets the first entry's endpoint name
            // when there is exactly one. When a block had multiple entries
            // they were already coalesced by Task 2 into one block with
            // multiple entries; we pick the name of the first endpoint.
            let endpoint_name = endpoints
                .first()
                .map(|ep| ep.name.clone())
                .unwrap_or_default();
            if let AudioBlockKind::Input(ref mut ib) = chain.blocks[*idx].kind {
                ib.io = binding_id.clone();
                ib.endpoint = endpoint_name;
                ib.entries.clear();
            }
        }

        // Rewrite output blocks: set io/endpoint, drain entries.
        for (idx, endpoints) in &output_migrations {
            let endpoint_name = endpoints
                .first()
                .map(|ep| ep.name.clone())
                .unwrap_or_default();
            if let AudioBlockKind::Output(ref mut ob) = chain.blocks[*idx].kind {
                ob.io = binding_id.clone();
                ob.endpoint = endpoint_name;
                ob.entries.clear();
            }
        }
    }
}

/// Return the `id` of an existing binding that has the same endpoint set, or
/// create a new one, append it to `io_bindings`, and return its id.
fn find_or_create_binding(
    io_bindings: &mut Vec<IoBinding>,
    inputs: Vec<IoEndpoint>,
    outputs: Vec<IoEndpoint>,
) -> String {
    let target_hash = endpoint_set_hash(&inputs, &outputs);

    // Check for an existing binding with the same content hash.
    for binding in io_bindings.iter() {
        if endpoint_set_hash(&binding.inputs, &binding.outputs) == target_hash {
            return binding.id.clone();
        }
    }

    // No match — create a new binding with a deterministic auto-id.
    let new_id = format!("auto_{:016x}", target_hash);
    let new_binding = IoBinding {
        id: new_id.clone(),
        // Name defaults to the first input endpoint's device id for readability;
        // the user can rename via the settings UI later.
        name: inputs
            .first()
            .map(|ep| ep.device_id.0.clone())
            .or_else(|| outputs.first().map(|ep| ep.device_id.0.clone()))
            .unwrap_or_else(|| "Legacy".into()),
        inputs,
        outputs,
    };
    io_bindings.push(new_binding);
    new_id
}

/// Stable content hash for a set of input + output endpoints.
///
/// The hash is derived from each endpoint's device_id, mode, and channel list,
/// collected in the order they appear. Order matters: two bindings with the
/// same endpoints in different orders are treated as distinct (preserving the
/// original routing intent).
fn endpoint_set_hash(inputs: &[IoEndpoint], outputs: &[IoEndpoint]) -> u64 {
    let mut hasher = DefaultHasher::new();
    for ep in inputs {
        ep.device_id.0.hash(&mut hasher);
        ep.name.hash(&mut hasher);
        // mode discriminant
        let mode_tag: u8 = match ep.mode {
            ChannelMode::Mono => 0,
            ChannelMode::Stereo => 1,
            ChannelMode::DualMono => 2,
        };
        mode_tag.hash(&mut hasher);
        ep.channels.hash(&mut hasher);
    }
    // Separator between inputs and outputs so (inputs=[A], outputs=[]) ≠
    // (inputs=[], outputs=[A]).
    0xFF_u8.hash(&mut hasher);
    for ep in outputs {
        ep.device_id.0.hash(&mut hasher);
        ep.name.hash(&mut hasher);
        let mode_tag: u8 = match ep.mode {
            ChannelMode::Mono => 0,
            ChannelMode::Stereo => 1,
            ChannelMode::DualMono => 2,
        };
        mode_tag.hash(&mut hasher);
        ep.channels.hash(&mut hasher);
    }
    hasher.finish()
}

/// Generate a stable endpoint name for an input block entry.
///
/// Uses the entry index so that multiple entries in the same block
/// get distinct names. The index is 1-based for readability.
fn endpoint_name_for_input(block: &InputBlock, entry_idx: usize) -> String {
    // If the block has exactly one entry, use a clean name without an index.
    if block.entries.len() == 1 {
        format!("In ({})", block.entries[0].device_id.0)
    } else {
        format!("In {} ({})", entry_idx + 1, block.entries[entry_idx].device_id.0)
    }
}

/// Generate a stable endpoint name for an output block entry.
fn endpoint_name_for_output(block: &OutputBlock, entry_idx: usize) -> String {
    if block.entries.len() == 1 {
        format!("Out ({})", block.entries[0].device_id.0)
    } else {
        format!(
            "Out {} ({})",
            entry_idx + 1,
            block.entries[entry_idx].device_id.0
        )
    }
}
