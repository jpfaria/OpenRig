//! Responsibility: says when two rig inputs would fight over the same tap.

use domain::io_binding::IoBinding;
use project::endpoint_disables::{EndpointNode, EndpointRef};
use project::rig::{RigInput, RigProject};
use std::collections::BTreeSet;

/// The `(device, channel)` capture taps an input occupies. Two inputs are
/// in conflict iff their tap sets intersect — they would read the same
/// physical capture point, which two isolated runtimes must never share
/// (invariant #4).
pub(crate) fn input_taps(input: &RigInput, registry: &[IoBinding]) -> Vec<(String, usize)> {
    let mut taps = Vec::new();
    // Checklist selection: every input endpoint of every selected binding —
    // #328: except the ones the chain graph's input node leaves unchecked.
    // They open no stream, so they claim no tap; the chain-side detectors
    // skip them through `resolve_chain_ports` and all three must agree (#924).
    for binding_id in &input.io_binding_ids {
        let Some(binding) = registry.iter().find(|b| &b.id == binding_id) else {
            continue;
        };
        for ep in &binding.inputs {
            let endpoint = EndpointRef {
                io: binding.id.clone(),
                endpoint: ep.name.clone(),
            };
            if !input
                .disabled_endpoints
                .is_enabled(EndpointNode::Input, &endpoint)
            {
                continue;
            }
            for &ch in &ep.channels {
                taps.push((ep.device_id.0.clone(), ch));
            }
        }
    }
    // Single per-input binding reference (legacy-ish io/endpoint).
    if !input.io.is_empty() {
        if let Some(binding) = registry.iter().find(|b| b.id == input.io) {
            for ep in &binding.inputs {
                if input.endpoint.is_empty() || ep.name == input.endpoint {
                    for &ch in &ep.channels {
                        taps.push((ep.device_id.0.clone(), ch));
                    }
                }
            }
        }
    }
    taps
}

/// First tap of `candidate` already claimed by a currently-enabled input,
/// if any: `(device, channel, holder-input-name)`. Deterministic via the
/// project's `BTreeMap` ordering.
pub(crate) fn tap_conflict(
    project: &RigProject,
    enabled: &BTreeSet<String>,
    candidate: &RigInput,
    registry: &[IoBinding],
) -> Option<(String, usize, String)> {
    let want = input_taps(candidate, registry);
    for name in enabled {
        if let Some(other) = project.inputs.get(name) {
            for (dev, ch) in input_taps(other, registry) {
                if want.iter().any(|(d, c)| *d == dev && *c == ch) {
                    return Some((dev, ch, name.clone()));
                }
            }
        }
    }
    None
}
