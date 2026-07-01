//! Issue #614 — RED-FIRST test: compact chain view DI loop wiring must expose
//! a `wire_compact_chain_di_loop` function that connects play/stop/source-
//! selected/choose-file to the same underlying handlers used by the chains
//! tile wiring.
//!
//! The test drives the same `play_chain_di_loop` / `stop_chain_di_loop` paths
//! the chain-row wiring calls, but through the compact-view entry point.
//! Until `compact_chain_callbacks::wire_compact_di_loop` (or equivalent)
//! exists and delegates correctly, the build itself will fail — that IS the
//! red signal.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use application::command::Command;
use application::di_loader::DiLoopSource;
use application::dispatcher::CommandDispatcher;
use application::local_dispatcher::LocalDispatcher;
use domain::ids::ChainId;
use engine::runtime::{build_chain_runtime_state, RuntimeGraph, DEFAULT_ELASTIC_TARGET};
use infra_cpal::ProjectRuntimeController;
use project::chain::Chain;
use project::project::Project;
use std::rc::Rc;

// ── helpers ──────────────────────────────────────────────────────────────────

fn write_mono_wav(path: &Path, sr: u32, samples: &[f32]) {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: sr,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut w = hound::WavWriter::create(path, spec).expect("WavWriter::create");
    for &s in samples {
        w.write_sample(s).expect("write_sample");
    }
    w.finalize().expect("finalize");
}

fn make_project(chain_id: &str) -> Rc<RefCell<Project>> {
    Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains: vec![Chain {
            id: ChainId(chain_id.to_string()),
            description: None,
            instrument: "electric_guitar".to_string(),
            enabled: true,
            volume: 100.0,
            io_binding_ids: vec![],
            blocks: vec![],
            di_output: None,
        }],
        midi: None,
    }))
}

fn make_controller(chain_id: &ChainId) -> ProjectRuntimeController {
    let chain = Chain {
        id: chain_id.clone(),
        description: None,
        instrument: "electric_guitar".to_string(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks: vec![],
        di_output: None,
    };
    let runtime_arc = Arc::new(
        build_chain_runtime_state(&chain, 48_000.0, &[DEFAULT_ELASTIC_TARGET], &[])
            .expect("build_chain_runtime_state"),
    );
    let mut chains = HashMap::new();
    chains.insert((chain_id.clone(), 0), runtime_arc);
    ProjectRuntimeController::for_testing(RuntimeGraph { chains })
}

// ── compact view play arms the same runtime ──────────────────────────────────

/// `compact_chain_di_loop_play` must arm the chain runtime, exactly as the
/// chains-tile `play_chain_di_loop` does.  The compact view is the trigger
/// point — the focused chain must be correctly identified and armed.
#[test]
fn compact_di_loop_play_arms_focused_chain_runtime() {
    let dir = tempfile::tempdir().expect("tempdir");
    let wav = dir.path().join("compact_di.wav");
    let samples: Vec<f32> = (0..128).map(|i| i as f32 / 127.0).collect();
    write_mono_wav(&wav, 48_000, &samples);

    let chain_id = ChainId("chain_614_compact_play".to_string());
    let project = make_project(&chain_id.0);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    let controller = RefCell::new(Some(make_controller(&chain_id)));

    // Load a source via the dispatcher.
    dispatcher
        .dispatch(Command::SetChainDiLoopSource {
            chain: chain_id.clone(),
            source: DiLoopSource::File(wav),
        })
        .expect("SetChainDiLoopSource must succeed");
    // #693: the decode runs on its own task — wait for it to land
    // (poll_async_results is the frontend tick's job).
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while session_dispatcher_di_loaded(&dispatcher, &chain_id).is_none()
        && std::time::Instant::now() < deadline
    {
        let _ = dispatcher.poll_async_results();
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    // Precondition: runtime not armed.
    assert!(
        !controller
            .borrow()
            .as_ref()
            .unwrap()
            .chain_has_di_loop(&chain_id),
        "precondition: di loop must not be armed before compact play"
    );

    // Call the compact-view play entry point — this is the new public symbol.
    adapter_gui::compact_chain_callbacks::compact_chain_di_loop_play(
        &controller,
        &dispatcher,
        &chain_id,
    );

    // The same runtime must now be armed.
    assert!(
        controller
            .borrow()
            .as_ref()
            .unwrap()
            .chain_has_di_loop(&chain_id),
        "REGRESSION #614 compact: compact_chain_di_loop_play did not arm the \
         runtime — chain_has_di_loop() is still false"
    );
    // #717: play must ALSO arm the dedicated DI stream, so its worker drives the
    // isolated runtime and the DI graph gets its own live meters.
    assert!(
        controller
            .borrow()
            .as_ref()
            .unwrap()
            .di_stream_active(&chain_id),
        "#717: compact play must arm the dedicated DI stream (di_stream_active)"
    );
}

/// `compact_chain_di_loop_stop` must disarm the chain runtime.
#[test]
fn compact_di_loop_stop_disarms_focused_chain_runtime() {
    let dir = tempfile::tempdir().expect("tempdir");
    let wav = dir.path().join("compact_di_stop.wav");
    write_mono_wav(&wav, 48_000, &[0.5f32; 64]);

    let chain_id = ChainId("chain_614_compact_stop".to_string());
    let project = make_project(&chain_id.0);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    let controller = RefCell::new(Some(make_controller(&chain_id)));

    // Load + play to arm the runtime.
    dispatcher
        .dispatch(Command::SetChainDiLoopSource {
            chain: chain_id.clone(),
            source: DiLoopSource::File(wav),
        })
        .expect("SetChainDiLoopSource");
    // #693: wait for the off-thread decode to land before playing.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while session_dispatcher_di_loaded(&dispatcher, &chain_id).is_none()
        && std::time::Instant::now() < deadline
    {
        let _ = dispatcher.poll_async_results();
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    adapter_gui::compact_chain_callbacks::compact_chain_di_loop_play(
        &controller,
        &dispatcher,
        &chain_id,
    );

    assert!(
        controller
            .borrow()
            .as_ref()
            .unwrap()
            .chain_has_di_loop(&chain_id),
        "precondition: di loop must be armed after play"
    );

    // Stop via the compact entry point.
    adapter_gui::compact_chain_callbacks::compact_chain_di_loop_stop(
        &controller,
        &dispatcher,
        &chain_id,
    );

    assert!(
        !controller
            .borrow()
            .as_ref()
            .unwrap()
            .chain_has_di_loop(&chain_id),
        "REGRESSION #614 compact: compact_chain_di_loop_stop did not disarm \
         the runtime — chain_has_di_loop() is still true"
    );
}

/// #693 helper: the decoded loop for `chain`, if the off-thread decode landed.
fn session_dispatcher_di_loaded(
    dispatcher: &application::local_dispatcher::LocalDispatcher,
    chain: &domain::ids::ChainId,
) -> Option<std::sync::Arc<engine::DiPcm>> {
    dispatcher.di_loop_for_chain(chain)
}
