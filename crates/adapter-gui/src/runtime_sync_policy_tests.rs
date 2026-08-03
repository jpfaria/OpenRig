//! Issue #670 — which external events (MCP / MIDI footswitch) require a
//! chain-runtime REBUILD? Reproduced on the live app: every chain-naming
//! event triggered sync_live_chain_runtime (a full upsert with live USB
//! device queries) — so turning the DI loop on (a wait-free runtime pointer
//! swap) rebuilt the whole chain and starved the output (owner: "ligo o DI e
//! vira um desastre"; captured live: 64-underrun burst exactly at the DI
//! upsert).

use super::runtime_sync_policy::event_requires_runtime_sync;
use application::event::Event;
use domain::ids::ChainId;

fn chain() -> ChainId {
    ChainId("rig:input-3".into())
}

#[test]
fn di_loop_events_do_not_rebuild() {
    assert!(!event_requires_runtime_sync(
        &Event::ChainDiLoopEnabledChanged {
            chain: chain(),
            enabled: true,
        }
    ));
    assert!(!event_requires_runtime_sync(
        &Event::ChainDiLoopSourceChanged { chain: chain() }
    ));
    // #771: the DI output pick is runtime-only too — the DI wiring re-arms
    // the isolated playback itself; a full chain rebuild is pure damage.
    assert!(!event_requires_runtime_sync(
        &Event::ChainDiLoopOutputChanged { chain: chain() }
    ));
}

/// #127 (Task 8): `BlockCommand::ToggleBlockEnabled` now applies the #522 LIVE
/// in-place toggle from the dispatcher itself. If the drain ALSO rebuilt on the
/// resulting event, an MCP/footswitch block toggle would pay a full chain sync
/// (device resolve + model reload) right after the cheap toggle already
/// happened — the #740 freeze, on the wrong side of the fix.
#[test]
fn a_block_enable_toggle_does_not_rebuild() {
    assert!(!event_requires_runtime_sync(&Event::BlockEnabledChanged {
        chain: chain(),
        block: domain::ids::BlockId("b".into()),
        enabled: false,
    }));
}

#[test]
fn graph_changing_events_do_rebuild() {
    assert!(event_requires_runtime_sync(&Event::ChainEnabledChanged {
        chain: chain(),
        enabled: true,
    }));
    assert!(event_requires_runtime_sync(&Event::BlockParameterChanged {
        chain: chain(),
        block: domain::ids::BlockId("b".into()),
        path: "drive".into(),
    }));
}

// #85's pair of tests for `events_require_full_project_sync` lived here. The
// rate/buffer rebuild they described is no longer a drain decision this
// frontend makes: `SettingsCommand::SaveAudioSettings` applies
// `RuntimeControl::apply_device_settings` and then `sync_project` from the
// dispatcher, so the same save over MCP or MIDI re-opens the graph too. The
// behaviour is pinned there, against a real dispatch, by
// `application::local_dispatcher_runtime_doors_tests::
// saving_the_device_settings_rebuilds_the_whole_graph` and
// `..._makes_the_driver_adopt_them_first`.
