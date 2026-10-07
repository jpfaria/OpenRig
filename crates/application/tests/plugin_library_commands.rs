//! The plugin library is driven through the dispatcher: editing a plugin's
//! parameters, restoring a version, reading the names again, removing and
//! creating a plugin are `Command`s, so the screen and an MCP client reach
//! the same state. Only plugins the user owns are listed or touched.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use application::command::{Command, PluginLibraryCommand};
use application::dispatcher::CommandDispatcher;
use application::event::{Event, PluginLibraryEvent};
use application::local_dispatcher::LocalDispatcher;
use application::plugin_library::{
    CaptureBackend, ColumnKind, EditorGrid, GridColumn, GridRow, PluginRoots,
};
use application::query_plugin_library::{plugin_grid_json, plugin_library_json};
use application::tone3000::api_client::{ApiError, Tone3000Api};
use application::tone3000::api_types::{Model, Page, Tone};
use application::tone3000::api_url::SearchQuery;
use application::tone3000::axes::{infer_axes, CaptureKind};
use application::tone3000::install::{install_tone, InstallRequest};
use application::tone3000::source_stamp::read_updated_at;
use application::tone3000::Tone3000BlockType;
use application::tone3000_state::{Tone3000ApiFactory, Tone3000ControlState};
use domain::ids::{BlockId, ChainId};
use domain::value_objects::ParameterValue;
use infra_filesystem::Tone3000Config;
use plugin_loader::manifest::{Backend, PluginManifest};
use project::block::{AudioBlock, AudioBlockKind, CoreBlock};
use project::chain::Chain;
use project::param::ParameterSet;
use project::project::Project;
use serde_json::Value;

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

/// The roots a session sees: the user's plugins folder (TONE3000 installs
/// go there too) and a folder standing for the plugins the app ships.
struct Folders {
    _tmp: tempfile::TempDir,
    plugins: PathBuf,
    shipped: PathBuf,
    sources: PathBuf,
}

fn folders() -> Folders {
    let tmp = tempfile::tempdir().unwrap();
    let make = |name: &str| {
        let dir = tmp.path().join(name);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    };
    Folders {
        plugins: make("plugins"),
        shipped: make("shipped"),
        sources: make("sources"),
        _tmp: tmp,
    }
}

/// An IR cab whose two captures are told apart by a `preset` axis, the way
/// an install leaves a tone whose names carry no knob settings. Loaded into
/// the (process-wide) catalog, so every test uses its own id.
fn preset_cab(parent: &Path, id: &str) -> PathBuf {
    let dir = parent.join(id);
    std::fs::create_dir_all(dir.join("captures")).unwrap();
    write_ir(&dir.join("captures/000.wav"), 0.5);
    write_ir(&dir.join("captures/001.wav"), 0.125);
    let yaml = format!(
        "manifest_version: 1
id: {id}
display_name: Preset Cab
type: cab
backend: ir
parameters:
  - name: preset
    display_name: Preset
    values: [SM57 Cap, SM57 Cone]
captures:
  - values: {{ preset: SM57 Cap }}
    file: captures/000.wav
    output_gain_db: -3.0
  - values: {{ preset: SM57 Cone }}
    file: captures/001.wav
    output_gain_db: 2.0
"
    );
    std::fs::write(dir.join("manifest.yaml"), yaml).unwrap();
    let _ = plugin_loader::registry::unload(id);
    plugin_loader::registry::load_one(id, &[parent.to_path_buf()]).unwrap();
    dir
}

struct Session {
    dispatcher: LocalDispatcher,
    project: Rc<RefCell<Project>>,
}

impl std::ops::Deref for Session {
    type Target = LocalDispatcher;
    fn deref(&self) -> &LocalDispatcher {
        &self.dispatcher
    }
}

