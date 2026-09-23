//! Responsibility: decides what an insert block contributes to its chain's topology.
//!
//! Issue #967 — the rule every layer asks, split in two because streams and
//! DSP answer different questions:
//!
//! * [`insert_owns_streams`]: a BOUND insert (both sides of its E/S resolve)
//!   owns a send and a return stream whether it is enabled or not. The streams
//!   infra-cpal opens, the stream signatures a live edit is compared against,
//!   and the engine's endpoint shims (route and cpal indices) all follow this,
//!   so switching the insert never opens or closes a stream. When they followed
//!   the enable flag, a one-bit flip read as a re-bind and every stream of the
//!   chain was closed and reopened — 2.1 s (off) and 3.0 s (on) of silence on
//!   the owner's rig.
//! * [`insert_cuts_chain`]: only an ENABLED bound insert cuts the chain's DSP
//!   into a send segment and a return segment. A disabled one is exactly what
//!   it always was — the chain plays straight through it, every head paired
//!   with its own E/S's outputs. Switching it is a DSP rebuild off the audio
//!   thread; its streams stay as they are (a disabled insert's send route is
//!   never written, so its send stream carries silence, and nothing reads its
//!   return).

use domain::io_binding::IoBinding;
use project::block::AudioBlock;

use crate::insert_endpoints::insert_is_bound;

/// Does `block` own a send and a return stream of its chain?
pub fn insert_owns_streams(block: &AudioBlock, registry: &[IoBinding]) -> bool {
    insert_is_bound(&block.kind, registry)
}

/// Does `block` cut its chain's DSP into a send segment and a return segment?
pub fn insert_cuts_chain(block: &AudioBlock, registry: &[IoBinding]) -> bool {
    block.enabled && insert_owns_streams(block, registry)
}

#[cfg(test)]
#[path = "insert_cut_tests.rs"]
mod tests;
