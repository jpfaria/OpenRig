//! The player commands drive the runtime from the dispatcher, so a GUI button,
//! an MCP client and a footswitch all reach the same sound.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use engine::player::settings::PlayerSettings;
use infra_filesystem::{FilesystemStorage, PlayerConfig};
use project::project::Project;

use crate::command::{Command, PlayerCommand};
use crate::dispatcher::CommandDispatcher;
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;
use crate::player_event::PlayerEvent;
use crate::player_library::PlayerLibraryDirs;
use crate::player_state::PlayerControlState;
use crate::runtime_control::RuntimeControl;

#[derive(Default)]
struct SpyRuntimeControl {
    calls: Rc<RefCell<Vec<String>>>,
}

fn describe(settings: PlayerSettings) -> String {
    format!(
        "vol={} speed={} semis={} loop={:?}",
        settings.volume, settings.speed, settings.semitones, settings.loop_range
    )
}

impl SpyRuntimeControl {
    fn log(&self, line: String) {
        self.calls.borrow_mut().push(line);
    }
}

impl RuntimeControl for SpyRuntimeControl {
    fn load_player_track(&self, path: &Path) -> anyhow::Result<()> {
        self.log(format!("load {}", path.display()));
        Ok(())
    }

    fn start_player(
        &self,
        track: &Path,
        settings: PlayerSettings,
        output_key: Option<&str>,
    ) -> anyhow::Result<()> {
        self.log(format!(
            "start {} [{}] on {}",
            track.display(),
            describe(settings),
            output_key.unwrap_or("<first>")
        ));
        Ok(())
    }

    fn pause_player(&self) {
        self.log("pause".into());
    }

    fn stop_player(&self) {
        self.log("stop".into());
    }

    fn seek_player(&self, seconds: f64) {
        self.log(format!("seek {seconds}"));
    }

    fn set_player_settings(&self, settings: PlayerSettings) {
        self.log(format!("settings [{}]", describe(settings)));
    }

    fn refresh_player_output(&self, output_key: Option<&str>) -> anyhow::Result<()> {
        self.log(format!("refresh on {}", output_key.unwrap_or("<first>")));
        Ok(())
    }

    fn sync_chain(&self, chain: &domain::ids::ChainId) -> anyhow::Result<()> {
        self.log(format!("sync chain {}", chain.0));
        Ok(())
    }
}

struct FailingStart;

impl RuntimeControl for FailingStart {
    fn start_player(
        &self,
        _track: &Path,
        _settings: PlayerSettings,
        _output_key: Option<&str>,
    ) -> anyhow::Result<()> {
        Err(anyhow::anyhow!("no output endpoint to play through"))
    }
}

fn dispatcher() -> LocalDispatcher {
    LocalDispatcher::new(Rc::new(RefCell::new(Project {
        name: None,
        device_settings: Vec::new(),
        chains: Vec::new(),
        midi: None,
    })))
}

fn spied() -> (LocalDispatcher, Rc<RefCell<Vec<String>>>) {
    let calls = Rc::new(RefCell::new(Vec::new()));
    let dispatcher = dispatcher();
    dispatcher.attach_runtime_control(Rc::new(SpyRuntimeControl {
        calls: Rc::clone(&calls),
    }));
    (dispatcher, calls)
}

fn run(dispatcher: &LocalDispatcher, cmd: PlayerCommand) -> anyhow::Result<Vec<Event>> {
    dispatcher.dispatch(Command::Player(cmd))
}

/// A real file with a supported extension; the dispatcher only checks that it
/// exists, the decode happens in the runtime.
fn track_file(dir: &tempfile::TempDir, name: &str) -> PathBuf {
    let path = dir.path().join(name);
    std::fs::write(&path, b"x").unwrap();
    path
}

fn loaded() -> (
    LocalDispatcher,
    Rc<RefCell<Vec<String>>>,
    PathBuf,
    tempfile::TempDir,
) {
    let (dispatcher, calls) = spied();
    let dir = tempfile::tempdir().unwrap();
    let track = track_file(&dir, "blues.m4a");
    run(
        &dispatcher,
        PlayerCommand::LoadPlayerTrack {
            path: track.clone(),
        },
    )
    .expect("load");
    calls.borrow_mut().clear();
    (dispatcher, calls, track, dir)
}

