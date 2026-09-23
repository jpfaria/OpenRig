//! Responsibility: names the binding on both sides of each stream.

use domain::io_binding::IoBinding;
use project::binding_discovery::{resolve_chain_ports, PortDirection};
use project::block::AudioBlockKind;
use project::chain::Chain;

use crate::insert_endpoints::{insert_return_as_input_entry, insert_send_as_output_entry};
use crate::runtime_endpoints::{
    effective_inputs, effective_outputs, resolve_chain_io, resolve_chain_io_by_binding, InputEntry,
    OutputEntry,
};
use crate::runtime_segments::split_chain_into_segments;
use crate::segment_binding::{binding_of_raw_input, binding_of_route};

/// The E/S names one stream (meter row) carries: where its input comes from
/// and where its output goes (#928).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamIoLabels {
    pub input: String,
    pub output: String,
}

/// One entry per stream, in the same order `chain_stream_count` counts them —
/// the labels are read off the same segment map, so a row and its name can
/// never disagree.
pub fn chain_stream_io_labels(chain: &Chain, registry: &[IoBinding]) -> Vec<StreamIoLabels> {
    let (resolved_inputs, resolved_outputs) = resolve_chain_io(chain, registry);
    let (eff_inputs, cpal_indices, split_positions, entry_groups) =
        effective_inputs(chain, &resolved_inputs, registry);
    let eff_outputs = effective_outputs(chain, &resolved_outputs, registry);
    let segments = split_chain_into_segments(
        chain,
        &eff_inputs,
        &cpal_indices,
        &split_positions,
        &entry_groups,
        &eff_outputs,
        registry,
    );
    let by_binding = resolve_chain_io_by_binding(chain, registry);
    let inserts = insert_bindings(chain, registry);
    let ports = port_bindings(chain, registry);
    let name = |id: &str| binding_name(registry, id);

    segments
        .iter()
        .map(|segment| {
            let mut outputs: Vec<String> = Vec::new();
            for &route in &segment.output_route_indices {
                let Some(entry) = eff_outputs.get(route) else {
                    continue;
                };
                let id = inserts
                    .iter()
                    .find(|(_, _, send)| same_output(send, entry))
                    .map(|(id, _, _)| id.clone())
                    .or_else(|| binding_of_route(&by_binding, route).map(str::to_string))
                    .or_else(|| ports.output_owner(entry));
                if let Some(id) = id {
                    let label = name(&id);
                    if !outputs.contains(&label) {
                        outputs.push(label);
                    }
                }
            }
            // A head input is named by the E/S it was resolved FROM (its
            // position), never by its capture point — two E/S may read one
            // channel (#924/#928) and each row is still its own.
            let input_id = inserts
                .iter()
                .find(|(_, ret, _)| same_input(ret, &segment.input))
                .map(|(id, _, _)| id.clone())
                .or_else(|| {
                    binding_of_raw_input(&by_binding, segment.entry_group).map(str::to_string)
                })
                .or_else(|| ports.input_owner(&segment.input));
            StreamIoLabels {
                input: input_id.map(|id| name(&id)).unwrap_or_default(),
                output: outputs.join(" + "),
            }
        })
        .collect()
}

fn binding_name(registry: &[IoBinding], id: &str) -> String {
    registry
        .iter()
        .find(|b| b.id == id)
        .map(|b| b.name.clone())
        .unwrap_or_else(|| id.to_string())
}

fn same_input(a: &InputEntry, b: &InputEntry) -> bool {
    a.device_id == b.device_id && a.channels == b.channels
}

fn same_output(a: &OutputEntry, b: &OutputEntry) -> bool {
    a.device_id == b.device_id && a.channels == b.channels
}

/// `(binding id, return entry, send entry)` of every enabled, bound insert.
fn insert_bindings(
    chain: &Chain,
    registry: &[IoBinding],
) -> Vec<(String, InputEntry, OutputEntry)> {
    chain
        .blocks
        .iter()
        // The insert names the rows its CUT makes (#967: only an enabled
        // insert cuts; a disabled one's streams carry nothing to name).
        .filter(|b| crate::insert_cut::insert_cuts_chain(b, registry))
        .filter_map(|b| match &b.kind {
            AudioBlockKind::Insert(ib) => Some((
                ib.io.clone(),
                insert_return_as_input_entry(ib, registry)?,
                insert_send_as_output_entry(ib, registry)?,
            )),
            _ => None,
        })
        .collect()
}

/// Mid `Input` / `Output` ports: which E/S each resolved endpoint came from.
struct PortBindings {
    inputs: Vec<(String, InputEntry)>,
    outputs: Vec<(String, OutputEntry)>,
}

impl PortBindings {
    fn input_owner(&self, e: &InputEntry) -> Option<String> {
        self.inputs
            .iter()
            .find(|(_, i)| same_input(i, e))
            .map(|(id, _)| id.clone())
    }

    fn output_owner(&self, e: &OutputEntry) -> Option<String> {
        self.outputs
            .iter()
            .find(|(_, o)| same_output(o, e))
            .map(|(id, _)| id.clone())
    }
}

fn port_bindings(chain: &Chain, registry: &[IoBinding]) -> PortBindings {
    let mut inputs = Vec::new();
    let mut outputs = Vec::new();
    for port in resolve_chain_ports(chain, registry) {
        match port.direction {
            PortDirection::Input => inputs.push((
                port.binding_id,
                InputEntry {
                    device_id: port.endpoint.device_id,
                    mode: port.endpoint.mode.into(),
                    channels: port.endpoint.channels,
                },
            )),
            PortDirection::Output => outputs.push((
                port.binding_id,
                OutputEntry {
                    device_id: port.endpoint.device_id,
                    mode: project::chain::ChainOutputMode::try_from(port.endpoint.mode)
                        .unwrap_or(project::chain::ChainOutputMode::Stereo),
                    channels: port.endpoint.channels,
                },
            )),
        }
    }
    PortBindings { inputs, outputs }
}
