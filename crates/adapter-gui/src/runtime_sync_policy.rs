//! Issue #670 — runtime-sync policy for EXTERNAL events (MCP tools, MIDI
//! footswitch). The drain loop used to call `sync_live_chain_runtime` (a full
//! chain upsert, including live CoreAudio device queries) for EVERY event
//! naming a chain. For runtime-only state — the DI loop, which is applied as
//! a wait-free pointer swap — that rebuild is pure damage: captured live, a
//! 64-underrun burst landed exactly on the DI-on upsert (the owner's "ligo o
//! DI e vira um desastre", also every footswitch press bound to DI play).
//!
//! Only events that change the chain GRAPH (blocks, params, enablement, IO)
//! rebuild; runtime-only events are applied by their own dedicated handlers.

use application::event::Event;

/// Does this event change the chain graph (and therefore require
/// `sync_live_chain_runtime`)? Runtime-only events return `false`.
pub(crate) fn event_requires_runtime_sync(event: &Event) -> bool {
    !matches!(
        event,
        Event::ChainDiLoopEnabledChanged { .. } | Event::ChainDiLoopSourceChanged { .. }
    )
}