fn dispatcher(folders: &Folders, project: Project) -> Session {
    let project = Rc::new(RefCell::new(project));
    let dispatcher = LocalDispatcher::new(project.clone());
    let state = Tone3000ControlState::restored(
        &Tone3000Config::default(),
        None,
        Some(folders.plugins.clone()),
    );
    dispatcher.attach_tone3000_state(Rc::new(RefCell::new(state)));
    Session {
        dispatcher,
        project,
    }
}

fn run(dispatcher: &LocalDispatcher, cmd: PluginLibraryCommand) -> anyhow::Result<Vec<Event>> {
    dispatcher.dispatch(Command::PluginLibrary(cmd))
}

fn library_events(events: &[Event]) -> Vec<PluginLibraryEvent> {
    events
        .iter()
        .filter_map(|e| match e {
            Event::PluginLibrary(e) => Some(e.clone()),
            _ => None,
        })
        .collect()
}

/// Drains worker results until a library event matches.
fn wait_for(
    dispatcher: &LocalDispatcher,
    done: impl Fn(&PluginLibraryEvent) -> bool,
) -> Vec<Event> {
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut seen = Vec::new();
    while Instant::now() < deadline {
        seen.extend(dispatcher.poll_async_results());
        if library_events(&seen).iter().any(&done) {
            return seen;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("timed out; saw {seen:?}");
}

fn read_manifest(dir: &Path) -> PluginManifest {
    serde_yaml::from_str(&std::fs::read_to_string(dir.join("manifest.yaml")).unwrap()).unwrap()
}

fn axis_names(manifest: &PluginManifest) -> Vec<String> {
    match &manifest.backend {
        Backend::Ir { parameters, .. } | Backend::Nam { parameters, .. } => {
            parameters.iter().map(|p| p.name.clone()).collect()
        }
        _ => Vec::new(),
    }
}

fn row(file: &str, cells: &[&str]) -> GridRow {
    GridRow {
        file: file.into(),
        source_name: String::new(),
        cells: cells.iter().map(|c| c.to_string()).collect(),
    }
}

fn column(name: &str, kind: ColumnKind) -> GridColumn {
    GridColumn {
        name: name.into(),
        display_name: None,
        kind,
    }
}

/// The user's naming of the two captures: a `distance` knob.
fn distance_grid() -> EditorGrid {
    EditorGrid {
        columns: vec![column("distance", ColumnKind::Knob)],
        rows: vec![
            row("captures/000.wav", &["0"]),
            row("captures/001.wav", &["2"]),
        ],
    }
}

fn save(id: &str, grid: EditorGrid) -> PluginLibraryCommand {
    PluginLibraryCommand::SavePluginParameters {
        plugin_id: id.into(),
        grid,
    }
}

fn block(model: &str, preset: &str) -> AudioBlock {
    let mut params = ParameterSet::default();
    params.insert("preset", ParameterValue::String(preset.into()));
    params.insert("output_db", ParameterValue::Float(-1.5));
    AudioBlock {
        id: BlockId("cab-1".into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: block_core::EFFECT_TYPE_CAB.into(),
            model: model.into(),
            params,
        }),
    }
}

fn project_with(block: AudioBlock) -> Project {
    Project {
        chains: vec![Chain {
            id: ChainId("chain-1".into()),
            description: None,
            instrument: "electric_guitar".into(),
            enabled: false,
            volume: 100.0,
            io_binding_ids: vec![],
            blocks: vec![block],
            loopers: vec![],
            di_output: None,
            disabled_endpoints: Default::default(),
            mix: Default::default(),
        }],
        ..Project::default()
    }
}

fn block_params(session: &Session) -> ParameterSet {
    let project = session.project.borrow();
    match &project.chains[0].blocks[0].kind {
        AudioBlockKind::Core(core) => core.params.clone(),
        other => panic!("unexpected block {other:?}"),
    }
}

fn library(dispatcher: &LocalDispatcher) -> Vec<Value> {
    let json: Value =
        serde_json::from_str(&plugin_library_json(&dispatcher.plugin_library_roots())).unwrap();
    json["plugins"].as_array().unwrap().clone()
}

fn grid_view(dispatcher: &LocalDispatcher, id: &str) -> Value {
    serde_json::from_str(&plugin_grid_json(&dispatcher.plugin_library_roots(), id).unwrap())
        .unwrap()
}

#[test]
fn the_library_lists_only_the_plugins_the_user_owns() {
    let f = folders();
    preset_cab(&f.plugins.join("ir"), "lib_list_folder");
    preset_cab(&f.plugins.join("ir"), "tone3000_910001");
    preset_cab(&f.shipped, "lib_list_shipped");
    let d = dispatcher(&f, Project::default());

    let listed = library(&d);
    let origin = |id: &str| {
        listed
            .iter()
            .find(|e| e["plugin_id"] == id)
            .map(|e| e["origin"].as_str().unwrap().to_string())
    };

    assert_eq!(origin("lib_list_folder").as_deref(), Some("plugins_folder"));
    assert_eq!(origin("tone3000_910001").as_deref(), Some("tone3000"));
    assert_eq!(
        origin("lib_list_shipped"),
        None,
        "shipped plugins are hidden"
    );
    let entry = listed
        .iter()
        .find(|e| e["plugin_id"] == "lib_list_folder")
        .unwrap();
    assert_eq!(entry["captures"], 2);
    assert_eq!(entry["editable"], true);
    assert_eq!(entry["backend"], "ir");
}

#[test]
fn the_grid_has_a_row_per_capture_and_a_column_per_parameter() {
    let f = folders();
    preset_cab(&f.plugins, "lib_grid_read");
    let d = dispatcher(&f, Project::default());

    let view = grid_view(&d, "lib_grid_read");

    let columns = view["grid"]["columns"].as_array().unwrap();
    assert_eq!(columns.len(), 1);
    assert_eq!(columns[0]["name"], "preset");
    assert_eq!(columns[0]["kind"], "choice");
    let rows = view["grid"]["rows"].as_array().unwrap();
    assert_eq!(rows[1]["file"], "captures/001.wav");
    assert_eq!(
        rows[1]["source_name"], "001",
        "no recorded name: the file stem"
    );
    assert_eq!(rows[1]["cells"][0], "SM57 Cone");
    assert_eq!(view["versions"].as_array().unwrap().len(), 0);
}

#[test]
fn a_plugin_the_user_does_not_own_has_no_grid() {
    let f = folders();
    preset_cab(&f.shipped, "lib_grid_shipped");
    let d = dispatcher(&f, Project::default());
    assert!(plugin_grid_json(&d.plugin_library_roots(), "lib_grid_shipped").is_err());
}

#[test]
fn saving_renames_the_parameters_and_keeps_every_capture_file() {
    let f = folders();
    let dir = preset_cab(&f.plugins, "lib_save");
    let before = read_manifest(&dir);
    let wav = std::fs::read(dir.join("captures/001.wav")).unwrap();
    let d = dispatcher(&f, Project::default());

    let events = run(&d, save("lib_save", distance_grid())).unwrap();

    assert_eq!(
        library_events(&events),
        [PluginLibraryEvent::Saved {
            plugin_id: "lib_save".into(),
            version: 2
        }]
    );
    let after = read_manifest(&dir);
    assert_eq!(axis_names(&after), ["distance"]);
    assert_eq!(
        std::fs::read(dir.join("captures/001.wav")).unwrap(),
        wav,
        "capture files never move"
    );
    let gains = |m: &PluginManifest| match &m.backend {
        Backend::Ir { captures, .. } => captures
            .iter()
            .map(|c| (c.file.clone(), c.output_gain_db))
            .collect::<Vec<_>>(),
        _ => panic!("ir"),
    };
    assert_eq!(gains(&after), gains(&before), "levels stay with their file");
    let versions = plugin_loader::version_store::version_numbers(&dir);
    assert_eq!(versions, [1, 2]);
    assert_eq!(
        plugin_loader::version_store::read_version(&dir, 1).unwrap(),
        before
    );
    let loaded = plugin_loader::registry::find("lib_save").unwrap();
    assert_eq!(
        axis_names(&loaded.manifest),
        ["distance"],
        "catalog reloaded"
    );
}

#[test]
fn a_grid_that_cannot_pick_one_capture_is_refused_and_nothing_is_written() {
    let f = folders();
    let dir = preset_cab(&f.plugins, "lib_refuse");
    let before = std::fs::read_to_string(dir.join("manifest.yaml")).unwrap();
    let d = dispatcher(&f, Project::default());
    let mut same = distance_grid();
    same.rows[1].cells = vec!["0".into()];
    let mut not_a_number = distance_grid();
    not_a_number.rows[0].cells = vec!["far".into()];
    let mut missing_row = distance_grid();
    missing_row.rows.pop();
    let mut foreign_file = distance_grid();
    foreign_file.rows[0].file = "../elsewhere.wav".into();
    let mut engine_name = distance_grid();
    engine_name.columns[0].name = "output_db".into();

    for grid in [same, not_a_number, missing_row, foreign_file, engine_name] {
        assert!(
            run(&d, save("lib_refuse", grid.clone())).is_err(),
            "{grid:?}"
        );
    }
    assert_eq!(
        std::fs::read_to_string(dir.join("manifest.yaml")).unwrap(),
        before
    );
    assert!(plugin_loader::version_store::version_numbers(&dir).is_empty());
}

#[test]
fn a_block_in_the_open_project_keeps_playing_the_same_capture() {
    let f = folders();
    preset_cab(&f.plugins, "lib_follow");
    let d = dispatcher(&f, project_with(block("lib_follow", "SM57 Cone")));

    let events = run(&d, save("lib_follow", distance_grid())).unwrap();

    let params = block_params(&d);
    assert_eq!(params.get("distance"), Some(&ParameterValue::Float(2.0)));
    assert_eq!(params.get("preset"), None);
    assert_eq!(params.get("output_db"), Some(&ParameterValue::Float(-1.5)));
    assert!(events.iter().any(|e| matches!(
        e,
        Event::BlockParameterChanged { chain, block, .. }
            if chain.0 == "chain-1" && block.0 == "cab-1"
    )));
}

#[test]
fn restoring_a_version_saves_it_as_the_newest() {
    let f = folders();
    let dir = preset_cab(&f.plugins, "lib_restore");
    let original = read_manifest(&dir);
    let d = dispatcher(&f, project_with(block("lib_restore", "SM57 Cone")));
    run(&d, save("lib_restore", distance_grid())).unwrap();

    let events = run(
        &d,
        PluginLibraryCommand::RestorePluginVersion {
            plugin_id: "lib_restore".into(),
            version: 1,
        },
    )
    .unwrap();

    assert_eq!(
        library_events(&events),
        [PluginLibraryEvent::Restored {
            plugin_id: "lib_restore".into(),
            version: 3
        }]
    );
    assert_eq!(read_manifest(&dir), original);
    assert_eq!(
        plugin_loader::version_store::version_numbers(&dir),
        [1, 2, 3]
    );
    assert_eq!(
        block_params(&d).get("preset"),
        Some(&ParameterValue::String("SM57 Cone".into()))
    );
    assert!(run(
        &d,
        PluginLibraryCommand::RestorePluginVersion {
            plugin_id: "lib_restore".into(),
            version: 9,
        },
    )
    .is_err());
}

#[test]
fn redo_reads_the_capture_names_again() {
    let f = folders();
    let dir = preset_cab(&f.plugins, "lib_redo");
    let d = dispatcher(&f, Project::default());
    run(&d, save("lib_redo", distance_grid())).unwrap();

    let events = run(
        &d,
        PluginLibraryCommand::RedoPluginParameters {
            plugin_id: "lib_redo".into(),
        },
    )
    .unwrap();

    assert_eq!(
        library_events(&events),
        [PluginLibraryEvent::Redone {
            plugin_id: "lib_redo".into(),
            version: 3
        }]
    );
    let expected = infer_axes(&["000".to_string(), "001".to_string()], CaptureKind::Ir);
    let expected_names: Vec<String> = expected.parameters.iter().map(|p| p.name.clone()).collect();
    assert_eq!(axis_names(&read_manifest(&dir)), expected_names);
}

#[test]
fn uninstall_removes_the_package_from_disk_and_from_the_catalog() {
    let f = folders();
    let dir = preset_cab(&f.plugins, "lib_uninstall");
    let d = dispatcher(&f, Project::default());

    let events = run(
        &d,
        PluginLibraryCommand::UninstallPlugin {
            plugin_id: "lib_uninstall".into(),
        },
    )
    .unwrap();

    assert_eq!(
        library_events(&events),
        [PluginLibraryEvent::Uninstalled {
            plugin_id: "lib_uninstall".into()
        }]
    );
    assert!(!dir.exists());
    assert!(plugin_loader::registry::find("lib_uninstall").is_none());
}

#[test]
fn uninstall_refuses_a_plugin_the_user_does_not_own() {
    let f = folders();
    let dir = preset_cab(&f.shipped, "lib_uninstall_shipped");
    let d = dispatcher(&f, Project::default());

    assert!(run(
        &d,
        PluginLibraryCommand::UninstallPlugin {
            plugin_id: "lib_uninstall_shipped".into(),
        },
    )
    .is_err());
    assert!(dir.join("manifest.yaml").is_file());
    assert!(plugin_loader::registry::find("lib_uninstall_shipped").is_some());
}

#[test]
fn a_new_plugin_is_built_from_capture_files_in_the_plugins_folder() {
    let f = folders();
    let loud = f.sources.join("Room Close.wav");
    let soft = f.sources.join("Room Far.wav");
    write_ir(&loud, 0.5);
    write_ir(&soft, 0.125);
    let d = dispatcher(&f, Project::default());
    let grid = EditorGrid {
        columns: vec![column("distance", ColumnKind::Knob)],
        rows: vec![
            GridRow {
                file: loud.clone(),
                source_name: String::new(),
                cells: vec!["0".into()],
            },
            GridRow {
                file: soft.clone(),
                source_name: String::new(),
                cells: vec!["3".into()],
            },
        ],
    };

    run(
        &d,
        PluginLibraryCommand::CreatePlugin {
            display_name: "Lib Created Room".into(),
            brand: Some("me".into()),
            block_type: Tone3000BlockType::Cab,
            backend: CaptureBackend::Ir,
            grid,
        },
    )
    .unwrap();
    let events = wait_for(&d, |e| {
        matches!(
            e,
            PluginLibraryEvent::Created { .. } | PluginLibraryEvent::CreateFailed { .. }
        )
    });

    let Some(PluginLibraryEvent::Created { plugin_id }) = library_events(&events).pop() else {
        panic!("{events:?}");
    };
    let dir = f.plugins.join("ir").join(&plugin_id);
    let manifest = read_manifest(&dir);
    assert_eq!(manifest.display_name, "Lib Created Room");
    assert_eq!(axis_names(&manifest), ["distance"]);
    let Backend::Ir { captures, .. } = &manifest.backend else {
        panic!("ir");
    };
    assert_eq!(captures.len(), 2);
    for capture in captures {
        assert!(dir.join(&capture.file).is_file(), "{capture:?} copied in");
        assert!(capture.output_gain_db.is_some(), "levelled like an install");
    }
    assert!(loud.is_file(), "the source files stay where they were");
    assert!(plugin_loader::registry::find(&plugin_id).is_some());
}

#[test]
fn a_new_plugin_needs_a_name() {
    let f = folders();
    let d = dispatcher(&f, Project::default());
    assert!(run(
        &d,
        PluginLibraryCommand::CreatePlugin {
            display_name: "  ".into(),
            brand: None,
            block_type: Tone3000BlockType::Cab,
            backend: CaptureBackend::Ir,
            grid: EditorGrid {
                columns: vec![],
                rows: vec![],
            },
        },
    )
    .is_err());
}

// ── TONE3000 plugins ─────────────────────────────────────────────────────

struct FakeApi {
    tone: std::sync::Mutex<Tone>,
    models: Vec<Model>,
    files: BTreeMap<String, PathBuf>,
}

impl Tone3000Api for FakeApi {
    fn search(&self, _query: &SearchQuery) -> Result<Page<Tone>, ApiError> {
        Err(ApiError::Http(404))
    }

    fn tone(&self, id: u64) -> Result<Tone, ApiError> {
        let tone = self.tone.lock().unwrap().clone();
        if id == tone.id {
            Ok(tone)
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

fn tone(id: u64, updated_at: &str) -> Tone {
    serde_json::from_value(serde_json::json!({
        "id": id, "title": "Fake Cab", "format": "ir", "gear": "cab", "irs_count": 2,
        "updated_at": updated_at
    }))
    .unwrap()
}

fn model(id: u64, name: &str, url: &str) -> Model {
    serde_json::from_value(serde_json::json!({
        "id": id, "tone_id": 1, "name": name, "model_url": url
    }))
    .unwrap()
}

fn fake_api(f: &Folders, tone_id: u64) -> std::sync::Arc<FakeApi> {
    let loud = f.sources.join("loud.wav");
    let soft = f.sources.join("soft.wav");
    write_ir(&loud, 0.5);
    write_ir(&soft, 0.125);
    std::sync::Arc::new(FakeApi {
        tone: std::sync::Mutex::new(tone(tone_id, "2026-05-01T10:00:00Z")),
        models: vec![
            model(1, "Cab SM57 Cap", "https://cdn/ir/loud.wav"),
            model(2, "Cab SM57 Cone", "https://cdn/ir/soft.wav"),
        ],
        files: BTreeMap::from([
            ("https://cdn/ir/loud.wav".to_string(), loud),
            ("https://cdn/ir/soft.wav".to_string(), soft),
        ]),
    })
}

/// A session with a TONE3000 key whose API is `api`, and the tone already
/// installed in the plugins folder.
fn keyed_dispatcher(f: &Folders, api: std::sync::Arc<FakeApi>, tone_id: u64) -> LocalDispatcher {
    install_tone(
        api.as_ref(),
        &f.plugins,
        &InstallRequest {
            tone_id,
            architecture: None,
            block_type: None,
        },
        &mut |_| {},
    )
    .unwrap();
    let id = format!("tone3000_{tone_id}");
    let _ = plugin_loader::registry::unload(&id);
    plugin_loader::registry::load_one(&id, &[f.plugins.join("ir")]).unwrap();
    let factory: Tone3000ApiFactory = {
        let api = api.clone();
        std::sync::Arc::new(move |_key: &str| -> std::sync::Arc<dyn Tone3000Api> { api.clone() })
    };
    let state = Tone3000ControlState::restored(
        &Tone3000Config {
            api_key: Some("t3k_cs_fixture_key_0123456789".into()),
        },
        None,
        Some(f.plugins.clone()),
    )
    .with_api_factory(factory);
    let d = LocalDispatcher::new(Rc::new(RefCell::new(Project::default())));
    d.attach_tone3000_state(Rc::new(RefCell::new(state)));
    d
}

#[test]
fn an_installed_tone_shows_its_tone3000_capture_names() {
    let f = folders();
    let d = keyed_dispatcher(&f, fake_api(&f, 920001), 920001);

    let view = grid_view(&d, "tone3000_920001");

    let rows = view["grid"]["rows"].as_array().unwrap();
    assert_eq!(rows[0]["source_name"], "Cab SM57 Cap");
    assert_eq!(rows[1]["source_name"], "Cab SM57 Cone");
    assert_eq!(view["origin"], "tone3000");
}

#[test]
fn redo_of_a_current_tone_reads_its_names_from_tone3000() {
    let f = folders();
    let d = keyed_dispatcher(&f, fake_api(&f, 920002), 920002);
    let dir = f.plugins.join("ir/tone3000_920002");
    let installed = axis_names(&read_manifest(&dir));
    run(&d, save("tone3000_920002", distance_grid())).unwrap();

    run(
        &d,
        PluginLibraryCommand::RedoPluginParameters {
            plugin_id: "tone3000_920002".into(),
        },
    )
    .unwrap();
    let events = wait_for(&d, |e| {
        matches!(
            e,
            PluginLibraryEvent::Redone { .. } | PluginLibraryEvent::RedoFailed { .. }
        )
    });

    assert!(
        library_events(&events).contains(&PluginLibraryEvent::Redone {
            plugin_id: "tone3000_920002".into(),
            version: 3
        })
    );
    assert_eq!(axis_names(&read_manifest(&dir)), installed);
}

#[test]
fn redo_of_a_tone_changed_on_tone3000_downloads_it_again() {
    let f = folders();
    let api = fake_api(&f, 920003);
    let d = keyed_dispatcher(&f, api.clone(), 920003);
    let dir = f.plugins.join("ir/tone3000_920003");
    *api.tone.lock().unwrap() = tone(920003, "2026-06-01T10:00:00Z");

    run(
        &d,
        PluginLibraryCommand::RedoPluginParameters {
            plugin_id: "tone3000_920003".into(),
        },
    )
    .unwrap();
    wait_for(&d, |e| {
        matches!(
            e,
            PluginLibraryEvent::Redone { .. } | PluginLibraryEvent::RedoFailed { .. }
        )
    });

    assert_eq!(
        read_updated_at(&dir).as_deref(),
        Some("2026-06-01T10:00:00Z")
    );
}

#[test]
fn the_library_marks_a_tone3000_plugin_with_its_tone() {
    let f = folders();
    let d = keyed_dispatcher(&f, fake_api(&f, 920004), 920004);

    let entry = library(&d)
        .into_iter()
        .find(|e| e["plugin_id"] == "tone3000_920004")
        .unwrap();

    assert_eq!(entry["tone_ids"], serde_json::json!([920004]));
    assert_eq!(entry["updated_at"], "2026-05-01T10:00:00Z");
}

#[test]
fn the_owned_folder_is_the_plugins_folder_installs_go_into() {
    let f = folders();
    let d = dispatcher(&f, Project::default());
    assert_eq!(
        d.plugin_library_roots(),
        PluginRoots {
            plugins_folder: Some(f.plugins.clone()),
        }
    );
}

#[test]
fn a_plugin_naming_a_tone3000_tone_came_from_tone3000() {
    let f = folders();
    let dir = preset_cab(&f.plugins.join("ir"), "lib_sourced_cab");
    let yaml = std::fs::read_to_string(dir.join("manifest.yaml")).unwrap();
    std::fs::write(
        dir.join("manifest.yaml"),
        yaml.replace(
            "type: cab",
            "sources: [https://www.tone3000.com/tones/930001]\ntype: cab",
        ),
    )
    .unwrap();
    let _ = plugin_loader::registry::unload("lib_sourced_cab");
    plugin_loader::registry::load_one("lib_sourced_cab", &[f.plugins.join("ir")]).unwrap();
    let d = dispatcher(&f, Project::default());

    let entry = library(&d)
        .into_iter()
        .find(|e| e["plugin_id"] == "lib_sourced_cab")
        .unwrap();

    assert_eq!(entry["origin"], "tone3000");
    assert_eq!(entry["tone_ids"], serde_json::json!([930001]));
}
