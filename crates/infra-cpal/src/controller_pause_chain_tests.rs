//! Issue #522 — fast-path for `Command::ToggleChainEnabled`.
//!
//! The legacy behaviour: `upsert_chain` with `chain.enabled = false`
//! calls `remove_chain`, which drops the `Arc<ChainRuntimeState>` and
//! tears the CPAL input/output streams down. Re-enabling the chain then
//! has to walk the full rebuild path: device validation, NAM model
//! loads, segment + route reassembly, fresh CPAL streams. End-to-end
//! that is the ~1-second hitch the user observes on every chain
//! enable/disable toggle.
//!
//! The fast path swaps `remove_chain` for `pause_chain`: the runtime
//! stays in `active_chains`, the CPAL streams stay open, and the audio
//! callbacks short-circuit on `is_draining()` to emit silence with zero
//! processor work. Re-enabling resumes the same Arc by clearing the
//! draining flag — no rebuild, no NAM reload, no CPAL touch.

use std::sync::Arc;

use domain::ids::ChainId;
use project::chain::Chain;
use project::project::Project;

use super::active_runtime::ActiveChainRuntime;
use super::resolved::ChainStreamSignature;
use super::ProjectRuntimeController;

fn empty_chain(id: &str, enabled: bool) -> Chain {
    Chain {
        id: ChainId(id.into()),
        description: None,
        instrument: "electric_guitar".to_string(),
        enabled,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks: vec![],
        di_output: None,
    }
}

fn empty_project() -> Project {
    Project {
        name: None,
        device_settings: vec![],
        chains: vec![],
        midi: None,
    }
}

