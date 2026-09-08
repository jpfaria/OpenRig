//! Responsibility: says which E/S a resolved head entry was resolved from.

use crate::runtime_endpoints::BindingIo;

/// The binding the `raw_idx`-th resolved head INPUT came from. Entries are
/// resolved binding by binding, in selection order, so the position — not the
/// capture point — identifies the E/S: two E/S may read the same channel
/// (#928) and each still owns its own entry. `None` past the head entries
/// (mid ports, insert returns).
pub(crate) fn binding_of_raw_input(by: &[BindingIo], raw_idx: usize) -> Option<&str> {
    let mut start = 0;
    for group in by {
        let end = start + group.inputs.len();
        if (start..end).contains(&raw_idx) {
            return Some(group.binding_id.as_str());
        }
        start = end;
    }
    None
}

/// The binding the `route_idx`-th resolved OUTPUT came from, by the same
/// positional rule. `None` past the tail routes (mid ports, insert sends).
pub(crate) fn binding_of_route(by: &[BindingIo], route_idx: usize) -> Option<&str> {
    let mut start = 0;
    for group in by {
        let end = start + group.outputs.len();
        if (start..end).contains(&route_idx) {
            return Some(group.binding_id.as_str());
        }
        start = end;
    }
    None
}
