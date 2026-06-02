//! Task 5 — RED-FIRST test for `Command::SetChainDiLoopSource` and
//! `Command::SetChainDiLoopEnabled`.
//!
//! These commands are ephemeral (not persisted to the project) and distinct
//! from any project-level DI configuration (#324). The test mirrors the
//! pattern in `issue_614_load_di_loop.rs` (WAV helper) and
//! `local_dispatcher_output_tests.rs` (dispatcher harness).

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use application::command::Command;
use application::di_loader::DiLoopSource;
use application::dispatcher::CommandDispatcher;
use application::event::Event;
use application::local_dispatcher::LocalDispatcher;
use domain::ids::ChainId;
use project::chain::Chain;
use project::project::Project;

/// Write a minimal valid mono PCM-float WAV at the given sample rate.
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
            blocks: vec![],
        }],
        midi: None,
    }))
}

/// Dispatching `SetChainDiLoopSource` with a valid WAV must succeed and emit
/// `Event::ChainDiLoopSourceChanged`.
#[test]
fn set_chain_di_loop_source_valid_file_emits_event() {
    let dir = tempfile::tempdir().expect("tempdir");
    let wav = dir.path().join("di.wav");
    let samples: Vec<f32> = (0..256).map(|i| i as f32 / 255.0).collect();
    write_mono_wav(&wav, 48_000, &samples);

    let project = make_project("chain_0");
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let events = dispatcher
        .dispatch(Command::SetChainDiLoopSource {
            chain: ChainId("chain_0".to_string()),
            source: DiLoopSource::File(wav),
        })
        .expect("SetChainDiLoopSource must succeed for a valid WAV");

    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::ChainDiLoopSourceChanged { chain }
            if chain.0 == "chain_0"
        )),
        "expected Event::ChainDiLoopSourceChanged for chain_0, got {events:?}"
    );
}

/// After `SetChainDiLoopSource`, enabling the DI loop must emit
/// `Event::ChainDiLoopEnabledChanged { enabled: true }`.
#[test]
fn set_chain_di_loop_enabled_true_after_source_emits_event() {
    let dir = tempfile::tempdir().expect("tempdir");
    let wav = dir.path().join("di.wav");
    write_mono_wav(&wav, 48_000, &[0.5f32; 64]);

    let project = make_project("chain_0");
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    // Load a source first.
    dispatcher
        .dispatch(Command::SetChainDiLoopSource {
            chain: ChainId("chain_0".to_string()),
            source: DiLoopSource::File(wav),
        })
        .expect("source must load");

    // Enable it.
    let events = dispatcher
        .dispatch(Command::SetChainDiLoopEnabled {
            chain: ChainId("chain_0".to_string()),
            enabled: true,
        })
        .expect("SetChainDiLoopEnabled true must succeed");

    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::ChainDiLoopEnabledChanged { chain, enabled: true }
            if chain.0 == "chain_0"
        )),
        "expected Event::ChainDiLoopEnabledChanged{{chain_0, true}}, got {events:?}"
    );
}

/// `SetChainDiLoopEnabled { enabled: false }` must emit the disabled event even
/// when no loop was previously enabled (idempotent clear).
#[test]
fn set_chain_di_loop_enabled_false_emits_event() {
    let project = make_project("chain_0");
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let events = dispatcher
        .dispatch(Command::SetChainDiLoopEnabled {
            chain: ChainId("chain_0".to_string()),
            enabled: false,
        })
        .expect("SetChainDiLoopEnabled false must succeed");

    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::ChainDiLoopEnabledChanged { chain, enabled: false }
            if chain.0 == "chain_0"
        )),
        "expected Event::ChainDiLoopEnabledChanged{{chain_0, false}}, got {events:?}"
    );
}

/// `SetChainDiLoopSource` with a missing file must return `Err` (decode failure
/// is surfaced, not swallowed silently).
#[test]
fn set_chain_di_loop_source_missing_file_returns_err() {
    let project = make_project("chain_0");
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::SetChainDiLoopSource {
        chain: ChainId("chain_0".to_string()),
        source: DiLoopSource::File(std::path::PathBuf::from("/nonexistent/di.wav")),
    });

    assert!(
        result.is_err(),
        "missing file must return Err, not Ok"
    );
}

/// `SetChainDiLoopSource` for a non-existent chain must return `Err`.
#[test]
fn set_chain_di_loop_source_missing_chain_returns_err() {
    let dir = tempfile::tempdir().expect("tempdir");
    let wav = dir.path().join("di.wav");
    write_mono_wav(&wav, 48_000, &[0.5f32; 32]);

    let project = make_project("chain_0");
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::SetChainDiLoopSource {
        chain: ChainId("chain_MISSING".to_string()),
        source: DiLoopSource::File(wav),
    });

    assert!(
        result.is_err(),
        "missing chain must return Err"
    );
}

/// `SetChainDiLoopEnabled` for a non-existent chain must return `Err`.
#[test]
fn set_chain_di_loop_enabled_missing_chain_returns_err() {
    let project = make_project("chain_0");
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::SetChainDiLoopEnabled {
        chain: ChainId("chain_MISSING".to_string()),
        enabled: true,
    });

    assert!(
        result.is_err(),
        "missing chain for enable must return Err"
    );
}
