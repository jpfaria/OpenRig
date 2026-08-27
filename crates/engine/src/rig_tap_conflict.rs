//! Responsibility: says when two rig inputs would fight over the same tap.

use domain::io_binding::IoBinding;
use project::rig::{RigInput, RigProject};
use std::collections::BTreeSet;

/// The `(device, channel)` capture taps an input occupies. Two inputs are
/// in conflict iff their tap sets intersect — they would read the same
/// physical capture point, which two isolated runtimes must never share
/// (invariant #4).
pub(crate) fn input_taps(input: &RigInput, registry: &[IoBinding]) -> Vec<(String, usize)> {
    let mut taps = Vec::new();
    let push_binding = |io: &str, ep_name: &str, taps: &mut Vec<(String, usize)>| {
        let Some(binding) = registry.iter().find(|b| b.id == io) else {
            return;
        };
        for ep in &binding.inputs {
            if ep_name.is_empty() || ep.name == ep_name {
                for &ch in &ep.channels {
                    taps.push((ep.device_id.0.clone(), ch));
                }
            }
        }
    };
    // Checklist selection: every input endpoint of every selected binding.
    for binding_id in &input.io_binding_ids {
        push_binding(binding_id, "", &mut taps);
    }
    // Single per-input binding reference (legacy-ish io/endpoint).
    if !input.io.is_empty() {
        push_binding(&input.io, &input.endpoint, &mut taps);
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
