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
        chain_slots: std::collections::HashMap::new(),
        worker: crate::ControlWorker::new(),
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
        chain_slots: std::collections::HashMap::new(),
        worker: crate::ControlWorker::new(),
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
        chain_slots: std::collections::HashMap::new(),
        worker: crate::ControlWorker::new(),
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
        chain_slots: std::collections::HashMap::new(),
        worker: crate::ControlWorker::new(),
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
        volume: 100.0,
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
        .insert((chain_id.clone(), 0), Arc::clone(&runtime_arc));

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
        chain_slots: std::collections::HashMap::new(),
        worker: crate::ControlWorker::new(),
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
fn empty_project() -> project::project::Project {
    project::project::Project {
        name: None,
        device_settings: Vec::new(),
        chains: Vec::new(),
        midi: None,
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

// ── Issue #350 phase 3: each physical input device wires its OWN runtime ──
//
// The user's real bug: a chain with 2 InputBlock entries on 2 DIFFERENT
// physical devices (Scarlett + TEYUN) both feeding one OutputBlock. The
// 2nd guitar was silent because cpal collapsed to the first per-input
// runtime. This is a structural assertion — no real audio device is
// touched. It proves: (a) the chain owns one isolated runtime PER
// distinct input device, keyed by a distinct cpal group id; (b) the
// group ids match the cpal input indices the input streams bind to;
// (c) the output mix helper consumes from ALL of them (multi-runtime
// path) while a single-runtime chain stays on the byte-identical path.
/// Fixture: a chain with two `InputBlock`s on two distinct physical devices,
/// both feeding one stereo `OutputBlock`. Mirrors the issue #350 bug shape.
fn two_device_chain() -> project::chain::Chain {
    use domain::ids::{BlockId, ChainId, DeviceId};
    use project::block::{
        AudioBlock, AudioBlockKind, InputBlock, InputEntry, OutputBlock, OutputEntry,
    };
    use project::chain::{Chain, ChainInputMode, ChainOutputMode};

    let input = |id: &str, dev: &str| AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".into(),
            entries: vec![InputEntry {
                device_id: DeviceId(dev.into()),
                mode: ChainInputMode::Mono,
                channels: vec![0],
            }],
        }),
    };
    Chain {
        id: ChainId("two_dev".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        blocks: vec![
            input("two_dev:in:0", "scarlett_2i2"),
            input("two_dev:in:1", "teyun_q26"),
            AudioBlock {
                id: BlockId("two_dev:out:0".into()),
                enabled: true,
                kind: AudioBlockKind::Output(OutputBlock {
                    model: "standard".into(),
                    entries: vec![OutputEntry {
                        device_id: DeviceId("scarlett_2i2".into()),
                        mode: ChainOutputMode::Stereo,
                        channels: vec![0, 1],
                    }],
                }),
            },
        ],
    }
}

#[test]
fn two_device_inputs_each_wire_their_own_runtime() {
    use engine::runtime::process_output_f32_mixed;
    use project::project::Project;
    use std::collections::HashMap;
    use std::sync::Arc;

    let chain = two_device_chain();
    let project = Project {
        name: Some("two_dev_test".into()),
        chains: vec![chain.clone()],
        device_settings: Vec::new(),
        midi: None,
    };
    let mut sample_rates = HashMap::new();
    sample_rates.insert(chain.id.clone(), 48_000.0_f32);
    let graph = engine::runtime::build_runtime_graph(&project, &sample_rates, &HashMap::new())
        .expect("two-device chain must build");

    // (a)+(b): exactly two isolated runtimes, one per distinct device,
    // keyed by distinct ascending cpal group ids 0 and 1.
    let runtimes = graph.runtimes_with_groups_for(&chain.id);
    assert_eq!(
        runtimes.len(),
        2,
        "two distinct input devices => two isolated per-input runtimes"
    );
    assert_eq!(runtimes[0].0, 0, "first device => cpal group 0");
    assert_eq!(runtimes[1].0, 1, "second device => cpal group 1");
    assert!(
        !Arc::ptr_eq(&runtimes[0].1, &runtimes[1].1),
        "each physical input device must own a SEPARATE ChainRuntimeState \
         (CLAUDE.md invariant #4 — zero shared runtime state)"
    );

    // (c): the output mix helper drives BOTH runtimes — pulling from each
    // per-input runtime's own SPSC output ring and summing at the backend
    // (the only mix point invariant #4 permits). With both runtimes idle
    // their rings are empty, so the summed device buffer is silence; the
    // assertion that matters structurally is that passing N runtimes does
    // not panic and clears the buffer (multi-runtime path), while a single
    // runtime takes the byte-identical fast path.
    let all: Vec<_> = runtimes.iter().map(|(_, rt)| rt.clone()).collect();
    let mut out = vec![0.5_f32; 128];
    let mut scratch = vec![0.0_f32; 128];
    process_output_f32_mixed(&all, 0, &mut out, 2, &mut scratch);
    assert!(
        out.iter().all(|s| s.abs() < 1.0e-6),
        "mixed output of two idle isolated runtimes must be silence"
    );

    // Single-runtime fast path: identical call shape, one runtime.
    let mut out1 = vec![0.5_f32; 128];
    let mut scratch1 = vec![0.0_f32; 128];
    process_output_f32_mixed(&all[..1], 0, &mut out1, 2, &mut scratch1);
    assert!(
        out1.iter().all(|s| s.abs() < 1.0e-6),
        "single-runtime path must behave like the legacy process_output_f32"
    );
}
