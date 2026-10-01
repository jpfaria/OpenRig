//! Responsibility: lists the endpoints a chain's own bindings offer.
//!
//! #328 (spec §5.3): what the graph's input and output checklists show — every
//! input and every output endpoint of the E/S bindings the chain selects,
//! unchecked ones included — and what a stale checklist entry is pruned
//! against (`EndpointDisables::retain_known`).

use domain::io_binding::IoBinding;

use crate::endpoint_disables::EndpointRef;

/// `(inputs, outputs)` of the bindings in `io_binding_ids`, in selection order
/// then binding order. A selected binding missing from `registry` offers
/// nothing.
pub fn endpoint_candidates(
    io_binding_ids: &[String],
    registry: &[IoBinding],
) -> (Vec<EndpointRef>, Vec<EndpointRef>) {
    let mut inputs = Vec::new();
    let mut outputs = Vec::new();
    for binding_id in io_binding_ids {
        let Some(binding) = registry.iter().find(|b| &b.id == binding_id) else {
            continue;
        };
        let refer = |name: &str| EndpointRef {
            io: binding.id.clone(),
            endpoint: name.to_string(),
        };
        inputs.extend(binding.inputs.iter().map(|ep| refer(&ep.name)));
        outputs.extend(binding.outputs.iter().map(|ep| refer(&ep.name)));
    }
    (inputs, outputs)
}