#[test]
fn loading_a_track_reaches_the_runtime_and_is_remembered() {
    let (dispatcher, calls) = spied();
    let dir = tempfile::tempdir().unwrap();
    let track = track_file(&dir, "blues.m4a");
    let events = run(
        &dispatcher,
        PlayerCommand::LoadPlayerTrack {
            path: track.clone(),
        },
    )
    .expect("load");
    assert_eq!(*calls.borrow(), vec![format!("load {}", track.display())]);
    assert_eq!(
        events,
        vec![Event::Player(PlayerEvent::TrackLoaded {
            path: track.clone()
        })]
    );
    let snapshot = dispatcher.player_snapshot();
    assert_eq!(snapshot.track, Some(track));
    assert!(!snapshot.playing);
}

#[test]
fn a_missing_or_unsupported_file_is_refused() {
    let (dispatcher, calls) = spied();
    let dir = tempfile::tempdir().unwrap();
    let text = track_file(&dir, "notes.txt");
    for path in [dir.path().join("gone.wav"), text] {
        assert!(run(&dispatcher, PlayerCommand::LoadPlayerTrack { path }).is_err());
    }
    assert!(calls.borrow().is_empty());
    assert_eq!(dispatcher.player_snapshot().track, None);
}

#[test]
fn play_starts_the_loaded_track_with_the_dispatchers_settings() {
    let (dispatcher, calls, track, _dir) = loaded();
    run(&dispatcher, PlayerCommand::SetPlayerSpeed { speed: 0.75 }).unwrap();
    run(
        &dispatcher,
        PlayerCommand::SetPlayerSemitones { semitones: -2.0 },
    )
    .unwrap();
    run(
        &dispatcher,
        PlayerCommand::SetPlayerOutput {
            device_id: Some("binding-a\u{1f}main".into()),
        },
    )
    .unwrap();
    calls.borrow_mut().clear();
    let events = run(
        &dispatcher,
        PlayerCommand::SetPlayerPlaying { playing: true },
    )
    .unwrap();
    let expected = describe(PlayerSettings {
        speed: 0.75,
        semitones: -2.0,
        ..PlayerSettings::default()
    });
    assert_eq!(
        *calls.borrow(),
        vec![format!(
            "start {} [{expected}] on binding-a\u{1f}main",
            track.display()
        )]
    );
    assert_eq!(
        events,
        vec![Event::Player(PlayerEvent::TransportChanged {
            playing: true
        })]
    );
    assert!(dispatcher.player_snapshot().playing);
}

#[test]
fn play_without_a_track_is_refused() {
    let (dispatcher, calls) = spied();
    assert!(run(
        &dispatcher,
        PlayerCommand::SetPlayerPlaying { playing: true }
    )
    .is_err());
    assert!(calls.borrow().is_empty());
}

#[test]
fn a_start_the_runtime_refused_leaves_the_player_stopped() {
    let dispatcher = dispatcher();
    let dir = tempfile::tempdir().unwrap();
    let track = track_file(&dir, "blues.m4a");
    run(&dispatcher, PlayerCommand::LoadPlayerTrack { path: track }).unwrap();
    dispatcher.attach_runtime_control(Rc::new(FailingStart));
    assert!(run(
        &dispatcher,
        PlayerCommand::SetPlayerPlaying { playing: true }
    )
    .is_err());
    assert!(!dispatcher.player_snapshot().playing);
}

#[test]
fn pause_reaches_the_runtime() {
    let (dispatcher, calls, _track, _dir) = loaded();
    let events = run(
        &dispatcher,
        PlayerCommand::SetPlayerPlaying { playing: false },
    )
    .unwrap();
    assert_eq!(*calls.borrow(), vec!["pause".to_string()]);
    assert_eq!(
        events,
        vec![Event::Player(PlayerEvent::TransportChanged {
            playing: false
        })]
    );
}

#[test]
fn stop_rewinds() {
    let (dispatcher, calls, _track, _dir) = loaded();
    run(
        &dispatcher,
        PlayerCommand::SetPlayerPlaying { playing: true },
    )
    .unwrap();
    calls.borrow_mut().clear();
    let events = run(&dispatcher, PlayerCommand::StopPlayer).unwrap();
    assert_eq!(*calls.borrow(), vec!["stop".to_string()]);
    assert_eq!(
        events,
        vec![
            Event::Player(PlayerEvent::TransportChanged { playing: false }),
            Event::Player(PlayerEvent::Seeked { seconds: 0.0 })
        ]
    );
    assert!(!dispatcher.player_snapshot().playing);
}

