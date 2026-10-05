//! The drum commands drive the dispatcher's drum state and reach the
//! frontend's drum runtime; nothing here knows about Slint or devices.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use feature_dsp::drums::{DrumPattern, DrumSettings, Groove};
use infra_filesystem::DrumsConfig;
use project::project::Project;

use crate::command::{Command, DrumsCommand, MetronomeCommand};
use crate::dispatcher::CommandDispatcher;
use crate::drums::{DrumKitEntry, DrumLibrary};
use crate::drums_runtime::{DrumsRuntime, DrumsSetup};
use crate::drums_state::DrumsControlState;
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;
use crate::runtime_control::RuntimeControl;

#[derive(Default)]
struct SpyDrums {
    calls: Rc<RefCell<Vec<String>>>,
    fail_start: bool,
}

impl SpyDrums {
    fn log(&self, entry: String) {
        self.calls.borrow_mut().push(entry);
    }
}

impl DrumsRuntime for SpyDrums {
    fn start_drums(&self, setup: DrumsSetup<'_>) -> anyhow::Result<()> {
        if self.fail_start {
            anyhow::bail!("device gone");
        }
        self.log(format!(
            "start bpm={} vol={} kit={} groove={} on {}",
            setup.settings.bpm,
            setup.settings.volume,
            setup
                .kit_dir
                .map_or("-".into(), |d| d.display().to_string()),
            setup.groove.as_ref().map_or("-", |g| g.id.as_str()),
            setup.output_key.unwrap_or("<first>"),
        ));
        Ok(())
    }

    fn stop_drums(&self) {
        self.log("stop".into());
    }

    fn set_drums_settings(&self, settings: DrumSettings) {
        self.log(format!(
            "settings bpm={} vol={}",
            settings.bpm, settings.volume
        ));
    }

    fn set_drums_playing(&self, playing: bool) {
        self.log(format!("playing {playing}"));
    }

    fn trigger_drum_fill(&self) {
        self.log("fill".into());
    }

    fn set_drum_kit(&self, dir: &Path) {
        self.log(format!("kit {}", dir.display()));
    }

    fn set_drum_groove(&self, groove: Arc<Groove>) {
        self.log(format!("groove {}", groove.id));
    }

    fn refresh_drums_output(&self, output_key: Option<&str>) -> anyhow::Result<()> {
        self.log(format!("refresh on {}", output_key.unwrap_or("<first>")));
        Ok(())
    }
}

impl RuntimeControl for SpyDrums {
    fn drums(&self) -> Option<&dyn DrumsRuntime> {
        Some(self)
    }

    fn sync_chain(&self, chain: &domain::ids::ChainId) -> anyhow::Result<()> {
        self.log(format!("sync chain {}", chain.0));
        Ok(())
    }
}

fn groove(id: &str) -> Groove {
    Groove {
        id: id.into(),
        name: id.into(),
        genre: "rock".into(),
        beats_per_bar: 4,
        tempo: 100.0,
        beat: DrumPattern::new(4.0, Vec::new()),
        fills: Vec::new(),
    }
}

fn library() -> DrumLibrary {
    DrumLibrary {
        kits: vec![
            DrumKitEntry {
                id: "one".into(),
                name: "Kit One".into(),
                dir: PathBuf::from("/kits/one"),
            },
            DrumKitEntry {
                id: "two".into(),
                name: "Kit Two".into(),
                dir: PathBuf::from("/kits/two"),
            },
        ],
        grooves: vec![groove("rock-01"), groove("funk-01")],
    }
}

fn dispatcher_with(
    spy: SpyDrums,
    config: DrumsConfig,
) -> (LocalDispatcher, Rc<RefCell<Vec<String>>>) {
    let calls = Rc::clone(&spy.calls);
    let dispatcher = LocalDispatcher::new(Rc::new(RefCell::new(Project::default())));
    dispatcher.attach_drums_state(Rc::new(RefCell::new(DrumsControlState::restored(
        &config,
        library(),
        None,
    ))));
    dispatcher.attach_runtime_control(Rc::new(spy));
    (dispatcher, calls)
}

fn dispatcher() -> (LocalDispatcher, Rc<RefCell<Vec<String>>>) {
    dispatcher_with(SpyDrums::default(), DrumsConfig::default())
}

fn run(dispatcher: &LocalDispatcher, cmd: DrumsCommand) -> anyhow::Result<Vec<Event>> {
    dispatcher.dispatch(Command::Drums(cmd))
}

#[test]
fn restored_state_takes_the_saved_kit_and_groove() {
    let config = DrumsConfig {
        kit: Some("two".into()),
        groove: Some("funk-01".into()),
        ..DrumsConfig::default()
    };
    let state = DrumsControlState::restored(&config, library(), None);
    let snapshot = state.snapshot();
    assert_eq!(snapshot.kit.as_deref(), Some("two"));
    assert_eq!(snapshot.groove.as_deref(), Some("funk-01"));
    assert!(!snapshot.enabled && !snapshot.playing, "drums boot stopped");
}

