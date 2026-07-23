//! Issue #581 (additional scope) — Settings → System → Audio:
//! clicking **Aplicar** mutates the in-memory session, but the choice
//! is lost on restart, so the user re-picks the device every cold
//! start.
//!
//! Root cause: `Command::SaveAudioSettings`
//! (`crates/application/src/local_dispatcher_project.rs`) writes the
//! new `device_settings` into the in-memory `Project` and emits
//! `Event::AudioSettingsSaved`, but never touches durable storage.
//! Today the durable side is bolted onto the GUI caller
//! (`crates/adapter-gui/src/settings/audio.rs` calls
//! `FilesystemStorage::save_gui_audio_settings(...)` before dispatching),
//! which means any other transport (MCP, gRPC) dispatching the same
//! command silently loses the user's pick — a violation of the
//! Command-bus parity LAW: every transport must hit the same persistence
//! the GUI does. Persistence belongs INSIDE the handler.
//!
//! This test reproduces the broken contract end-to-end: dispatch
//! `SaveAudioSettings` against a fresh dispatcher and then reload the
//! per-machine `config.yaml` from disk — exactly what
//! `load_project_session` does on the next launch. RED today, GREEN
//! once the handler persists.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Mutex;

use application::command::Command;
use application::dispatcher::CommandDispatcher;
use application::local_dispatcher::LocalDispatcher;
use domain::ids::DeviceId;
use infra_filesystem::FilesystemStorage;
use project::device::DeviceSettings;
use project::project::Project;

/// `$HOME` is process-global; serialise tests that swap it.
static HOME_LOCK: Mutex<()> = Mutex::new(());

fn empty_project_rc() -> Rc<RefCell<Project>> {
    Rc::new(RefCell::new(Project {
        name: Some("issue-581-fixture".to_string()),
        device_settings: vec![],
        chains: vec![],
        midi: None,
    }))
}

fn picked_device() -> DeviceSettings {
    DeviceSettings {
        device_id: DeviceId("picked-device".to_string()),
        sample_rate: 88200,
        buffer_size_frames: 128,
        bit_depth: 24,
        #[cfg(target_os = "linux")]
        realtime: true,
        #[cfg(target_os = "linux")]
        rt_priority: 70,
        #[cfg(target_os = "linux")]
        nperiods: 3,
    }
}

/// Run `f` with `$HOME` redirected at a fresh tempdir so the FS write
/// stays out of the developer's real `~/Library/Application Support/`
/// — mirrors `tests/issue_540_set_paths_persists.rs::with_temp_home`.
fn with_temp_home<F: FnOnce(&PathBuf)>(label: &str, f: F) {
    let _g = HOME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let tmp =
        std::env::temp_dir().join(format!("openrig-581-{label}-{}-{now}", std::process::id()));
    std::fs::create_dir_all(&tmp).expect("mkdir tempdir");
    let prev = std::env::var_os("HOME");
    // dirs::config_dir() honours $XDG_CONFIG_HOME over $HOME/.config on Linux
    // (CI runners set it), so a HOME-only swap leaks to the runner's real
    // config dir. Track XDG alongside HOME so config paths follow the tempdir.
    let prev_xdg = std::env::var_os("XDG_CONFIG_HOME");
    std::env::set_var("HOME", &tmp);
    std::env::set_var("XDG_CONFIG_HOME", tmp.join(".config"));
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(&tmp)));
    if let Some(prev) = prev {
        std::env::set_var("HOME", prev);
    } else {
        std::env::remove_var("HOME");
    }
    if let Some(prev_xdg) = prev_xdg {
        std::env::set_var("XDG_CONFIG_HOME", prev_xdg);
    } else {
        std::env::remove_var("XDG_CONFIG_HOME");
    }
    let _ = std::fs::remove_dir_all(&tmp);
    if let Err(p) = res {
        std::panic::resume_unwind(p);
    }
}

#[test]
fn issue_581_save_audio_settings_persists_into_config_yaml() {
    with_temp_home("save-audio", |_| {
        let project = empty_project_rc();
        let dispatcher = LocalDispatcher::new(Rc::clone(&project));

        // The single action under test: user clicks **Aplicar** with a
        // picked device + non-default sample rate / buffer / bit depth.
        // The GUI dispatches exactly one command — no side helper is
        // invoked on the caller, so persistence MUST live inside the
        // handler for MCP and gRPC clients to behave the same way.
        let picked = picked_device();
        dispatcher
            .dispatch(Command::SaveAudioSettings {
                input_devices: vec![picked.clone()],
                output_devices: vec![],
            })
            .expect("SaveAudioSettings dispatch");
        // #693: the config write runs on the persist worker — wait for
        // durability before reading config.yaml back.
        application::persist_worker::flush();

        // Simulate the next cold start: re-read the per-machine
        // `config.yaml` from disk fresh — this is what
        // `load_project_session` does in `adapter-gui/src/project_ops.rs`
        // before populating `project.device_settings`.
        let config = FilesystemStorage::load_app_config().expect("load_app_config from fresh HOME");

        // The picked device must round-trip into the durable config so
        // `project_ops::build_device_settings_from_gui` can rebuild it
        // on the next launch.
        let persisted_device_ids: Vec<&str> = config
            .input_devices
            .iter()
            .chain(config.output_devices.iter())
            .map(|d| d.device_id.as_str())
            .collect();
        assert!(
            persisted_device_ids.contains(&picked.device_id.0.as_str()),
            "REGRESSION: Command::SaveAudioSettings did not persist `{}` into config.yaml — \
             the user's audio device pick survives in memory only and is lost on the next \
             app start. Persist inside the handler so MCP and gRPC clients dispatching the \
             same command get persistence too. Persisted ids: {:?}",
            picked.device_id.0,
            persisted_device_ids,
        );

        // And the picked sample-rate / buffer / bit-depth must round-trip too —
        // otherwise the device row would come back selected with default values.
        let persisted = config
            .input_devices
            .iter()
            .chain(config.output_devices.iter())
            .find(|d| d.device_id == picked.device_id.0)
            .expect("picked device id present in config.yaml after dispatch");
        assert_eq!(
            persisted.sample_rate, picked.sample_rate,
            "sample_rate must round-trip through config.yaml"
        );
        assert_eq!(
            persisted.buffer_size_frames, picked.buffer_size_frames,
            "buffer_size_frames must round-trip through config.yaml"
        );
        assert_eq!(
            persisted.bit_depth, picked.bit_depth,
            "bit_depth must round-trip through config.yaml"
        );
    });
}
