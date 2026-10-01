//! Responsibility: drops the checklist refs a rig input's bindings no longer offer.
//!
//! #328 (spec §1.3): a ref to an endpoint the E/S no longer offers is ignored
//! at runtime and dropped on the next save. The save captures the chains into
//! the rig, then this runs over the rig it writes.

use domain::io_binding::IoBinding;

use crate::endpoint_candidates::endpoint_candidates;
use crate::rig::RigProject;

/// Keep, on every rig input, only the unchecked refs its own bindings still
/// offer in `registry`.
pub fn prune_stale_endpoint_disables(rig: &mut RigProject, registry: &[IoBinding]) {
    for input in rig.inputs.values_mut() {
        let (inputs, outputs) = endpoint_candidates(&input.io_binding_ids, registry);
        input.disabled_endpoints.retain_known(&inputs, &outputs);
    }
}
