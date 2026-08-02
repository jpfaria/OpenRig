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

/// #85 — the user changed the sample rate in the audio settings and the rig
/// went silent until the project was reopened. `AudioSettingsSaved` names no
/// chain, so the per-chain loop above skipped it and NOTHING re-synced: the
/// streams kept running against the old device config. A rate/buffer change is
/// a rebuild of every chain, not a stop.
#[test]
fn saving_the_audio_settings_rebuilds_every_chain() {
    assert!(
        super::runtime_sync_policy::events_require_full_project_sync(&[Event::AudioSettingsSaved]),
        "#85: after a device rate/buffer change the whole project must be re-synced"
    );
}

/// It must stay a rebuild of the WHOLE project only for that: a DI toggle or a
/// per-chain edit keeps the targeted path.
#[test]
fn ordinary_events_do_not_force_a_full_project_sync() {
    assert!(
        !super::runtime_sync_policy::events_require_full_project_sync(&[
            Event::ChainDiLoopEnabledChanged {
                chain: chain(),
                enabled: true,
            },
        ])
    );
}
