//! An install whose capture names hold words the inference cannot read
//! stops before the package exists and shows its captures as a grid. The
//! user names the parameters, and only then is the package written.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use application::command::{Command, Tone3000Command};
use application::dispatcher::CommandDispatcher;
use application::event::{Event, Tone3000Event};
use application::local_dispatcher::LocalDispatcher;
use application::plugin_library::{ColumnKind, EditorGrid};
use application::tone3000::api_client::{ApiError, Tone3000Api};
use application::tone3000::api_types::{Model, Page, Tone};
use application::tone3000::api_url::SearchQuery;
use application::tone3000_state::{Tone3000ApiFactory, Tone3000ControlState};
use infra_filesystem::Tone3000Config;
use project::project::Project;

/// One IR tone whose captures are local wavs.
struct FakeApi {
    tone: Tone,
    models: Vec<Model>,
    files: BTreeMap<String, PathBuf>,
}

impl Tone3000Api for FakeApi {
    fn search(&self, _query: &SearchQuery) -> Result<Page<Tone>, ApiError> {
        Err(ApiError::Http(404))
    }

    fn tone(&self, id: u64) -> Result<Tone, ApiError> {
        if id == self.tone.id {
            Ok(self.tone.clone())
        } else {
            Err(ApiError::Http(404))
        }
    }

    fn models_page(
        &self,
        _tone_id: u64,
        _architecture: Option<application::tone3000::Tone3000Architecture>,
        page: u32,
    ) -> Result<Page<Model>, ApiError> {
        Ok(Page {
            data: self.models.clone(),
            page,
            page_size: 100,
            total: self.models.len() as u32,
            total_pages: 1,
        })
    }

    fn download(&self, url: &str, dest: &Path) -> Result<(), ApiError> {
        let source = self.files.get(url).ok_or(ApiError::Http(404))?;
        std::fs::copy(source, dest).map_err(|e| ApiError::Io(e.to_string()))?;
        Ok(())
    }
}

fn write_ir(path: &Path, gain: f32) {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 48_000,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create(path, spec).unwrap();
    for tap in [1.0_f32, 0.5, 0.25] {
        writer.write_sample(gain * tap).unwrap();
    }
    writer.finalize().unwrap();
}

/// Every test uses its own tone id: the plugin registry is process-wide.
fn api(assets: &Path, tone_id: u64, names: [&str; 2]) -> FakeApi {
    let tone: Tone = serde_json::from_value(serde_json::json!({
        "id": tone_id, "title": "Odd Cab", "format": "ir", "gear": "cab", "irs_count": 2,
        "updated_at": "2026-05-01T10:00:00Z"
    }))
    .unwrap();
    let mut models = Vec::new();
    let mut files = BTreeMap::new();
    for (i, name) in names.iter().enumerate() {
        let url = format!("https://cdn/ir/{i}.wav");
        let file = assets.join(format!("{i}.wav"));
        write_ir(&file, 0.5 / (i as f32 + 1.0));
        models.push(
            serde_json::from_value(serde_json::json!({
                "id": i + 1, "tone_id": tone_id, "name": name, "model_url": url
            }))
            .unwrap(),
        );
        files.insert(url, file);
    }
    FakeApi {
        tone,
        models,
        files,
    }
}

fn dispatcher(api: FakeApi, root: &Path) -> LocalDispatcher {
    let api: Arc<FakeApi> = Arc::new(api);
    let factory: Tone3000ApiFactory =
        Arc::new(move |_: &str| -> Arc<dyn Tone3000Api> { api.clone() });
    let config = Tone3000Config {
        api_key: Some("t3k_cs_fixture_key_0123456789".into()),
    };
    let state = Tone3000ControlState::restored(&config, None, Some(root.to_path_buf()))
        .with_api_factory(factory);
    let dispatcher = LocalDispatcher::new(Rc::new(RefCell::new(Project::default())));
    dispatcher.attach_tone3000_state(Rc::new(RefCell::new(state)));
    dispatcher
}

