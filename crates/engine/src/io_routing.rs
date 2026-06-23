//! Per-binding stream resolution (issue #716).
//!
//! Setup-time only — runs when a chain is built or rebuilt, never on the audio
//! thread. Resolves a chain's `InputBlock` / `OutputBlock` *ports* (each
//! referencing an io binding + endpoint) against the io-binding registry and
//! enumerates the streams the engine must spawn.
//!
//! The model (CLAUDE.md invariant #4):
//!   - An input port carries an endpoint and references a binding.
//!   - An output port likewise.
//!   - A **stream** is spawned for each `(input port, output port)` pair that
//!     belongs to the SAME binding, with `inputPos <= outputPos` (chain block
//!     order). The stream runs ONLY the effect blocks strictly between the two
//!     ports, reads the input endpoint, and writes the output endpoint.
//!
//! Because pairing is scoped to a binding, the input of binding A can never
//! reach the output of binding B — structural isolation, enforced here at
//! build time with zero audio-thread cost.
//!
//! Legacy / unbound blocks (empty `io`) are not resolved here; the legacy
//! `entries`-based path in `runtime_endpoints` / `runtime_segments` keeps its
//! current cartesian behaviour untouched.

use domain::io_binding::{IoBinding, IoEndpoint};
use project::block::{AudioBlockKind, InputEntry, OutputEntry};
use project::chain::{Chain, ChainInputMode, ChainOutputMode};

/// An input port resolved against the registry: its binding/endpoint, the
/// concrete device entry, and the chain block position where the port sits.
struct ResolvedInputPort {
    binding: String,
    endpoint: String,
    pos: usize,
    entry: InputEntry,
}

/// An output port resolved against the registry.
struct ResolvedOutputPort {
    binding: String,
    endpoint: String,
    pos: usize,
    entry: OutputEntry,
}

/// One stream the engine must spawn: a same-binding `(input, output)` pair and
/// the effect blocks strictly between the two ports.
pub struct ResolvedStream {
    /// Binding id of the input port (== `output_binding` by construction).
    pub input_binding: String,
    /// Endpoint name read by this stream.
    pub input_endpoint: String,
    /// Binding id of the output port.
    pub output_binding: String,
    /// Endpoint name written by this stream.
    pub output_endpoint: String,
    /// Chain block indices of the effect blocks strictly between the ports.
    pub block_indices: Vec<usize>,
    /// Concrete device input entry resolved from the registry.
    pub input_entry: InputEntry,
    /// Concrete device output entry resolved from the registry.
    pub output_entry: OutputEntry,
}

/// Look up an endpoint by name within a binding's input list.
fn find_input_endpoint<'a>(binding: &'a IoBinding, name: &str) -> Option<&'a IoEndpoint> {
    binding.inputs.iter().find(|e| e.name == name)
}

/// Look up an endpoint by name within a binding's output list.
fn find_output_endpoint<'a>(binding: &'a IoBinding, name: &str) -> Option<&'a IoEndpoint> {
    binding.outputs.iter().find(|e| e.name == name)
}

/// Convert a registry input endpoint to the runtime's `InputEntry`. The
/// channel-mode bridge (`From<ChannelMode>`) keeps the vocabularies in sync.
fn input_entry_from_endpoint(ep: &IoEndpoint) -> InputEntry {
    InputEntry {
        device_id: ep.device_id.clone(),
        mode: ChainInputMode::from(ep.mode),
        channels: ep.channels.clone(),
    }
}

/// Convert a registry output endpoint to the runtime's `OutputEntry`. Outputs
/// have no dual-mono layout; `DualMono` falls back to stereo (the endpoint is
/// still carried as two channels).
fn output_entry_from_endpoint(ep: &IoEndpoint) -> OutputEntry {
    let mode = ChainOutputMode::try_from(ep.mode).unwrap_or(ChainOutputMode::Stereo);
    OutputEntry {
        device_id: ep.device_id.clone(),
        mode,
        channels: ep.channels.clone(),
    }
}

