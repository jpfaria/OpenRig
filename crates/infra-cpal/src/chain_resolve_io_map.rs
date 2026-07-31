//! #85 / #716 — which output devices each of a chain's input streams feeds.
//!
//! Split out of `chain_resolve.rs` (line cap). The rule it encodes is the
//! stream-isolation LAW: an output device's stream mixes ONLY the runtimes whose
//! own chain writes to that device, never "all runtimes at the same rate".

use std::collections::HashMap;

use domain::io_binding::IoBinding;
use engine::runtime_endpoints::{resolve_chain_io_by_binding, InputEntry};
use project::binding_discovery::{resolve_chain_ports, PortDirection};
use project::chain::Chain;

/// LAW (stream isolation): each output device's stream mixes ONLY the runtimes
/// whose OWN chain writes to that device — never all runtimes at the same rate.
/// Maps each input cpal index (Nth distinct input device, first-seen over the
/// resolved input order — the same order the engine assigns cpal indices) to
/// the output device id(s) that index's runtime feeds.
pub(crate) fn output_devices_by_input_cpal(
    chain: &Chain,
    registry: &[IoBinding],
    logical_inputs: &[InputEntry],
) -> Vec<Vec<String>> {
    let mut device_to_cpal: HashMap<String, usize> = HashMap::new();
    for lin in logical_inputs {
        let next = device_to_cpal.len();
        device_to_cpal
            .entry(lin.device_id.0.clone())
            .or_insert(next);
    }
    let mut by_cpal: Vec<Vec<String>> = vec![Vec::new(); device_to_cpal.len()];
    for group in resolve_chain_io_by_binding(chain, registry) {
        let out_devs: Vec<String> = group
            .outputs
            .iter()
            .map(|o| o.device_id.0.clone())
            .collect();
        for inp in &group.inputs {
            if let Some(&ci) = device_to_cpal.get(&inp.device_id.0) {
                for d in &out_devs {
                    if !by_cpal[ci].contains(d) {
                        by_cpal[ci].push(d.clone());
                    }
                }
            }
        }
    }

    // #85: a mid `Output` block writes from THIS chain's own runtime to another
    // E/S — usually another interface, which contributes no input here, so the
    // binding-group pass above never reaches it. Without its device listed, that
    // device's stream drops this runtime and the tap is silent. This does not
    // widen isolation: the device is added to THIS chain's input streams only,
    // because this chain's runtime is the one that writes the tap.
    let ports = resolve_chain_ports(chain, registry);
    let mid_output_devices: Vec<String> = ports
        .iter()
        .filter(|p| p.from_block && p.direction == PortDirection::Output)
        .map(|p| p.endpoint.device_id.0.clone())
        .collect();
    for devices in by_cpal.iter_mut() {
        for device in &mid_output_devices {
            if !devices.contains(device) {
                devices.push(device.clone());
            }
        }
    }

    // #85 (the mirror case): a mid `Input` brings its own E/S into the chain and
    // its signal flows on to the chain's TAIL. Its binding usually has no output
    // at all, so the group pass leaves that stream mapped to nothing and the
    // tail device drops the runtime the port feeds — the port is inaudible. The
    // #716 rule this must not break is about HEAD inputs (a TEYUN in never exits
    // a SCARLET out); a mid input is INSIDE this chain, so the chain's own tail
    // is exactly where its signal belongs.
    let tail_devices: Vec<String> = ports
        .iter()
        .filter(|p| !p.from_block && p.direction == PortDirection::Output)
        .map(|p| p.endpoint.device_id.0.clone())
        .collect();
    for port in ports
        .iter()
        .filter(|p| p.from_block && p.direction == PortDirection::Input)
    {
        if let Some(&ci) = device_to_cpal.get(&port.endpoint.device_id.0) {
            for device in &tail_devices {
                if !by_cpal[ci].contains(device) {
                    by_cpal[ci].push(device.clone());
                }
            }
        }
    }
    by_cpal
}
