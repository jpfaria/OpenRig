//! Issue #693 — config-persisting commands must not hold the caller.
//!
//! `SaveAudioSettings` (and the `Set*Path` trio) do a load-modify-write
//! of the per-machine `config.yaml` INLINE in the handler — on the
//! dispatching (GUI/MCP) thread. With a slow disk or stuck sink the
//! click freezes. Contract: the handler returns immediately; the
//! read-modify-write runs ordered on the persist worker.
//!
//! Worst-case sink: `config.yaml` is a FIFO with no reader under a
//! temp `$HOME` (same HOME-swap precedent as the issue #581 test).
#![cfg(unix)]

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use application::command::Command;
use application::dispatcher::CommandDispatcher;
use application::local_dispatcher::LocalDispatcher;
use project::project::Project;

#[test]
fn issue_693_save_audio_settings_returns_immediately_with_stuck_config() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let cfg_dir = tmp.path().join("Library/Application Support/OpenRig");
    std::fs::create_dir_all(&cfg_dir).expect("create config dir");
    let cfg = cfg_dir.join("config.yaml");
    let status = std::process::Command::new("mkfifo")
        .arg(&cfg)
        .status()
        .expect("run mkfifo");
    assert!(status.success(), "mkfifo failed");
    std::env::set_var("HOME", tmp.path());

    let (done_tx, done_rx) = mpsc::channel();
    std::thread::spawn(move || {
        let dispatcher = LocalDispatcher::new(Rc::new(RefCell::new(Project {
            name: None,
            device_settings: Vec::new(),
            chains: Vec::new(),
            midi: None,
        })));
        let t0 = Instant::now();
        let _ = dispatcher.dispatch(Command::SaveAudioSettings {
            input_devices: Vec::new(),
            output_devices: Vec::new(),
        });
        let _ = done_tx.send(t0.elapsed());
    });

    let result = done_rx.recv_timeout(Duration::from_secs(2));
    // Unblock the FIFO: the handler's first touch is a LOAD (read-open),
    // so pair it with a write-open + immediate drop (EOF). Detached
    // threads still stuck at process exit die with the process.
    let _ = std::fs::OpenOptions::new().write(true).open(&cfg);

    let elapsed = result.expect(
        "dispatch(SaveAudioSettings) is stuck on config.yaml I/O — the \
         handler persists inline on the calling thread (issue #693)",
    );
    assert!(
        elapsed < Duration::from_millis(500),
        "SaveAudioSettings held the caller for {elapsed:?} — config \
         persistence belongs to the persist worker (issue #693)"
    );
}

#[test]
fn issue_693_save_chain_preset_returns_immediately_with_stuck_preset_file() {
    use domain::ids::ChainId;
    use project::chain::Chain;

    let tmp = tempfile::TempDir::new().expect("tempdir");
    let presets = tmp.path().join("presets");
    std::fs::create_dir_all(&presets).expect("presets dir");
    // `SaveChainPreset { name: "p" }` resolves to `<presets>/p.yaml`.
    let preset_file = presets.join("p.yaml");
    let status = std::process::Command::new("mkfifo")
        .arg(&preset_file)
        .status()
        .expect("run mkfifo");
    assert!(status.success(), "mkfifo failed");

    let (done_tx, done_rx) = mpsc::channel();
    let presets_for_caller = presets.clone();
    std::thread::spawn(move || {
        let chain_id = ChainId("c1".into());
        let dispatcher = LocalDispatcher::new(Rc::new(RefCell::new(Project {
            name: None,
            device_settings: Vec::new(),
            chains: vec![Chain {
                id: chain_id.clone(),
                description: None,
                instrument: "electric_guitar".into(),
                enabled: false,
                volume: 1.0,
                blocks: Vec::new(),
            }],
            midi: None,
        })));
        dispatcher.attach_presets_path(presets_for_caller);
        let t0 = Instant::now();
        let _ = dispatcher.dispatch(Command::SaveChainPreset {
            chain: chain_id,
            name: "p".into(),
        });
        let _ = done_tx.send(t0.elapsed());
    });

    let result = done_rx.recv_timeout(Duration::from_secs(2));
    // Pair the write-open with a read-open so blocked threads release.
    let _ = std::fs::OpenOptions::new().read(true).open(&preset_file);

    let elapsed = result.expect(
        "dispatch(SaveChainPreset) is stuck on the preset file I/O — the \
         handler writes inline on the calling thread (issue #693)",
    );
    assert!(
        elapsed < Duration::from_millis(500),
        "SaveChainPreset held the caller for {elapsed:?} — preset \
         persistence belongs to the persist worker (issue #693)"
    );
}

#[test]
fn issue_693_set_di_loop_source_returns_immediately_with_stuck_wav() {
    use application::di_loader::DiLoopSource;
    use domain::ids::ChainId;
    use project::chain::Chain;

    let tmp = tempfile::TempDir::new().expect("tempdir");
    let wav = tmp.path().join("loop.wav");
    let status = std::process::Command::new("mkfifo")
        .arg(&wav)
        .status()
        .expect("run mkfifo");
    assert!(status.success(), "mkfifo failed");

    let (done_tx, done_rx) = mpsc::channel();
    let wav_for_caller = wav.clone();
    std::thread::spawn(move || {
        let chain_id = ChainId("c1".into());
        let dispatcher = LocalDispatcher::new(Rc::new(RefCell::new(Project {
            name: None,
            device_settings: Vec::new(),
            chains: vec![Chain {
                id: chain_id.clone(),
                description: None,
                instrument: "electric_guitar".into(),
                enabled: false,
                volume: 1.0,
                blocks: Vec::new(),
            }],
            midi: None,
        })));
        let t0 = Instant::now();
        let _ = dispatcher.dispatch(Command::SetChainDiLoopSource {
            chain: chain_id,
            source: DiLoopSource::File(wav_for_caller),
        });
        let _ = done_tx.send(t0.elapsed());
    });

    let result = done_rx.recv_timeout(Duration::from_secs(2));
    // Pair the decoder's read-open with a write-open (drop = EOF).
    let _ = std::fs::OpenOptions::new().write(true).open(&wav);

    let elapsed = result.expect(
        "dispatch(SetChainDiLoopSource) is stuck decoding the WAV — the \
         handler loads the DI loop inline on the calling thread (issue #693)",
    );
    assert!(
        elapsed < Duration::from_millis(500),
        "SetChainDiLoopSource held the caller for {elapsed:?} — the DI \
         decode belongs to its own task (issue #693)"
    );
}
