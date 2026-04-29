//! ProjectRuntimeController regression tests.
//!
//! Pulled out of `tests.rs` to keep both files under the 600-LOC cap.
//! Covers:
//!
//! - `is_healthy` / `is_running` sanity checks on a freshly built controller.
//! - `teardown_active_chain_for_rebuild` — issue #294 (stale JACK client on
//!   chain reconfigure) and issue #316 (draining flag must be cleared so the
//!   rebuild's new CPAL/JACK callbacks don't inherit it and silence audio).
//! - `jack_config_for_card` (Linux+JACK only) — DeviceSettings override pickup
//!   vs realtime defaults (issue #308).

#![cfg(test)]

use super::ProjectRuntimeController;

#[test]
fn is_healthy_returns_true_when_no_chains_active() {
    let mut controller = ProjectRuntimeController {
        runtime_graph: engine::runtime::RuntimeGraph {
            chains: std::collections::HashMap::new(),
        },
        active_chains: std::collections::HashMap::new(),
        #[cfg(all(target_os = "linux", feature = "jack"))]
        supervisor: super::jack_supervisor::JackSupervisor::new(
            super::jack_supervisor::LiveJackBackend::new(),
        ),
    };
    assert!(controller.is_healthy());
}

#[test]
fn is_running_returns_false_when_no_chains() {
    let controller = ProjectRuntimeController {
        runtime_graph: engine::runtime::RuntimeGraph {
            chains: std::collections::HashMap::new(),
        },
        active_chains: std::collections::HashMap::new(),
        #[cfg(all(target_os = "linux", feature = "jack"))]
        supervisor: super::jack_supervisor::JackSupervisor::new(
            super::jack_supervisor::LiveJackBackend::new(),
        ),
    };
    assert!(!controller.is_running());
}

// ── Regression tests for issue #294: stale JACK client on chain reconfigure ──
//
// Reconfiguring input channels on an active chain (e.g. unchecking a channel
// in a stereo input) used to leave the previous JACK client alive while the
// replacement client was being built, because HashMap::insert only dropped
// the old ActiveChainRuntime AFTER constructing the new one. On JACK, the
// new client would get a suffixed name while connect_ports_by_name still
// used the literal (unsuffixed) name — so the connections bound to the
// OLD client's ports, which then vanished when the old client was finally
// dropped, leaving the new client orphaned and audio silent.
//
// The fix tears down the existing ActiveChainRuntime BEFORE building the
// replacement (teardown_active_chain_for_rebuild), mirroring the pattern
// in remove_chain. These tests cover the teardown helper directly; the
// end-to-end "audio still flows after channel toggle" behavior is
// verifiable only on real JACK hardware and is exercised manually on the
// Orange Pi during regression testing.

#[test]
fn teardown_active_chain_for_rebuild_drops_entry_when_present() {
    let chain_id = domain::ids::ChainId("chain:0".into());
    let mut controller = ProjectRuntimeController {
        runtime_graph: engine::runtime::RuntimeGraph {
            chains: std::collections::HashMap::new(),
        },
        active_chains: std::collections::HashMap::new(),
        #[cfg(all(target_os = "linux", feature = "jack"))]
        supervisor: super::jack_supervisor::JackSupervisor::new(
            super::jack_supervisor::LiveJackBackend::new(),
        ),
    };
    controller.active_chains.insert(
        chain_id.clone(),
        super::active_runtime::ActiveChainRuntime {
            stream_signature: super::resolved::ChainStreamSignature {
                inputs: vec![],
                outputs: vec![],
            },
            _input_streams: vec![],
            _output_streams: vec![],
            #[cfg(all(target_os = "linux", feature = "jack"))]
            _jack_client: None,
            #[cfg(all(target_os = "linux", feature = "jack"))]
            _dsp_worker: None,
        },
    );
    assert!(controller.active_chains.contains_key(&chain_id));

    controller.teardown_active_chain_for_rebuild(&chain_id);

    assert!(
        !controller.active_chains.contains_key(&chain_id),
        "active_chains entry must be removed so the old JACK client/DSP worker are dropped \
             before a replacement is built"
    );
}

#[test]
fn teardown_active_chain_for_rebuild_is_noop_when_chain_absent() {
    let chain_id = domain::ids::ChainId("chain:missing".into());
    let mut controller = ProjectRuntimeController {
        runtime_graph: engine::runtime::RuntimeGraph {
            chains: std::collections::HashMap::new(),
        },
        active_chains: std::collections::HashMap::new(),
        #[cfg(all(target_os = "linux", feature = "jack"))]
        supervisor: super::jack_supervisor::JackSupervisor::new(
            super::jack_supervisor::LiveJackBackend::new(),
        ),
    };

    controller.teardown_active_chain_for_rebuild(&chain_id);

    assert!(controller.active_chains.is_empty());
}

