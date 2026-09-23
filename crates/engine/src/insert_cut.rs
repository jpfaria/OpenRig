//! Responsibility: decides whether an insert block cuts its chain.
//!
//! Issue #967 — the ONE rule every layer asks. The engine's segments and
//! endpoint shims, the streams infra-cpal opens, the signatures a live edit is
//! compared against, the stream labels and the toggle path all have to agree on
//! where a chain is cut; when one of them answered differently, the chain
//! streamed without a return it had a segment for, or every edit read as a
//! re-bind and reopened every stream.
//!
//! On cpal every BOUND insert cuts the chain, enabled or not: its send and
//! return streams stay open, and switching it off bypasses the loop in the DSP
//! (`insert_bridge`), which is what makes the footswitch instant. The JACK
//! backend drives one input and one output route per chain, so it cannot run a
//! cut it did not open; there a disabled insert keeps the pre-#967 behaviour —
//! no cut, the chain plays straight through — and a toggle rebuilds.

use domain::io_binding::IoBinding;
use project::block::AudioBlock;

use crate::insert_endpoints::insert_is_bound;

/// Can an insert be switched on and off in place, with no rebuild? `true` on
/// every cpal platform; `false` on linux+JACK (see the module doc).
pub const INSERT_TOGGLE_IS_LIVE: bool = !cfg!(all(target_os = "linux", feature = "jack"));

/// Does `block` cut its chain into a send segment and a return segment?
pub fn insert_cuts_chain(block: &AudioBlock, registry: &[IoBinding]) -> bool {
    insert_is_bound(&block.kind, registry) && (INSERT_TOGGLE_IS_LIVE || block.enabled)
}

#[cfg(test)]
#[path = "insert_cut_tests.rs"]
mod tests;