/// Resolve every bound input port in chain block order.
fn resolve_input_ports(chain: &Chain, registry: &[IoBinding]) -> Vec<ResolvedInputPort> {
    let mut ports = Vec::new();
    for (pos, block) in chain.blocks.iter().enumerate() {
        if !block.enabled {
            continue;
        }
        if let AudioBlockKind::Input(ib) = &block.kind {
            if ib.io.is_empty() {
                continue;
            }
            let Some(binding) = registry.iter().find(|b| b.id == ib.io) else {
                continue;
            };
            let Some(ep) = find_input_endpoint(binding, &ib.endpoint) else {
                continue;
            };
            ports.push(ResolvedInputPort {
                binding: ib.io.clone(),
                endpoint: ib.endpoint.clone(),
                pos,
                entry: input_entry_from_endpoint(ep),
            });
        }
    }
    ports
}

/// Resolve every bound output port in chain block order.
fn resolve_output_ports(chain: &Chain, registry: &[IoBinding]) -> Vec<ResolvedOutputPort> {
    let mut ports = Vec::new();
    for (pos, block) in chain.blocks.iter().enumerate() {
        if !block.enabled {
            continue;
        }
        if let AudioBlockKind::Output(ob) = &block.kind {
            if ob.io.is_empty() {
                continue;
            }
            let Some(binding) = registry.iter().find(|b| b.id == ob.io) else {
                continue;
            };
            let Some(ep) = find_output_endpoint(binding, &ob.endpoint) else {
                continue;
            };
            ports.push(ResolvedOutputPort {
                binding: ob.io.clone(),
                endpoint: ob.endpoint.clone(),
                pos,
                entry: output_entry_from_endpoint(ep),
            });
        }
    }
    ports
}

/// Effect blocks strictly between two chain positions (exclusive), skipping
/// I/O and Insert routing blocks. This is the block range a stream runs.
fn effect_blocks_between(chain: &Chain, after_pos: usize, before_pos: usize) -> Vec<usize> {
    chain
        .blocks
        .iter()
        .enumerate()
        .filter(|(i, b)| {
            *i > after_pos
                && *i < before_pos
                && b.enabled
                && !matches!(
                    &b.kind,
                    AudioBlockKind::Input(_)
                        | AudioBlockKind::Output(_)
                        | AudioBlockKind::Insert(_)
                )
        })
        .map(|(i, _)| i)
        .collect()
}

/// Enumerate the streams a chain must spawn under the per-binding routing rule.
///
/// For each `(input port, output port)` pair that shares a binding and has
/// `inputPos <= outputPos`, one stream is produced running the effect blocks
/// strictly between the ports. Ports whose binding/endpoint cannot be resolved
/// against `registry` are skipped (they produce no stream rather than a wrong
/// one — an honest absence, not a silent mis-route).
pub fn resolve_chain_streams(chain: &Chain, registry: &[IoBinding]) -> Vec<ResolvedStream> {
    let inputs = resolve_input_ports(chain, registry);
    let outputs = resolve_output_ports(chain, registry);

    let mut streams = Vec::new();
    for inp in &inputs {
        for out in &outputs {
            if inp.binding != out.binding {
                continue;
            }
            if inp.pos > out.pos {
                continue;
            }
            streams.push(ResolvedStream {
                input_binding: inp.binding.clone(),
                input_endpoint: inp.endpoint.clone(),
                output_binding: out.binding.clone(),
                output_endpoint: out.endpoint.clone(),
                block_indices: effect_blocks_between(chain, inp.pos, out.pos),
                input_entry: inp.entry.clone(),
                output_entry: out.entry.clone(),
            });
        }
    }
    streams
}

/// Whether a chain has any bound (non-legacy) input or output port. Bound
/// chains take the per-binding routing path; legacy chains keep the existing
/// `entries`-based segmentation untouched.
pub fn chain_has_bound_ports(chain: &Chain) -> bool {
    chain.blocks.iter().any(|b| {
        b.enabled
            && match &b.kind {
                AudioBlockKind::Input(ib) => !ib.io.is_empty(),
                AudioBlockKind::Output(ob) => !ob.io.is_empty(),
                _ => false,
            }
    })
}
