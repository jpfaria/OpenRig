//! Issue #614 — end-to-end RED-FIRST test: play/stop handlers must apply to
//! the chain's audio runtime, not just dispatch the command.
//!
//! The bug: `on_di_loop_play` dispatched `SetChainDiLoopEnabled` (which emits
//! `Event::ChainDiLoopEnabledChanged`) but never called `set_chain_di_loop` on
//! the `ProjectRuntimeController`.  `di_stream_active()` therefore stayed
//! `false` and the loop never played.
//!
//! This test drives the combined `play_chain_di_loop` / `stop_chain_di_loop`
//! functions that the production callbacks call.  Until those functions exist
//! and wire the apply, `di_stream_active()` stays `false` and the test is
//! RED.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;

use application::command::{ChainCommand, Command};
use application::di_loader::DiLoopSource;
use application::dispatcher::CommandDispatcher;
use application::local_dispatcher::LocalDispatcher;
use domain::ids::ChainId;
use engine::runtime::{build_chain_runtime_state, RuntimeGraph, DEFAULT_ELASTIC_TARGET};
use infra_cpal::ProjectRuntimeController;
use project::chain::Chain;
use project::project::Project;

// ── helpers ─────────────────────────────────────────────────────────────────

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

/// Build a `ProjectRuntimeController` with a single chain runtime populated
/// but without starting live audio streams (uses `ProjectRuntimeController::
/// for_testing` — no audio devices opened).
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

// ── end-to-end: play arms the runtime ───────────────────────────────────────

/// Calling the play handler after a source is loaded must arm the chain
/// runtime.  This is the end-to-end regression test for the bug where
/// `on_di_loop_play` dispatched the command but never applied it to the
/// runtime — `di_stream_active()` stayed `false`.
#[test]
fn play_chain_di_loop_arms_runtime() {
    let dir = tempfile::tempdir().expect("tempdir");
    let wav = dir.path().join("di.wav");
    let samples: Vec<f32> = (0..256).map(|i| i as f32 / 255.0).collect();
    write_mono_wav(&wav, 48_000, &samples);

    let chain_id = ChainId("chain_614_play".to_string());
    let project = make_project(&chain_id.0);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    let controller = RefCell::new(Some(make_controller(&chain_id)));

    // Load a source into the dispatcher's ephemeral store.
    dispatcher
        .dispatch(Command::Chain(ChainCommand::SetChainDiLoopSource {
            chain: chain_id.clone(),
            source: DiLoopSource::File(wav),
        }))
        .expect("SetChainDiLoopSource must succeed");
    // #693: wait for the off-thread decode to land before playing.
    {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while dispatcher.di_loop_for_chain(&chain_id).is_none()
            && std::time::Instant::now() < deadline
        {
            let _ = dispatcher.poll_async_results();
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    // Precondition: runtime is unarmed.
    assert!(
        !controller
            .borrow()
            .as_ref()
            .unwrap()
            .di_stream_active(&chain_id),
        "precondition: di loop must not be armed before play"
    );

    // Call the combined play helper (the function the Slint callback invokes).
    adapter_gui::di_loop_wiring::play_chain_di_loop(&controller, &dispatcher, &chain_id);

    // The runtime must now be armed.
    assert!(
        controller
            .borrow()
            .as_ref()
            .unwrap()
            .di_stream_active(&chain_id),
        "REGRESSION #614: play_chain_di_loop did not arm the runtime — \
         di_stream_active() is still false after pressing Play"
    );
}

// ── end-to-end: stop disarms the runtime ────────────────────────────────────

/// Calling the stop handler must clear the chain runtime regardless of
/// whether a source is currently loaded.
#[test]
fn stop_chain_di_loop_disarms_runtime() {
    let dir = tempfile::tempdir().expect("tempdir");
    let wav = dir.path().join("di.wav");
    write_mono_wav(&wav, 48_000, &[0.5f32; 64]);

    let chain_id = ChainId("chain_614_stop".to_string());
    let project = make_project(&chain_id.0);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    let controller = RefCell::new(Some(make_controller(&chain_id)));

    // Load + play so the runtime is armed.
    dispatcher
        .dispatch(Command::Chain(ChainCommand::SetChainDiLoopSource {
            chain: chain_id.clone(),
            source: DiLoopSource::File(wav),
        }))
        .expect("SetChainDiLoopSource must succeed");
    // #693: wait for the off-thread decode to land before playing.
    {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while dispatcher.di_loop_for_chain(&chain_id).is_none()
            && std::time::Instant::now() < deadline
        {
            let _ = dispatcher.poll_async_results();
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    adapter_gui::di_loop_wiring::play_chain_di_loop(&controller, &dispatcher, &chain_id);

    assert!(
        controller
            .borrow()
            .as_ref()
            .unwrap()
            .di_stream_active(&chain_id),
        "precondition: di loop must be armed after play"
    );

    // Now stop — runtime must be cleared.
    adapter_gui::di_loop_wiring::stop_chain_di_loop(&controller, &dispatcher, &chain_id);

    assert!(
        !controller
            .borrow()
            .as_ref()
            .unwrap()
            .di_stream_active(&chain_id),
        "REGRESSION #614: stop_chain_di_loop did not disarm the runtime — \
         di_stream_active() is still true after pressing Stop"
    );
}

// ── #771: play never touches the guitar runtime ─────────────────────────────

/// The DI plays ONLY on its isolated pre-rendered stream. Pressing Play must
/// leave the guitar runtime's injection slot empty — the loop never rides the
/// guitar's stream, meters, or outputs (isolation #4).
#[test]
fn play_leaves_the_guitar_runtime_unarmed() {
    let dir = tempfile::tempdir().expect("tempdir");
    let wav = dir.path().join("di.wav");
    write_mono_wav(&wav, 48_000, &[0.5f32; 64]);

    let chain_id = ChainId("chain_771_isolated".to_string());
    let project = make_project(&chain_id.0);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    let controller = RefCell::new(Some(make_controller(&chain_id)));

    dispatcher
        .dispatch(Command::Chain(ChainCommand::SetChainDiLoopSource {
            chain: chain_id.clone(),
            source: DiLoopSource::File(wav),
        }))
        .expect("SetChainDiLoopSource must succeed");
    {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while dispatcher.di_loop_for_chain(&chain_id).is_none()
            && std::time::Instant::now() < deadline
        {
            let _ = dispatcher.poll_async_results();
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    adapter_gui::di_loop_wiring::play_chain_di_loop(&controller, &dispatcher, &chain_id);

    assert!(
        controller
            .borrow()
            .as_ref()
            .unwrap()
            .di_stream_active(&chain_id),
        "precondition: play arms the isolated DI stream"
    );
    assert!(
        !controller
            .borrow()
            .as_ref()
            .unwrap()
            .chain_has_di_loop(&chain_id),
        "#771: play must NOT inject the loop into the guitar runtime — the DI \
         plays only on its isolated pre-rendered stream"
    );
}
