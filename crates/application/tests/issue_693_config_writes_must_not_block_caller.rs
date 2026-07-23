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
    // Best-effort unblock of any worker still parked on the FIFO. Do it on a
    // DETACHED thread: on Linux the handler resolves config under
    // $XDG_CONFIG_HOME (not this macOS-layout path), so it never opens this
    // FIFO — a blocking write-open on the test thread would then wait forever
    // for a reader and wedge the whole suite. The assertion below only needs
    // `result`, already in hand; the parked thread dies with the process.
    {
        let cfg = cfg.clone();
        std::thread::spawn(move || {
            let _ = std::fs::OpenOptions::new().write(true).open(&cfg);
        });
    }

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
                io_binding_ids: vec![],
                blocks: Vec::new(),
                di_output: None,
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
    // Best-effort pairing on a DETACHED thread so a missing peer can never
    // wedge the test thread (see the save-audio-settings case above).
    {
        let preset_file = preset_file.clone();
        std::thread::spawn(move || {
            let _ = std::fs::OpenOptions::new().read(true).open(&preset_file);
        });
    }

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
                io_binding_ids: vec![],
                blocks: Vec::new(),
                di_output: None,
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
    // Best-effort pairing on a DETACHED thread so a missing peer can never
    // wedge the test thread (see the save-audio-settings case above).
    {
        let wav = wav.clone();
        std::thread::spawn(move || {
            let _ = std::fs::OpenOptions::new().write(true).open(&wav);
        });
    }

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

#[test]
fn issue_693_reload_plugin_catalog_completes_via_poll() {
    use application::event::Event;

    // Empty plugin roots under a temp HOME — the scan itself is trivial
    // here; the contract under test is WHERE it runs: the dispatch must
    // only enqueue (empty return) and the completion event must arrive
    // via poll_async_results, like every other off-thread command.
    let tmp = tempfile::TempDir::new().expect("tempdir");
    std::env::set_var("HOME", tmp.path());

    let dispatcher = LocalDispatcher::new(Rc::new(RefCell::new(Project {
        name: None,
        device_settings: Vec::new(),
        chains: Vec::new(),
        midi: None,
    })));

    let events = dispatcher
        .dispatch(Command::ReloadPluginCatalog)
        .expect("ReloadPluginCatalog dispatch");
    assert!(
        events.is_empty(),
        "dispatch must only enqueue the rescan (its own task); got \
         synchronous events {events:?} (issue #693)"
    );

    let deadline = Instant::now() + Duration::from_secs(2);
    let mut done = Vec::new();
    while done.is_empty() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
        done = dispatcher.poll_async_results();
    }
    assert!(
        done.iter()
            .any(|e| matches!(e, Event::PluginCatalogReloaded { .. })),
        "expected PluginCatalogReloaded via poll, got {done:?}"
    );
}

#[test]
fn issue_693_render_chain_returns_immediately_with_stuck_chain_file() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let chain_yaml = tmp.path().join("chain.yaml");
    let status = std::process::Command::new("mkfifo")
        .arg(&chain_yaml)
        .status()
        .expect("run mkfifo");
    assert!(status.success(), "mkfifo failed");
    let input = tmp.path().join("in.wav");
    let output = tmp.path().join("out.wav");

    let (done_tx, done_rx) = mpsc::channel();
    let chain_for_caller = chain_yaml.clone();
    std::thread::spawn(move || {
        let dispatcher = LocalDispatcher::new(Rc::new(RefCell::new(Project {
            name: None,
            device_settings: Vec::new(),
            chains: Vec::new(),
            midi: None,
        })));
        let t0 = Instant::now();
        let _ = dispatcher.dispatch(Command::RenderChain {
            chain_path: chain_for_caller.to_string_lossy().to_string(),
            input_path: input.to_string_lossy().to_string(),
            output_path: output.to_string_lossy().to_string(),
            start_s: None,
            end_s: None,
            sample_rate_hz: None,
            block_size: None,
            bit_depth: None,
            tail_ms: None,
        });
        let _ = done_tx.send(t0.elapsed());
    });

    let result = done_rx.recv_timeout(Duration::from_secs(2));
    // Best-effort pairing on a DETACHED thread so a missing peer can never
    // wedge the test thread (see the save-audio-settings case above).
    {
        let chain_yaml = chain_yaml.clone();
        std::thread::spawn(move || {
            let _ = std::fs::OpenOptions::new().write(true).open(&chain_yaml);
        });
    }

    let elapsed = result.expect(
        "dispatch(RenderChain) is stuck on the offline render I/O — the \
         render runs inline on the calling thread (issue #693)",
    );
    assert!(
        elapsed < Duration::from_millis(500),
        "RenderChain held the caller for {elapsed:?} — the offline render \
         belongs to its own task (issue #693)"
    );
}