fn run(dispatcher: &LocalDispatcher, cmd: Tone3000Command) -> anyhow::Result<Vec<Event>> {
    dispatcher.dispatch(Command::Tone3000(cmd))
}

fn install(dispatcher: &LocalDispatcher, tone_id: u64) {
    run(
        dispatcher,
        Tone3000Command::InstallTone3000 {
            tone_id,
            architecture: None,
            block_type: None,
        },
    )
    .unwrap();
}

fn wait_for(
    dispatcher: &LocalDispatcher,
    done: impl Fn(&Tone3000Event) -> bool,
) -> Vec<Tone3000Event> {
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut seen = Vec::new();
    while Instant::now() < deadline {
        for event in dispatcher.poll_async_results() {
            if let Event::Tone3000(e) = event {
                seen.push(e);
            }
        }
        if seen.iter().any(&done) {
            return seen;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("timed out; saw {seen:?}");
}

/// Installs a tone with unreadable names and returns the grid it asks for.
fn asked_grid(dispatcher: &LocalDispatcher, tone_id: u64) -> EditorGrid {
    install(dispatcher, tone_id);
    let events = wait_for(dispatcher, |e| {
        matches!(
            e,
            Tone3000Event::NamesNeeded { .. }
                | Tone3000Event::Installed { .. }
                | Tone3000Event::InstallFailed { .. }
        )
    });
    events
        .into_iter()
        .find_map(|e| match e {
            Tone3000Event::NamesNeeded { grid, .. } => Some(grid),
            _ => None,
        })
        .expect("the install asked for names")
}

/// No package folder and no hidden leftover in the IR folder.
fn ir_folder_is_empty(root: &Path) -> bool {
    std::fs::read_dir(root.join("ir"))
        .map(|entries| entries.count() == 0)
        .unwrap_or(true)
}

#[test]
fn unreadable_names_stop_the_install_and_show_the_captures() {
    let assets = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let dispatcher = dispatcher(
        api(assets.path(), 961001, ["Box Zorbo", "Box Quimp"]),
        root.path(),
    );

    let grid = asked_grid(&dispatcher, 961001);

    assert_eq!(grid.columns.len(), 1);
    assert_eq!(grid.columns[0].name, "preset");
    assert_eq!(grid.columns[0].kind, ColumnKind::Choice);
    let mut names: Vec<&str> = grid.rows.iter().map(|r| r.source_name.as_str()).collect();
    names.sort();
    assert_eq!(names, ["Box Quimp", "Box Zorbo"]);
    assert!(!root.path().join("ir/tone3000_961001").exists());
    assert!(plugin_loader::registry::find("tone3000_961001").is_none());
    let snapshot = dispatcher.tone3000_snapshot();
    assert!(snapshot.installs.is_empty());
    assert!(snapshot.installed.is_empty());
    assert_eq!(snapshot.naming.len(), 1);
    assert_eq!(snapshot.naming[0].tone_id, 961001);
    assert_eq!(snapshot.naming[0].plugin_id, "tone3000_961001");
    assert_eq!(snapshot.naming[0].grid, grid);
}

#[test]
fn naming_the_parameters_installs_the_package() {
    let assets = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let dispatcher = dispatcher(
        api(assets.path(), 961002, ["Box Zorbo", "Box Quimp"]),
        root.path(),
    );
    let mut grid = asked_grid(&dispatcher, 961002);
    grid.columns[0].name = "mic".into();
    grid.columns[0].display_name = Some("Mic".into());
    for row in &mut grid.rows {
        row.cells = vec![if row.source_name == "Box Zorbo" {
            "sm57".into()
        } else {
            "md421".into()
        }];
    }

    let events = run(
        &dispatcher,
        Tone3000Command::FinishTone3000Install {
            tone_id: 961002,
            grid,
        },
    )
    .unwrap();

    assert!(events.contains(&Event::Tone3000(Tone3000Event::Installed {
        tone_id: 961002,
        plugin_id: "tone3000_961002".into(),
    })));
    let text =
        std::fs::read_to_string(root.path().join("ir/tone3000_961002/manifest.yaml")).unwrap();
    assert!(text.contains("name: mic"), "{text}");
    assert!(text.contains("md421"), "{text}");
    assert!(!text.contains("preset"), "{text}");
    assert!(plugin_loader::registry::find("tone3000_961002").is_some());
    let snapshot = dispatcher.tone3000_snapshot();
    assert!(snapshot.naming.is_empty());
    assert_eq!(snapshot.installed[0].plugin_id, "tone3000_961002");
    let leftovers: Vec<_> = std::fs::read_dir(root.path().join("ir"))
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(leftovers, ["tone3000_961002"]);
}

#[test]
fn a_grid_that_cannot_pick_a_capture_keeps_the_tone_waiting() {
    let assets = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let dispatcher = dispatcher(
        api(assets.path(), 961003, ["Box Zorbo", "Box Quimp"]),
        root.path(),
    );
    let mut grid = asked_grid(&dispatcher, 961003);
    for row in &mut grid.rows {
        row.cells = vec!["same".into()];
    }

    let finished = run(
        &dispatcher,
        Tone3000Command::FinishTone3000Install {
            tone_id: 961003,
            grid,
        },
    );

    assert!(finished.is_err());
    assert!(!root.path().join("ir/tone3000_961003").exists());
    assert_eq!(dispatcher.tone3000_snapshot().naming.len(), 1);
}

#[test]
fn canceling_leaves_nothing_on_disk() {
    let assets = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let dispatcher = dispatcher(
        api(assets.path(), 961004, ["Box Zorbo", "Box Quimp"]),
        root.path(),
    );
    asked_grid(&dispatcher, 961004);

    let events = run(
        &dispatcher,
        Tone3000Command::CancelTone3000Install { tone_id: 961004 },
    )
    .unwrap();

    assert_eq!(
        events,
        vec![Event::Tone3000(Tone3000Event::InstallCanceled {
            tone_id: 961004
        })]
    );
    assert!(ir_folder_is_empty(root.path()));
    assert!(dispatcher.tone3000_snapshot().naming.is_empty());
    assert!(plugin_loader::registry::find("tone3000_961004").is_none());
}

#[test]
fn a_tone_waiting_for_names_cannot_be_installed_again() {
    let assets = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let dispatcher = dispatcher(
        api(assets.path(), 961005, ["Box Zorbo", "Box Quimp"]),
        root.path(),
    );
    asked_grid(&dispatcher, 961005);

    let again = run(
        &dispatcher,
        Tone3000Command::InstallTone3000 {
            tone_id: 961005,
            architecture: None,
            block_type: None,
        },
    );

    assert!(again.is_err());
}

#[test]
fn finishing_a_tone_that_is_not_waiting_is_refused() {
    let assets = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let dispatcher = dispatcher(
        api(assets.path(), 961006, ["Box Zorbo", "Box Quimp"]),
        root.path(),
    );

    let grid = EditorGrid {
        columns: Vec::new(),
        rows: Vec::new(),
    };
    assert!(run(
        &dispatcher,
        Tone3000Command::FinishTone3000Install {
            tone_id: 961006,
            grid
        }
    )
    .is_err());
    assert!(run(
        &dispatcher,
        Tone3000Command::CancelTone3000Install { tone_id: 961006 }
    )
    .is_err());
}

#[test]
fn readable_names_install_without_asking() {
    let assets = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let dispatcher = dispatcher(
        api(assets.path(), 961007, ["Cab SM57 Cap", "Cab SM57 Cone"]),
        root.path(),
    );

    install(&dispatcher, 961007);
    let events = wait_for(&dispatcher, |e| {
        matches!(
            e,
            Tone3000Event::NamesNeeded { .. }
                | Tone3000Event::Installed { .. }
                | Tone3000Event::InstallFailed { .. }
        )
    });

    assert!(events.contains(&Tone3000Event::Installed {
        tone_id: 961007,
        plugin_id: "tone3000_961007".into(),
    }));
    assert!(dispatcher.tone3000_snapshot().naming.is_empty());
}
