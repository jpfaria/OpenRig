//! Default-chain construction — pure business logic.
//!
//! Provides a canonical starting-point `Chain` for the `AddChain` command.
//! Lives in `crates/application` so every transport (adapter-gui, adapter-grpc,
//! adapter-mcp, CLI) starts from the same baseline; no UI types leak in.
//!
//! **Spec reference:** `docs/superpowers/specs/2026-04-23-command-dispatch-architecture-design.md`

use domain::ids::{BlockId, ChainId, DeviceId};
use project::block::{
    AudioBlock, AudioBlockKind, InputBlock, InputEntry, OutputBlock, OutputEntry,
};
use project::chain::{ChainInputMode, ChainOutputMode};
use project::project::Project;

/// I/O endpoint configuration for [`build_default_chain`].
///
/// Groups the device id + channel list for one side (input or output) so the
/// factory takes one cohesive value per side instead of two loose arguments.
///
/// `io` and `endpoint` carry the I/O binding reference (#716, Task 20). Both
/// default to empty string (unbound). When set, the block's `io`/`endpoint`
/// fields are stamped accordingly so the chain immediately references the
/// binding instead of requiring a subsequent `SaveChainInputEndpoints` dispatch.
pub struct EndpointSpec<'a> {
    /// Optional device id string. `None` => empty entries on this side.
    pub device_id: Option<&'a str>,
    /// Channel indices for this side. Empty => empty entries on this side.
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
/// - One `InputBlock` with the provided device + channels (or empty entries if
///   no device is specified).
/// - One `OutputBlock` with the provided device + channels (or empty entries).
/// - An empty effects list.
pub fn build_default_chain(params: DefaultChainParams<'_>) -> project::chain::Chain {
    let DefaultChainParams {
        project,
        instrument,
        description,
        input,
        output,
    } = params;
    let input_device_id = input.device_id;
    let input_channels = input.channels;
    let input_io = input.io;
    let input_endpoint = input.endpoint;
    let output_device_id = output.device_id;
    let output_channels = output.channels;
    let output_io = output.io;
    let output_endpoint = output.endpoint;
    let chain_idx = project.chains.len();
    let chain_id = ChainId::generate();

    let mut blocks = Vec::new();

    // InputBlock — always present; entries may be empty if no device resolved.
    let input_entries = if let Some(dev_id) = input_device_id {
        if !input_channels.is_empty() {
            vec![InputEntry {
                device_id: DeviceId(dev_id.to_string()),
                mode: ChainInputMode::Mono,
                channels: input_channels,
            }]
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };
    blocks.push(AudioBlock {
        id: BlockId(format!("{}:input:0", chain_idx)),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".to_string(),
            io: input_io,
            endpoint: input_endpoint,
            entries: input_entries,
        }),
    });

    // OutputBlock — always present.
    let output_entries = if let Some(dev_id) = output_device_id {
        if !output_channels.is_empty() {
            vec![OutputEntry {
                device_id: DeviceId(dev_id.to_string()),
                mode: ChainOutputMode::Stereo,
                channels: output_channels,
            }]
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };
    blocks.push(AudioBlock {
        id: BlockId(format!("{}:output:0", chain_idx)),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".to_string(),
            io: output_io,
            endpoint: output_endpoint,
            entries: output_entries,
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
