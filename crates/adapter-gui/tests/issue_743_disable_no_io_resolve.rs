//! Issue #743 — disabling a chain must PAUSE without resolving device IO.
//!
//! The owner's live failure: toggling a four-stream, two-interface rig OFF
//! produced a ~750 ms GUI stall, a flood of underruns, and "microfonia absurda"
//! (a loud feedback howl). Root cause: `sync_live_chain_runtime` called
//! `chain_io_changed` on EVERY toggle — including disable — and that runs a
//! synchronous `resolve_chain_audio_config` (a CoreAudio device query that costs
//! hundreds of ms per device). For four devices that is the ~750 ms stall, and
//! it runs while the chain is still live, so the output streams starve and emit
//! repeated stale frames at full level = the howl. The pause (drain → silence)
//! only happened AFTER that stall.
//!
//! A disable never needs the IO-change check (that exists to detect a re-bind on
//! an ENABLE). The lifecycle planner must pause a disabled chain immediately,
//! without touching the device resolve. Deterministic, hardware-free: the
//! resolve is modelled by a spy closure that must NOT be invoked on disable.

use std::cell::Cell;

use adapter_gui::{plan_live_sync, LiveSyncAction};

#[test]
fn disabling_a_chain_pauses_without_resolving_device_io() {
    let resolved = Cell::new(false);
    let action = plan_live_sync(true, false, || {
        resolved.set(true);
        Ok(true)
    })
    .expect("planning a disable must not error");

    assert!(
        matches!(action, LiveSyncAction::Pause),
        "disabling a present chain must plan a Pause"
    );
    assert!(
        !resolved.get(),
        "BUG #743: disabling a chain resolved device IO — the multi-device \
         CoreAudio resolve stalls the GUI ~750ms while the live output starves \
         into a feedback howl. A disable must pause immediately, no resolve."
    );
}

#[test]
fn enabling_a_chain_consults_the_io_topology() {
    let resolved = Cell::new(false);
    let _action = plan_live_sync(true, true, || {
        resolved.set(true);
        Ok(false)
    })
    .expect("planning an enable must not error");

    assert!(
        resolved.get(),
        "enabling a chain must check whether its IO topology changed \
         (re-bind detection) — only the disable path skips the resolve"
    );
}

#[test]
fn an_absent_chain_is_removed_without_resolving() {
    let resolved = Cell::new(false);
    let action = plan_live_sync(false, false, || {
        resolved.set(true);
        Ok(true)
    })
    .expect("planning a removal must not error");

    assert!(matches!(action, LiveSyncAction::Remove));
    assert!(
        !resolved.get(),
        "removing an absent chain must not resolve device IO either"
    );
}
