//! Responsibility: resolves a chain's ports into the engine's endpoint entries.

use domain::ids::DeviceId;
use domain::io_binding::IoBinding;
use project::binding_discovery::{resolve_chain_ports, PortDirection};
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
pub fn resolve_chain_io(
    chain: &Chain,
    registry: &[IoBinding],
) -> (Vec<InputEntry>, Vec<OutputEntry>) {
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

/// A chain's I/O resolved and GROUPED by the binding it came from. Each group
/// pairs a binding's input endpoint(s) with that SAME binding's output
/// endpoint(s), so an input never cross-routes to another binding's output
/// (#716: "one stream per (input, output) pair WITHIN the same binding").
#[derive(Debug, Clone, PartialEq)]
pub struct BindingIo {
    pub binding_id: String,
    pub inputs: Vec<InputEntry>,
    pub outputs: Vec<OutputEntry>,
}

/// Resolve a chain's I/O grouped per referenced binding (in `io_binding_ids`
/// order). With a single binding this is one group whose flattened inputs and
/// outputs equal [`resolve_chain_io`] — so single-binding routing is unchanged.
pub fn resolve_chain_io_by_binding(chain: &Chain, registry: &[IoBinding]) -> Vec<BindingIo> {
    let mut groups: Vec<BindingIo> = Vec::new();
    for port in resolve_chain_ports(chain, registry) {
        let idx = match groups.iter().position(|g| g.binding_id == port.binding_id) {
            Some(i) => i,
            None => {
                groups.push(BindingIo {
                    binding_id: port.binding_id.clone(),
                    inputs: Vec::new(),
                    outputs: Vec::new(),
                });
                groups.len() - 1
            }
        };
        match port.direction {
            PortDirection::Input => groups[idx].inputs.push(InputEntry {
                device_id: port.endpoint.device_id,
                mode: ChainInputMode::from(port.endpoint.mode),
                channels: port.endpoint.channels,
            }),
            PortDirection::Output => groups[idx].outputs.push(OutputEntry {
                device_id: port.endpoint.device_id,
                mode: ChainOutputMode::try_from(port.endpoint.mode)
                    .unwrap_or(ChainOutputMode::Stereo),
                channels: port.endpoint.channels,
            }),
        }
    }
    groups
}