// ── Regression #316: teardown clears the draining flag for rebuild ──
//
// The JACK fix from #294 (this same `teardown_active_chain_for_rebuild`)
// calls `set_draining(true)` on the live `Arc<ChainRuntimeState>` so the
// audio callback bails out while the old CPAL/JACK streams are dropped.
// The Arc stays alive in `runtime_graph` because the caller is about to
// re-upsert it, and `RuntimeGraph::upsert_chain` reuses an existing
// entry instead of rebuilding the state. Without a matching reset the
// new streams' callbacks observe `is_draining()==true` from the very
// first invocation and silence every segment on the chain — including
// sibling InputEntries that were not touched by the channel edit. The
// user-visible symptom is "remove a channel from one entry → audio of
// the other entry on the same chain stops too" (issue #316). Toggling
// the chain off then on works because `remove_chain` drops the Arc, so
// the next enable rebuilds a fresh `ChainRuntimeState` with the flag
// already initialized to `false`.
#[test]
fn teardown_active_chain_for_rebuild_clears_draining_so_rebuild_can_resume_audio() {
    use std::sync::Arc;
    let chain_id = domain::ids::ChainId("chain:316".into());
    let chain = project::chain::Chain {
        id: chain_id.clone(),
        description: None,
        instrument: "electric_guitar".to_string(),
        enabled: true,
        blocks: vec![],
    };
    let runtime_arc = Arc::new(
        engine::runtime::build_chain_runtime_state(&chain, 48_000.0, &[1024])
            .expect("empty chain runtime should build"),
    );

    let mut graph = engine::runtime::RuntimeGraph {
        chains: std::collections::HashMap::new(),
    };
    graph
        .chains
        .insert(chain_id.clone(), Arc::clone(&runtime_arc));

    let mut active_chains = std::collections::HashMap::new();
    active_chains.insert(
        chain_id.clone(),
        super::active_runtime::ActiveChainRuntime {
            stream_signature: super::resolved::ChainStreamSignature {
                inputs: vec![],
                outputs: vec![],
            },
            _input_streams: vec![],
            _output_streams: vec![],
            #[cfg(all(target_os = "linux", feature = "jack"))]
            _jack_client: None,
            #[cfg(all(target_os = "linux", feature = "jack"))]
            _dsp_worker: None,
        },
    );

    let mut controller = ProjectRuntimeController {
        runtime_graph: graph,
        active_chains,
        #[cfg(all(target_os = "linux", feature = "jack"))]
        supervisor: super::jack_supervisor::JackSupervisor::new(
            super::jack_supervisor::LiveJackBackend::new(),
        ),
    };

    assert!(
        !runtime_arc.is_draining(),
        "freshly built runtime starts un-drained"
    );

    controller.teardown_active_chain_for_rebuild(&chain_id);

    assert!(
        !runtime_arc.is_draining(),
        "teardown_active_chain_for_rebuild must clear the draining flag — \
             the Arc<ChainRuntimeState> is reused by the rebuild that follows, \
             and leaving the flag set silences every CPAL/JACK callback on the \
             chain (including sibling InputEntries) until the chain is fully \
             removed and re-added (#316)"
    );
}

// ── jack_config_for_card reads DeviceSettings (#308) ─────────────────
//
// Guarded to Linux+jack because that is the only cfg the function is
// compiled for. On macOS/Windows these tests are compiled out — same
// as the function itself.

#[cfg(all(target_os = "linux", feature = "jack"))]
fn test_card(device_id: &str) -> super::usb_proc::UsbAudioCard {
    super::usb_proc::UsbAudioCard {
        card_num: "4".into(),
        server_name: "openrig_hw4".into(),
        display_name: "test card".into(),
        device_id: device_id.into(),
        capture_channels: 2,
        playback_channels: 2,
    }
}

#[cfg(all(target_os = "linux", feature = "jack"))]
fn empty_project() -> project::Project {
    project::Project {
        name: None,
        device_settings: Vec::new(),
        chains: Vec::new(),
    }
}

#[cfg(all(target_os = "linux", feature = "jack"))]
#[test]
fn jack_config_for_card_uses_device_settings_values() {
    use domain::ids::DeviceId;
    use project::device::DeviceSettings;

    let card = test_card("hw:4");
    let mut project = empty_project();
    project.device_settings.push(DeviceSettings {
        device_id: DeviceId("hw:4".into()),
        sample_rate: 48_000,
        buffer_size_frames: 64,
        bit_depth: 32,
        realtime: true,
        rt_priority: 80,
        nperiods: 2,
    });

    let config = ProjectRuntimeController::jack_config_for_card(&card, &project);

    assert!(config.realtime);
    assert_eq!(config.rt_priority, 80);
    assert_eq!(config.nperiods, 2);
    assert_eq!(config.sample_rate, 48_000);
    assert_eq!(config.buffer_size, 64);
}

#[cfg(all(target_os = "linux", feature = "jack"))]
#[test]
fn jack_config_for_card_falls_back_to_realtime_defaults_when_no_match() {
    let card = test_card("hw:4");
    // No matching device_settings — defaults are realtime + nperiods=3.
    // We ship nperiods=3 (not 2) because nperiods=2 triggered ALSA Broken
    // pipe on Q26 USB audio + RK3588 in hardware validation; the extra
    // period gives the USB driver enough slack without meaningfully
    // increasing latency (one period at 128 frames / 48kHz ≈ 2.7ms).
    let project = empty_project();

    let config = ProjectRuntimeController::jack_config_for_card(&card, &project);

    assert!(config.realtime);
    assert_eq!(config.rt_priority, 70);
    assert_eq!(config.nperiods, 3);
    assert_eq!(config.sample_rate, 48_000);
    assert_eq!(config.buffer_size, 64);
}