#[test]
fn seek_is_clamped_at_the_start_and_rejects_nonsense() {
    let (dispatcher, calls, _track, _dir) = loaded();
    let events = run(&dispatcher, PlayerCommand::SeekPlayer { seconds: -3.0 }).unwrap();
    assert_eq!(*calls.borrow(), vec!["seek 0".to_string()]);
    assert_eq!(
        events,
        vec![Event::Player(PlayerEvent::Seeked { seconds: 0.0 })]
    );
    assert!(run(&dispatcher, PlayerCommand::SeekPlayer { seconds: f64::NAN }).is_err());
}

#[test]
fn settings_are_clamped_before_they_reach_the_runtime() {
    let (dispatcher, calls) = spied();
    let events = run(&dispatcher, PlayerCommand::SetPlayerSpeed { speed: 5.0 }).unwrap();
    let expected = PlayerSettings {
        speed: 2.0,
        ..PlayerSettings::default()
    };
    assert_eq!(
        *calls.borrow(),
        vec![format!("settings [{}]", describe(expected))]
    );
    assert_eq!(
        events,
        vec![Event::Player(PlayerEvent::SettingsChanged {
            volume: expected.volume,
            speed: 2.0,
            semitones: 0.0,
            loop_start: None,
            loop_end: None,
        })]
    );
}

#[test]
fn a_loop_is_set_and_cleared() {
    let (dispatcher, _calls, _track, _dir) = loaded();
    run(
        &dispatcher,
        PlayerCommand::SetPlayerLoop {
            start_seconds: 4.0,
            end_seconds: 12.0,
        },
    )
    .unwrap();
    assert_eq!(
        dispatcher.player_snapshot().settings.loop_range,
        Some((4.0, 12.0))
    );
    run(&dispatcher, PlayerCommand::ClearPlayerLoop).unwrap();
    assert_eq!(dispatcher.player_snapshot().settings.loop_range, None);
}

#[test]
fn a_reversed_loop_is_refused() {
    let (dispatcher, _calls, _track, _dir) = loaded();
    assert!(run(
        &dispatcher,
        PlayerCommand::SetPlayerLoop {
            start_seconds: 8.0,
            end_seconds: 2.0,
        },
    )
    .is_err());
    assert_eq!(dispatcher.player_snapshot().settings.loop_range, None);
}

#[test]
fn picking_an_output_moves_a_playing_player_and_starts_nothing() {
    let (dispatcher, calls) = spied();
    let events = run(
        &dispatcher,
        PlayerCommand::SetPlayerOutput {
            device_id: Some("binding-b\u{1f}phones".into()),
        },
    )
    .unwrap();
    assert_eq!(
        *calls.borrow(),
        vec!["refresh on binding-b\u{1f}phones".to_string()]
    );
    assert_eq!(
        events,
        vec![Event::Player(PlayerEvent::OutputChanged {
            device_id: Some("binding-b\u{1f}phones".into())
        })]
    );
}

#[test]
fn no_player_command_ever_touches_a_chains_runtime() {
    let (dispatcher, calls, _track, _dir) = loaded();
    for cmd in [
        PlayerCommand::SetPlayerPlaying { playing: true },
        PlayerCommand::SetPlayerVolume { volume: 0.5 },
        PlayerCommand::SeekPlayer { seconds: 1.0 },
        PlayerCommand::StopPlayer,
    ] {
        run(&dispatcher, cmd).unwrap();
    }
    assert!(
        !calls.borrow().iter().any(|call| call.contains("chain")),
        "{:?}",
        calls.borrow()
    );
}

#[test]
fn volume_and_output_persist_to_the_attached_config() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.yaml");
    let dispatcher = dispatcher();
    dispatcher.attach_player_state(Rc::new(RefCell::new(PlayerControlState::restored(
        &PlayerConfig::default(),
        PlayerLibraryDirs::default(),
        Some(config.clone()),
    ))));
    run(&dispatcher, PlayerCommand::SetPlayerVolume { volume: 0.4 }).unwrap();
    run(
        &dispatcher,
        PlayerCommand::SetPlayerOutput {
            device_id: Some("binding-c\u{1f}mon".into()),
        },
    )
    .unwrap();
    // Speed is a practice choice of the moment, never persisted.
    run(&dispatcher, PlayerCommand::SetPlayerSpeed { speed: 0.5 }).unwrap();
    crate::persist_worker::flush();
    let saved = FilesystemStorage::load_app_config_at(&config).expect("config written");
    assert_eq!(saved.player.volume, 0.4);
    assert_eq!(
        saved.player.output_device.as_deref(),
        Some("binding-c\u{1f}mon")
    );
}