#[test]
fn a_saved_kit_that_is_gone_falls_back_to_the_first_one() {
    let config = DrumsConfig {
        kit: Some("deleted".into()),
        groove: Some("deleted".into()),
        ..DrumsConfig::default()
    };
    let snapshot = DrumsControlState::restored(&config, library(), None).snapshot();
    assert_eq!(snapshot.kit.as_deref(), Some("one"));
    assert_eq!(snapshot.groove.as_deref(), Some("rock-01"));
}

#[test]
fn play_opens_the_drums_with_the_chosen_kit_and_groove_then_plays() {
    let (dispatcher, calls) = dispatcher();

    let events = run(&dispatcher, DrumsCommand::PlayDrums).expect("play");

    assert_eq!(
        *calls.borrow(),
        vec![
            "start bpm=120 vol=0.8 kit=/kits/one groove=rock-01 on <first>".to_string(),
            "playing true".to_string(),
        ]
    );
    let snapshot = dispatcher.drums_snapshot();
    assert!(snapshot.enabled && snapshot.playing);
    assert!(events.contains(&Event::DrumsTransportChanged {
        enabled: true,
        playing: true
    }));
}

#[test]
fn a_refused_start_leaves_the_drums_stopped() {
    let spy = SpyDrums {
        fail_start: true,
        ..SpyDrums::default()
    };
    let (dispatcher, _) = dispatcher_with(spy, DrumsConfig::default());

    assert!(run(&dispatcher, DrumsCommand::PlayDrums).is_err());

    let snapshot = dispatcher.drums_snapshot();
    assert!(!snapshot.enabled && !snapshot.playing);
}

#[test]
fn stop_keeps_the_output_open() {
    let (dispatcher, calls) = dispatcher();
    run(&dispatcher, DrumsCommand::PlayDrums).expect("play");
    calls.borrow_mut().clear();

    run(&dispatcher, DrumsCommand::StopDrums).expect("stop");

    assert_eq!(*calls.borrow(), vec!["playing false".to_string()]);
    let snapshot = dispatcher.drums_snapshot();
    assert!(snapshot.enabled && !snapshot.playing);
}

#[test]
fn toggle_alternates_play_and_stop() {
    let (dispatcher, _) = dispatcher();
    run(&dispatcher, DrumsCommand::ToggleDrums).expect("toggle");
    assert!(dispatcher.drums_snapshot().playing);
    run(&dispatcher, DrumsCommand::ToggleDrums).expect("toggle");
    assert!(!dispatcher.drums_snapshot().playing);
}

#[test]
fn disabling_closes_the_output_and_stops() {
    let (dispatcher, calls) = dispatcher();
    run(&dispatcher, DrumsCommand::PlayDrums).expect("play");
    calls.borrow_mut().clear();

    let events = run(
        &dispatcher,
        DrumsCommand::SetDrumsEnabled { enabled: false },
    )
    .expect("off");

    assert_eq!(*calls.borrow(), vec!["stop".to_string()]);
    let snapshot = dispatcher.drums_snapshot();
    assert!(!snapshot.enabled && !snapshot.playing);
    assert_eq!(
        events,
        vec![Event::DrumsTransportChanged {
            enabled: false,
            playing: false
        }]
    );
}

#[test]
fn enabling_opens_the_output_without_playing() {
    let (dispatcher, calls) = dispatcher();
    run(&dispatcher, DrumsCommand::SetDrumsEnabled { enabled: true }).expect("on");
    assert_eq!(calls.borrow().len(), 1);
    assert!(calls.borrow()[0].starts_with("start "));
    let snapshot = dispatcher.drums_snapshot();
    assert!(snapshot.enabled && !snapshot.playing);
}

#[test]
fn a_fill_needs_the_drums_playing() {
    let (dispatcher, calls) = dispatcher();
    assert!(run(&dispatcher, DrumsCommand::TriggerDrumFill).is_err());

    run(&dispatcher, DrumsCommand::PlayDrums).expect("play");
    calls.borrow_mut().clear();
    let events = run(&dispatcher, DrumsCommand::TriggerDrumFill).expect("fill");

    assert_eq!(*calls.borrow(), vec!["fill".to_string()]);
    assert_eq!(events, vec![Event::DrumFillTriggered]);
}

#[test]
fn the_project_tempo_reaches_the_drums_runtime() {
    let (dispatcher, calls) = dispatcher();
    let events = set_project_bpm(&dispatcher, 140.0);

    assert_eq!(dispatcher.drums_snapshot().bpm, 140.0);
    assert!(calls
        .borrow()
        .contains(&"settings bpm=140 vol=0.8".to_string()));
    assert!(events.contains(&Event::DrumsSettingsChanged {
        bpm: 140.0,
        volume: 0.8
    }));
}

