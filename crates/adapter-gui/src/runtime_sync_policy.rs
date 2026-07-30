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

use application::command::{ChainCommand, Command};
use application::event::Event;
use domain::ids::ChainId;

use crate::state::ProjectSession;

/// #127: ask the audio runtime to bring ONE chain back in step with the
/// project — on the bus, like every other runtime request.
///
/// The sequence itself (device resolve, off-thread rebuild, activation
/// scheduling) still belongs to `runtime_lifecycle`, the module that OWNS the
/// controller; the dispatcher reaches it through the `RuntimeControl` the
/// frontend attached. What a wiring module must NOT do is call
/// `sync_live_chain_runtime` itself: that is the split that let the GUI reach
/// the audio by one road and MCP/MIDI by another.
///
/// Scoped to the chain it names, never a group and never "all matching" — the
/// isolation invariant is carried by the argument.
pub(crate) fn request_chain_sync(session: &ProjectSession, chain: &ChainId) -> anyhow::Result<()> {
    let events = session
        .dispatcher
        .dispatch(Command::Chain(ChainCommand::SyncChainRuntime {
            chain: chain.clone(),
        }))?;
    // The GUI attaches its `RuntimeControl` at every session install, so this
    // is unreachable here; it means the sync was NOT applied and is still owed.
    if events
        .iter()
        .any(|e| matches!(e, Event::ChainRuntimeSyncNeeded { .. }))
    {
        log::warn!(
            "chain '{}' asked for a runtime sync nobody could apply — no RuntimeControl attached",
            chain.0
        );
    }
    Ok(())
}

/// Does this event change the chain graph (and therefore require
/// `sync_live_chain_runtime`)? Runtime-only events — and events whose command
/// handler already applied the runtime effect itself — return `false`.
pub(crate) fn event_requires_runtime_sync(event: &Event) -> bool {
    !matches!(
        event,
        Event::ChainDiLoopEnabledChanged { .. }
            | Event::ChainDiLoopSourceChanged { .. }
            | Event::ChainDiLoopOutputChanged { .. }
            // #127: `ToggleBlockEnabled` applies the #522 LIVE in-place toggle
            // from the dispatcher (through `RuntimeControl`). Rebuilding here
            // would add a full device resolve + model reload on top of a
            // toggle that already took effect — the #740 freeze all over
            // again, and for every footswitch press.
            | Event::BlockEnabledChanged { .. }
    )
}
