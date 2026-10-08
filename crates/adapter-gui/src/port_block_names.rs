//! Responsibility: names each port block of a chain after what it plays through.
//!
//! #398. An insert card shows the binding it plays through ("Scarlett"),
//! never its kind a second time ("INSERT" over "INSERT"). #1103: a mid-chain
//! input/output card shows the device and channels of its endpoint
//! ("Quantum HD 8 · Out 1/2"), named like the chain's own input/output rows;
//! an endpoint the binding no longer has falls back to the binding name. A
//! binding gone from the registry keeps its id; a port with no binding reads
//! `none`.

use domain::AudioDeviceDescriptor;
use infra_filesystem::IoBinding;
use project::block::{walk_blocks, AudioBlockKind};
use project::chain::Chain;
use project::physical_endpoint_label::physical_endpoint_label;

/// `(block id, name)` for every port block, at any depth.
pub(crate) fn port_block_names(
    chain: &Chain,
    registry: &[IoBinding],
    input_devices: &[AudioDeviceDescriptor],
    output_devices: &[AudioDeviceDescriptor],
    none: &str,
) -> Vec<(String, String)> {
    walk_blocks(&chain.blocks)
        .into_iter()
        .filter_map(|block| {
            let (io, port) = match &block.kind {
                AudioBlockKind::Insert(insert) => (&insert.io, None),
                AudioBlockKind::Input(input) => (&input.io, Some((true, &input.endpoint))),
                AudioBlockKind::Output(output) => (&output.io, Some((false, &output.endpoint))),
                _ => return None,
            };
            if io.is_empty() {
                return Some((block.id.0.clone(), none.to_string()));
            }
            let Some(binding) = registry.iter().find(|binding| &binding.id == io) else {
                return Some((block.id.0.clone(), io.clone()));
            };
            let device_label = port.and_then(|(is_input, endpoint)| {
                let (endpoints, direction, devices) = if is_input {
                    (&binding.inputs, "In", input_devices)
                } else {
                    (&binding.outputs, "Out", output_devices)
                };
                endpoints.iter().find(|e| &e.name == endpoint).map(|e| {
                    physical_endpoint_label(direction, &e.device_id.0, &e.channels, devices)
                })
            });
            Some((
                block.id.0.clone(),
                device_label.unwrap_or_else(|| binding.name.clone()),
            ))
        })
        .collect()
}