fn set_project_bpm(dispatcher: &LocalDispatcher, bpm: f32) -> Vec<Event> {
    dispatcher
        .dispatch(Command::Metronome(MetronomeCommand::SetMetronomeBpm {
            bpm,
        }))
        .expect("bpm")
}

#[test]
fn volume_is_clamped_to_unity() {
    let (dispatcher, _) = dispatcher();
    run(&dispatcher, DrumsCommand::SetDrumsVolume { volume: 3.0 }).expect("vol");
    assert_eq!(dispatcher.drums_snapshot().volume, 1.0);
    run(&dispatcher, DrumsCommand::SetDrumsVolume { volume: -1.0 }).expect("vol");
    assert_eq!(dispatcher.drums_snapshot().volume, 0.0);
}

#[test]
fn selecting_a_kit_hands_its_folder_to_the_runtime() {
    let (dispatcher, calls) = dispatcher();
    let events = run(
        &dispatcher,
        DrumsCommand::SelectDrumKit { kit: "two".into() },
    )
    .expect("kit");

    assert_eq!(*calls.borrow(), vec!["kit /kits/two".to_string()]);
    assert_eq!(dispatcher.drums_snapshot().kit.as_deref(), Some("two"));
    assert_eq!(
        events,
        vec![Event::DrumsContentChanged {
            kit: Some("two".into()),
            groove: Some("rock-01".into())
        }]
    );
}

#[test]
fn an_unknown_kit_or_groove_is_an_error() {
    let (dispatcher, calls) = dispatcher();
    assert!(run(
        &dispatcher,
        DrumsCommand::SelectDrumKit { kit: "nope".into() }
    )
    .is_err());
    assert!(run(
        &dispatcher,
        DrumsCommand::SelectDrumGroove {
            groove: "nope".into()
        }
    )
    .is_err());
    assert!(calls.borrow().is_empty());
    assert_eq!(dispatcher.drums_snapshot().kit.as_deref(), Some("one"));
}

#[test]
fn selecting_a_groove_hands_it_to_the_runtime() {
    let (dispatcher, calls) = dispatcher();
    run(
        &dispatcher,
        DrumsCommand::SelectDrumGroove {
            groove: "funk-01".into(),
        },
    )
    .expect("groove");

    assert_eq!(*calls.borrow(), vec!["groove funk-01".to_string()]);
    assert_eq!(
        dispatcher.drums_snapshot().groove.as_deref(),
        Some("funk-01")
    );
}

#[test]
fn choosing_an_output_follows_it() {
    let (dispatcher, calls) = dispatcher();
    let key = Some("main\u{1f}Out 3/4".to_string());
    let events = run(
        &dispatcher,
        DrumsCommand::SetDrumsOutput {
            output_key: key.clone(),
        },
    )
    .expect("output");

    assert_eq!(
        *calls.borrow(),
        vec!["refresh on main\u{1f}Out 3/4".to_string()]
    );
    assert_eq!(dispatcher.drums_snapshot().output_key, key);
    assert_eq!(events, vec![Event::DrumsOutputChanged { output_key: key }]);
}

#[test]
fn drum_commands_never_touch_a_chain() {
    let (dispatcher, calls) = dispatcher();
    run(&dispatcher, DrumsCommand::PlayDrums).expect("play");
    run(&dispatcher, DrumsCommand::TriggerDrumFill).expect("fill");
    set_project_bpm(&dispatcher, 90.0);
    run(&dispatcher, DrumsCommand::StopDrums).expect("stop");
    assert!(calls.borrow().iter().all(|c| !c.starts_with("sync chain")));
}

#[test]
fn without_a_runtime_the_commands_still_record_their_state() {
    let dispatcher = LocalDispatcher::new(Rc::new(RefCell::new(Project::default())));
    dispatcher.attach_drums_state(Rc::new(RefCell::new(DrumsControlState::restored(
        &DrumsConfig::default(),
        library(),
        None,
    ))));

    run(&dispatcher, DrumsCommand::PlayDrums).expect("play");
    set_project_bpm(&dispatcher, 140.0);

    let snapshot = dispatcher.drums_snapshot();
    assert!(snapshot.playing);
    assert_eq!(snapshot.bpm, 140.0);
}

#[test]
fn a_tempo_that_is_not_a_number_falls_back_to_the_default() {
    let config = DrumsConfig::default();
    let mut state = DrumsControlState::restored(&config, library(), None);
    assert_eq!(state.set_bpm(f32::NAN), feature_dsp::metronome::BPM_DEFAULT);
}
