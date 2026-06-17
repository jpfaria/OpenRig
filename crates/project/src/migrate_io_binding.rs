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

use crate::block::AudioBlockKind;
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

        // First pass: collect raw (device_id, mode, channels) tuples so we can
        // assign chain-wide unique names before building endpoints.
        // We do two separate passes: one to collect, one to build with names.

        // Collect raw input entries across all legacy blocks.
        let mut raw_inputs: Vec<(usize, RawEntry)> = Vec::new(); // (block_idx, entry)
        let mut raw_outputs: Vec<(usize, RawEntry)> = Vec::new();

        for (idx, block) in chain.blocks.iter().enumerate() {
            match &block.kind {
                AudioBlockKind::Input(ib) if ib.io.is_empty() && !ib.entries.is_empty() => {
                    for entry in &ib.entries {
                        raw_inputs.push((
                            idx,
                            RawEntry {
                                device_id: entry.device_id.clone(),
                                mode: ChannelMode::from(entry.mode),
                                channels: entry.channels.clone(),
                            },
                        ));
                    }
                }
                AudioBlockKind::Output(ob) if ob.io.is_empty() && !ob.entries.is_empty() => {
                    for entry in &ob.entries {
                        raw_outputs.push((
                            idx,
                            RawEntry {
                                device_id: entry.device_id.clone(),
                                mode: ChannelMode::from(entry.mode),
                                channels: entry.channels.clone(),
                            },
                        ));
                    }
                }
                _ => {}
            }
        }

        // Build IoEndpoints with chain-wide unique names.
        // Identical (device_id, channels, mode) tuples across different blocks share
        // one endpoint; distinct tuples get distinct names including a suffix when
        // two entries share the same device.
        let named_inputs = assign_unique_names(&raw_inputs, Direction::Input);
        let named_outputs = assign_unique_names(&raw_outputs, Direction::Output);

        // Group by block index so we know which endpoint name each block gets.
        for (idx, block) in chain.blocks.iter().enumerate() {
            match &block.kind {
                AudioBlockKind::Input(ib) if ib.io.is_empty() && !ib.entries.is_empty() => {
                    let endpoints: Vec<IoEndpoint> = named_inputs
                        .iter()
                        .filter(|(bidx, _)| *bidx == idx)
                        .map(|(_, ep)| ep.clone())
                        .collect();
                    if !endpoints.is_empty() {
                        input_migrations.push((idx, endpoints));
                    }
                }
                AudioBlockKind::Output(ob) if ob.io.is_empty() && !ob.entries.is_empty() => {
                    let endpoints: Vec<IoEndpoint> = named_outputs
                        .iter()
                        .filter(|(bidx, _)| *bidx == idx)
                        .map(|(_, ep)| ep.clone())
                        .collect();
                    if !endpoints.is_empty() {
                        output_migrations.push((idx, endpoints));
                    }
                }
                _ => {}
            }
        }

        // Nothing to migrate in this chain.
        if input_migrations.is_empty() && output_migrations.is_empty() {
            continue;
        }

        // Gather all endpoints for this chain into a single binding.
        // Deduplicate: identical (device, channels, mode) tuples share one endpoint.
        let all_inputs: Vec<IoEndpoint> =
            dedup_endpoints(named_inputs.iter().map(|(_, ep)| ep.clone()));
        let all_outputs: Vec<IoEndpoint> =
            dedup_endpoints(named_outputs.iter().map(|(_, ep)| ep.clone()));

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

/// Raw entry data collected from a legacy block before naming.
struct RawEntry {
    device_id: domain::ids::DeviceId,
    mode: ChannelMode,
    channels: Vec<usize>,
}

/// Direction tag for building human-readable endpoint names.
enum Direction {
    Input,
    Output,
}

/// Assign chain-wide unique names to a flat list of raw entries.
///
/// Entries with the **same** (device_id, channels, mode) receive the **same**
/// name — they are genuinely identical and will be deduplicated into one
/// `IoEndpoint`.  Entries that differ only in channel set get a numeric
/// suffix (`In1`, `In2`, …) so the names remain distinct and stable.
///
/// Returns `Vec<(block_idx, IoEndpoint)>` in the same order as the input.
fn assign_unique_names(
    entries: &[(usize, RawEntry)],
    direction: Direction,
) -> Vec<(usize, IoEndpoint)> {
    let prefix = match direction {
        Direction::Input => "In",
        Direction::Output => "Out",
    };

    // Count how many entries share each device_id so we know when a suffix
    // is needed.  Two entries that are fully identical (same device + channels
    // + mode) do NOT need a suffix — they will dedup later.  Only entries that
    // differ by channel set need disambiguation.
    //
    // Strategy: for each device_id, collect the distinct (channels, mode)
    // tuples.  If there is more than one distinct tuple for a device, assign
    // a per-device counter.
    use std::collections::HashMap;

    // Map device_id → list of distinct (channels, mode) tuples (in order of
    // first appearance).
    let mut device_distinct: HashMap<&str, Vec<(&[usize], ChannelMode)>> = HashMap::new();
    for (_, entry) in entries {
        let device = entry.device_id.0.as_str();
        let list = device_distinct.entry(device).or_default();
        let already = list
            .iter()
            .any(|(ch, m)| *ch == entry.channels.as_slice() && *m == entry.mode);
        if !already {
            list.push((&entry.channels, entry.mode));
        }
    }

    // Now assign names: for each entry, look up whether its device has > 1
    // distinct tuple.  If yes, find the 1-based index of this entry's tuple
    // within that device's list and append it.
    let mut result = Vec::with_capacity(entries.len());
    for (block_idx, entry) in entries {
        let device = entry.device_id.0.as_str();
        let distinct = &device_distinct[device];
        let name = if distinct.len() == 1 {
            // Only one distinct tuple for this device — no suffix needed.
            format!("{} ({})", prefix, entry.device_id.0)
        } else {
            // Multiple distinct tuples — find 1-based index of this one.
            let pos = distinct
                .iter()
                .position(|(ch, m)| *ch == entry.channels.as_slice() && *m == entry.mode)
                .unwrap_or(0);
            format!("{}{} ({})", prefix, pos + 1, entry.device_id.0)
        };
        result.push((
            *block_idx,
            IoEndpoint {
                name,
                device_id: entry.device_id.clone(),
                mode: entry.mode,
                channels: entry.channels.clone(),
            },
        ));
    }
    result
}

/// Deduplicate endpoints by name, preserving first-occurrence order.
///
/// Two entries with the same name are genuinely identical (same device +
/// channels + mode produced the same name via `assign_unique_names`).
fn dedup_endpoints(iter: impl Iterator<Item = IoEndpoint>) -> Vec<IoEndpoint> {
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut out = Vec::new();
    for ep in iter {
        if seen.insert(ep.name.clone()) {
            out.push(ep);
        }
    }
    out
}