fn controller_with_active_chain(
    chain_id: &ChainId,
) -> (
    ProjectRuntimeController,
    Arc<engine::runtime::ChainRuntimeState>,
) {
    let chain = empty_chain(&chain_id.0, true);
    let runtime_arc = Arc::new(
        engine::runtime::build_chain_runtime_state(&chain, 48_000.0, &[1024], &[])
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
        ActiveChainRuntime {
            stream_signature: ChainStreamSignature {
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

    let controller = ProjectRuntimeController {
        runtime_graph: graph,
        active_chains,
        chain_slots: std::collections::HashMap::new(),
        worker: crate::ControlWorker::new(),
        pending_rebuilds: Vec::new(),
        pending_activations: Vec::new(),
        sample_rate: 48_000,
        io_bindings: Vec::new(),
        di_streams: std::cell::RefCell::new(std::collections::HashMap::new()),
        di_playback_cells: std::cell::RefCell::new(std::collections::HashMap::new()),
        di_retired: Default::default(),
            metronome_stream: std::cell::RefCell::new(None),
            metronome_shared: std::sync::Arc::new(
                engine::metronome_state::MetronomeShared::new(Default::default()),
            ),
        #[cfg(all(target_os = "linux", feature = "jack"))]
        supervisor: super::jack_supervisor::JackSupervisor::new(
            super::jack_supervisor::LiveJackBackend::new(),
        ),
    };
    (controller, runtime_arc)
}

#[test]
#[cfg(not(all(target_os = "linux", feature = "jack")))]
fn upsert_chain_disabled_pauses_runtime_without_removing() {
    let chain_id = ChainId("chain:522:pause".into());
    let (mut controller, runtime_arc) = controller_with_active_chain(&chain_id);
    let project = empty_project();

    assert!(controller.active_chains.contains_key(&chain_id));
    assert!(!runtime_arc.is_draining());

    let disabled = empty_chain(&chain_id.0, false);
    controller
        .upsert_chain(&project, &disabled)
        .expect("upsert with enabled=false must succeed");

    assert!(
        controller.active_chains.contains_key(&chain_id),
        "disabling a chain must NOT drop its runtime/streams — device stays \
         open so the next enable can resume the same `Arc<ChainRuntimeState>` \
         without rebuilding (issue #522)"
    );
    assert!(
        runtime_arc.is_draining(),
        "disabled chain must be paused via set_draining so the audio \
         callbacks emit silence with zero processor work"
    );
}

#[test]
#[cfg(not(all(target_os = "linux", feature = "jack")))]
fn upsert_chain_enabled_resumes_paused_runtime_without_rebuilding() {
    let chain_id = ChainId("chain:522:resume".into());
    let (mut controller, runtime_arc) = controller_with_active_chain(&chain_id);
    let project = empty_project();

    // Pause it first.
    let disabled = empty_chain(&chain_id.0, false);
    controller
        .upsert_chain(&project, &disabled)
        .expect("disable succeeds");
    assert!(runtime_arc.is_draining());

    // Now re-enable. The runtime Arc must be reused (same pointer) and
    // the draining flag must clear so the audio callbacks resume.
    let runtime_addr_before = Arc::as_ptr(&runtime_arc) as usize;
    let enabled = empty_chain(&chain_id.0, true);
    controller
        .upsert_chain(&project, &enabled)
        .expect("re-enable must succeed without rebuild");

    let stored = controller
        .runtime_graph
        .chains
        .get(&(chain_id.clone(), 0))
        .expect("runtime stays under the same key on resume")
        .clone();
    assert_eq!(
        Arc::as_ptr(&stored) as usize,
        runtime_addr_before,
        "re-enable must reuse the SAME ChainRuntimeState Arc — rebuilding \
         would reload every NAM model and rebuild every block processor"
    );
    assert!(
        !runtime_arc.is_draining(),
        "resume must clear set_draining so the audio thread processes again"
    );
}

/// Issue #545 — `pause_chain` / fast-path resume both call
/// `runtime_for_chain`, which only returns the FIRST runtime of a
/// chain (see the comment in `runtime_graph::runtime_for_chain`:
/// "Multi-input fan-out for these call sites is Phase 3 (#350)"). On
/// a chain with multiple input groups (one per physical input
/// device), only group 0 actually flips — the other groups keep
/// processing, which is why the user observes the tap/meter still
/// moving and CPU staying at the running-chain baseline after
/// toggling the chain off.
///
/// Two paired contracts are pinned here: pause must drain every
/// runtime, and the resume fast-path must clear draining on every
/// runtime. Otherwise either edge leaves some groups out of sync.
#[test]
#[cfg(not(all(target_os = "linux", feature = "jack")))]
fn pause_chain_drains_every_input_group_runtime() {
    let chain_id = ChainId("chain:545:multi-input".into());
    let (mut controller, group0) = controller_with_active_chain(&chain_id);

    // Add a second runtime under group 1 — same chain, second physical
    // input device. Mirrors what the runtime_graph holds when a chain
    // has two `InputBlock` entries.
    let chain = empty_chain(&chain_id.0, true);
    let group1 = Arc::new(
        engine::runtime::build_chain_runtime_state(&chain, 48_000.0, &[1024], &[])
            .expect("group-1 runtime should build"),
    );
    controller
        .runtime_graph
        .chains
        .insert((chain_id.clone(), 1), Arc::clone(&group1));

    assert!(!group0.is_draining());
    assert!(!group1.is_draining());

    controller.pause_chain(&chain_id);

    assert!(
        group0.is_draining(),
        "pause_chain must drain group 0 (currently passes — it is the only \
         runtime `runtime_for_chain` returns)"
    );
    assert!(
        group1.is_draining(),
        "REGRESSION: pause_chain failed to drain group 1 runtime — only the \
         first input group is touched. The user observes the chain looking \
         alive (tap moving, CPU not dropping) because the other input \
         groups keep processing."
    );
}

/// Issue #545 — symmetric counterpart of the pause test. After the
/// pause fix lands, re-enabling the chain must also clear draining on
/// every group, not just the first. Otherwise the cab path on the
/// second physical input stays muted after toggle-on, even though the
/// engine ran the resume.
#[test]
#[cfg(not(all(target_os = "linux", feature = "jack")))]
fn upsert_chain_enabled_resumes_every_input_group_runtime() {
    let chain_id = ChainId("chain:545:multi-input:resume".into());
    let (mut controller, group0) = controller_with_active_chain(&chain_id);

    let chain = empty_chain(&chain_id.0, true);
    let group1 = Arc::new(
        engine::runtime::build_chain_runtime_state(&chain, 48_000.0, &[1024], &[])
            .expect("group-1 runtime should build"),
    );
    controller
        .runtime_graph
        .chains
        .insert((chain_id.clone(), 1), Arc::clone(&group1));

    // Pause both groups first (the fixed pause_chain does the fan-out).
    controller.pause_chain(&chain_id);
    assert!(group0.is_draining());
    assert!(group1.is_draining());

    // Now re-enable via the same fast-path the controller takes on
    // `Command::ToggleChainEnabled { enabled: true }`.
    let project = empty_project();
    let enabled = empty_chain(&chain_id.0, true);
    controller
        .upsert_chain(&project, &enabled)
        .expect("re-enable must succeed");

    assert!(
        !group0.is_draining(),
        "resume must clear draining on group 0"
    );
    assert!(
        !group1.is_draining(),
        "REGRESSION: resume only cleared group 0; group 1 stayed draining \
         and its audio stays silent after toggle-on."
    );
}

// ── Issue #670: per-chain xrun count accessor for the GUI overload meter ──

#[test]
#[cfg(not(all(target_os = "linux", feature = "jack")))]
fn chain_xrun_count_reports_runtime_overruns() {
    let chain_id = ChainId("chain:670:xrun".into());
    let (controller, runtime_arc) = controller_with_active_chain(&chain_id);
    assert_eq!(controller.chain_xrun_count(&chain_id), 0);
    // Two overrunning callbacks (2 ms each against a 1 ms deadline).
    runtime_arc.record_callback_load(2_000_000, 1_000_000);
    runtime_arc.record_callback_load(2_000_000, 1_000_000);
    assert_eq!(controller.chain_xrun_count(&chain_id), 2);
}

#[test]
#[cfg(not(all(target_os = "linux", feature = "jack")))]
fn chain_xrun_count_is_zero_for_unknown_chain() {
    let chain_id = ChainId("chain:670:known".into());
    let (controller, _rt) = controller_with_active_chain(&chain_id);
    assert_eq!(
        controller.chain_xrun_count(&ChainId("nope".into())),
        0,
        "unknown chain has no runtime, so no xruns"
    );
}
