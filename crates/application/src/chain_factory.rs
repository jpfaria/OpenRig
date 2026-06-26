//! Default-chain construction — pure business logic.
//!
//! Provides a canonical starting-point `Chain` for the `AddChain` command.
//! Lives in `crates/application` so every transport (adapter-gui, adapter-grpc,
//! adapter-mcp, CLI) starts from the same baseline; no UI types leak in.
//!
//! **Spec reference:** `docs/superpowers/specs/2026-04-23-command-dispatch-architecture-design.md`

use domain::ids::{BlockId, ChainId};
use project::block::{AudioBlock, AudioBlockKind, InputBlock, OutputBlock};
use project::project::Project;

/// I/O endpoint configuration for [`build_default_chain`].
///
/// `io` and `endpoint` carry the I/O binding reference (#716, model A). Both
/// default to empty string (unbound). When set, the block's `io`/`endpoint`
/// fields are stamped accordingly so the chain immediately references the
/// binding. The chain itself never embeds device endpoints — the device /
/// channels are resolved from the per-machine binding registry.
///
/// `device_id`/`channels` are retained for caller ergonomics (legacy call
/// sites still pass them) but are no longer written into the block: the device
/// data lives only in the binding registry now.
pub struct EndpointSpec<'a> {
    /// Optional device id string. Retained for caller compatibility; not stored
    /// in the block (device data comes from the binding registry, #716).
    pub device_id: Option<&'a str>,
    /// Channel indices. Retained for caller compatibility; not stored in the
    /// block (channel data comes from the binding registry, #716).
    pub channels: Vec<usize>,
    /// I/O binding id for this block (e.g. `"default"`). Empty = unbound.
    pub io: String,
    /// Endpoint name within the binding (e.g. `"In1"`, `"Out1"`). Empty = unbound.
    pub endpoint: String,
}

/// Parameters for [`build_default_chain`], grouped to keep the call site and
/// signature cohesive (one struct instead of seven positional arguments).
pub struct DefaultChainParams<'a> {
    /// The current project; used only to determine the next available chain
    /// index for default ids. **Not mutated.**
    pub project: &'a Project,
    /// Instrument key (e.g. `"electric_guitar"`). Use
    /// `block_core::DEFAULT_INSTRUMENT` if unknown.
    pub instrument: &'a str,
    /// Optional human-readable chain name.
    pub description: Option<String>,
    /// Input-side endpoint spec.
    pub input: EndpointSpec<'a>,
    /// Output-side endpoint spec.
    pub output: EndpointSpec<'a>,
}

/// Build a new [`Chain`] with default I/O blocks ready for dispatch via
/// `Command::AddChain`.
///
/// The returned chain has:
/// - A freshly-generated `ChainId`.
/// - `enabled = false` (the user toggles it on explicitly after creation).
/// - `description = None` (callers may set a name from the UI draft or CLI arg
///   before dispatching).
/// - One `InputBlock` referencing the provided `io`/`endpoint` binding.
/// - One `OutputBlock` referencing the provided `io`/`endpoint` binding.
/// - An empty effects list.
///
/// Device/channel data is NOT stored on the chain (#716, model A): it is
/// resolved from the per-machine binding registry via
/// `engine::runtime_endpoints::resolve_chain_io`.
pub fn build_default_chain(params: DefaultChainParams<'_>) -> project::chain::Chain {
    let DefaultChainParams {
        project,
        instrument,
        description,
        input,
        output,
    } = params;
    let input_io = input.io;
    let input_endpoint = input.endpoint;
    let output_io = output.io;
    let output_endpoint = output.endpoint;
    let chain_idx = project.chains.len();
    let chain_id = ChainId::generate();

    let mut blocks = Vec::new();

    // InputBlock — always present; references the binding (device resolved from
    // the registry, never embedded in the chain).
    blocks.push(AudioBlock {
        id: BlockId(format!("{}:input:0", chain_idx)),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".to_string(),
            io: input_io,
            endpoint: input_endpoint,
        }),
    });

    // OutputBlock — always present.
    blocks.push(AudioBlock {
        id: BlockId(format!("{}:output:0", chain_idx)),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".to_string(),
            io: output_io,
            endpoint: output_endpoint,
        }),
    });

    project::chain::Chain {
        id: chain_id,
        description,
        instrument: instrument.to_string(),
        enabled: false,
        volume: 100.0,
        io_binding_ids: Vec::new(),
        blocks,
    }
}
