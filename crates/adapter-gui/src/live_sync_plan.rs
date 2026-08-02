//! #743: the planned action for a one-chain live sync, modelled as data so the
//! decision — crucially, WHETHER a device-IO resolve runs — is unit-testable
//! without audio hardware.
//!
//! Split out of `runtime_lifecycle` (line cap): the decision is a pure
//! function, the sequence that carries it out is the controller owner's.

use anyhow::Result;

/// What a one-chain live sync should do.
pub enum LiveSyncAction {
    /// The chain is gone from the project: drop it from the live graph.
    Remove,
    /// The chain is present but disabled: pause it (drain → silence) in O(1).
    /// No device-IO resolve — that synchronous CoreAudio query (hundreds of ms
    /// per device) would stall the GUI while the live output starves into a
    /// feedback howl (#743). A disable never re-binds, so the check is moot.
    Pause,
    /// The chain is present and enabled: (re)activate it. `io_changed` is the
    /// re-bind check — only an enable consults it.
    Enable { io_changed: bool },
}

/// Decide the live-sync action for a toggled chain. The `io_changed` closure
/// (the device-IO resolve) is invoked ONLY for an enable; a disable or a
/// removal must never touch it — that resolve is the ~750 ms CoreAudio stall
/// that starves the live output into feedback on a four-device toggle (#743).
pub fn plan_live_sync(
    chain_present: bool,
    chain_enabled: bool,
    io_changed: impl FnOnce() -> Result<bool>,
) -> Result<LiveSyncAction> {
    if !chain_present {
        return Ok(LiveSyncAction::Remove);
    }
    if !chain_enabled {
        return Ok(LiveSyncAction::Pause);
    }
    Ok(LiveSyncAction::Enable {
        io_changed: io_changed()?,
    })
}
