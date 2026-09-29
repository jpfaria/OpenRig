//! Responsibility: picks the tail routes an insert's return writes.
//!
//! #979 — while an insert cuts the chain, ONE return pipeline feeds the
//! chain's tail. The tail routes are the outputs of every E/S the chain
//! selects, and two E/S often end on the same physical output: the owner's
//! two guitars both play Main `[0,1]`, so the chain has one route there per
//! guitar. With the insert off that is right — each guitar is its own
//! pipeline on its own route, and the device sums the two. The return is one
//! pipeline carrying one signal, and a pipeline pairs with each physical
//! output once (#85: a stream is one input × one output). Writing every tail
//! route played the return once per E/S on the shared output: +6.02 dB with
//! the two routes in step, a comb one buffer (1.45 ms) wide when they rested
//! a buffer apart — the "stacked" sound.
//!
//! The return writes the first tail route on each physical output; the other
//! E/S's route there is written by nothing in this state, so the runtime
//! never builds it (#947). No route is built and then skipped or mixed down.

use crate::endpoint_entry::OutputEntry;

/// The routes of `tail_routes` (indices into `outputs`) that the return
/// writes: the first one on each physical output — the same device and the
/// same channels — in route order.
pub(crate) fn return_tail_routes(tail_routes: &[usize], outputs: &[OutputEntry]) -> Vec<usize> {
    tail_routes
        .iter()
        .enumerate()
        .filter(|&(position, &route)| {
            let Some(output) = outputs.get(route) else {
                return true;
            };
            !tail_routes[..position].iter().any(|&earlier| {
                outputs
                    .get(earlier)
                    .is_some_and(|seen| same_physical_output(seen, output))
            })
        })
        .map(|(_, &route)| route)
        .collect()
}

/// Two output entries land on the same physical output: the same channels of
/// the same device.
fn same_physical_output(a: &OutputEntry, b: &OutputEntry) -> bool {
    a.device_id == b.device_id && a.channels == b.channels
}
