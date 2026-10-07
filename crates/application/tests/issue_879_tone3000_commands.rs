//! #879: the TONE3000 browser is driven through the dispatcher — the key,
//! search, install and uninstall are `Command`s, so the screen and an MCP
//! client reach the same state. The user's key never leaves through a read.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use application::command::{Command, Tone3000Command};
use application::dispatcher::CommandDispatcher;
use application::event::{Event, Tone3000Event};
use application::local_dispatcher::LocalDispatcher;
use application::query_tone3000::tone3000_state_json;
use application::tone3000::api_client::{ApiError, Tone3000Api};
use application::tone3000::api_types::{Model, Page, Tone};
use application::tone3000::api_url::SearchQuery;
use application::tone3000::install::{install_tone, InstallRequest};
use application::tone3000::{Tone3000Format, Tone3000Gear, Tone3000Sort};
use application::tone3000_state::{Tone3000ApiFactory, Tone3000ControlState};
use infra_filesystem::{AppConfig, Tone3000Config};
use plugin_loader::manifest::BlockType;
use project::project::Project;

const KEY: &str = "t3k_cs_fixture_key_0123456789";

/// In-memory TONE3000 serving one IR tone whose captures are local wavs.
struct FakeApi {
    tone: Tone,
    models: Vec<Model>,
    files: BTreeMap<String, PathBuf>,
    fail_search: bool,
    searches: Mutex<Vec<SearchQuery>>,
}

impl Tone3000Api for FakeApi {
    fn search(&self, query: &SearchQuery) -> Result<Page<Tone>, ApiError> {
        self.searches.lock().unwrap().push(query.clone());
        if self.fail_search {
            return Err(ApiError::InvalidKey);
        }
        Ok(Page {
            data: vec![self.tone.clone()],
            page: query.page,
            page_size: query.page_size,
            total: 1,
            total_pages: 1,
        })
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

fn model(id: u64, name: &str, url: &str) -> Model {
    serde_json::from_value(serde_json::json!({
        "id": id, "tone_id": 1, "name": name, "model_url": url
    }))
    .unwrap()
}

/// Every test uses its own tone id: the plugin registry is process-wide.
fn ir_api(assets: &Path, tone_id: u64) -> FakeApi {
    let tone: Tone = serde_json::from_value(serde_json::json!({
        "id": tone_id, "title": "Fake Cab", "format": "ir", "gear": "cab", "irs_count": 2
    }))
    .unwrap();
    let loud = assets.join("loud.wav");
    let soft = assets.join("soft.wav");
    write_ir(&loud, 0.5);
    write_ir(&soft, 0.125);
    FakeApi {
        tone,
        models: vec![
            model(1, "Cab SM57 Cap", "https://cdn/ir/loud.wav"),
            model(2, "Cab SM57 Cone", "https://cdn/ir/soft.wav"),
        ],
        files: BTreeMap::from([
            ("https://cdn/ir/loud.wav".to_string(), loud),
            ("https://cdn/ir/soft.wav".to_string(), soft),
        ]),
        fail_search: false,
        searches: Mutex::new(Vec::new()),
    }
}

struct Rig {
    dispatcher: LocalDispatcher,
    keys: Arc<Mutex<Vec<String>>>,
    api: Arc<FakeApi>,
}

fn factory(api: Arc<FakeApi>, keys: Arc<Mutex<Vec<String>>>) -> Tone3000ApiFactory {
    Arc::new(move |key: &str| -> Arc<dyn Tone3000Api> {
        keys.lock().unwrap().push(key.to_string());
        api.clone()
    })
}

fn rig_with(
    api: FakeApi,
    config: &Tone3000Config,
    config_path: Option<PathBuf>,
    root: Option<PathBuf>,
) -> Rig {
    let api = Arc::new(api);
    let keys = Arc::new(Mutex::new(Vec::new()));
    let state = Tone3000ControlState::restored(config, config_path, root)
        .with_api_factory(factory(Arc::clone(&api), Arc::clone(&keys)));
    let dispatcher = LocalDispatcher::new(Rc::new(RefCell::new(Project::default())));
    dispatcher.attach_tone3000_state(Rc::new(RefCell::new(state)));
    Rig {
        dispatcher,
        keys,
        api,
    }
}

fn keyed_rig(api: FakeApi, root: &Path) -> Rig {
    let config = Tone3000Config {
        api_key: Some(KEY.into()),
    };
    rig_with(api, &config, None, Some(root.to_path_buf()))
}

fn run(dispatcher: &LocalDispatcher, cmd: Tone3000Command) -> anyhow::Result<Vec<Event>> {
    dispatcher.dispatch(Command::Tone3000(cmd))
}

fn search(query: &str) -> Tone3000Command {
    Tone3000Command::SearchTone3000 {
        query: query.into(),
        page: 1,
        format: Some(Tone3000Format::Ir),
        gear: Some(Tone3000Gear::Cab),
        sort: Some(Tone3000Sort::Trending),
    }
}

fn install(tone_id: u64) -> Tone3000Command {
    Tone3000Command::InstallTone3000 {
        tone_id,
        architecture: None,
        block_type: None,
    }
}

/// Drains the worker results until one event matches, the way the
/// frontend tick does.
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

fn tone3000_events(events: Vec<Event>) -> Vec<Tone3000Event> {
    events
        .into_iter()
        .filter_map(|e| match e {
            Event::Tone3000(e) => Some(e),
            _ => None,
        })
        .collect()
}

fn saved_config(path: &Path) -> AppConfig {
    application::persist_worker::flush();
    serde_yaml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn the_key_is_kept_in_the_system_config() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.yaml");
    let rig = rig_with(
        ir_api(dir.path(), 101),
        &Tone3000Config::default(),
        Some(path.clone()),
        None,
    );

    let events = run(
        &rig.dispatcher,
        Tone3000Command::SetTone3000ApiKey {
            key: format!("  {KEY}  "),
        },
    )
    .unwrap();

    assert_eq!(
        tone3000_events(events),
        [Tone3000Event::KeyChanged { configured: true }]
    );
    assert!(rig.dispatcher.tone3000_snapshot().key_configured);
    assert_eq!(saved_config(&path).tone3000.api_key.as_deref(), Some(KEY));
}

#[test]
fn an_empty_key_clears_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.yaml");
    let config = Tone3000Config {
        api_key: Some(KEY.into()),
    };
    let rig = rig_with(ir_api(dir.path(), 102), &config, Some(path.clone()), None);
    assert!(rig.dispatcher.tone3000_snapshot().key_configured);

    let events = run(
        &rig.dispatcher,
        Tone3000Command::SetTone3000ApiKey { key: "  ".into() },
    )
    .unwrap();

    assert_eq!(
        tone3000_events(events),
        [Tone3000Event::KeyChanged { configured: false }]
    );
    assert!(!rig.dispatcher.tone3000_snapshot().key_configured);
    assert_eq!(saved_config(&path).tone3000.api_key, None);
}

#[test]
fn the_key_never_leaves_through_a_read_or_a_log() {
    let dir = tempfile::tempdir().unwrap();
    let rig = keyed_rig(ir_api(dir.path(), 103), dir.path());
    let snapshot = rig.dispatcher.tone3000_snapshot();
    assert!(!tone3000_state_json(&snapshot).contains(KEY));
    assert!(!format!("{snapshot:?}").contains(KEY));
    let command = Command::Tone3000(Tone3000Command::SetTone3000ApiKey { key: KEY.into() });
    assert!(!format!("{command:?}").contains(KEY));
    let config = Tone3000Config {
        api_key: Some(KEY.into()),
    };
    assert!(!format!("{config:?}").contains(KEY));
}

#[test]
fn searching_without_a_key_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let rig = rig_with(
        ir_api(dir.path(), 104),
        &Tone3000Config::default(),
        None,
        Some(dir.path().to_path_buf()),
    );
    assert!(run(&rig.dispatcher, search("cab")).is_err());
    assert!(rig.api.searches.lock().unwrap().is_empty());
}

#[test]
fn search_results_land_in_the_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let rig = keyed_rig(ir_api(dir.path(), 105), dir.path());

    run(&rig.dispatcher, search("mesa cab")).unwrap();
    assert!(rig.dispatcher.tone3000_snapshot().search.in_flight);

    let events = wait_for(&rig.dispatcher, |e| {
        matches!(e, Tone3000Event::SearchFinished { .. })
    });
    assert!(events.contains(&Tone3000Event::SearchFinished { total: 1 }));

    let snapshot = rig.dispatcher.tone3000_snapshot();
    assert!(!snapshot.search.in_flight);
    assert_eq!(snapshot.search.error, None);
    let results = snapshot.search.results.expect("results");
    assert_eq!(results.data[0].id, 105);
    assert_eq!(snapshot.search.query.unwrap().query, "mesa cab");

    let sent = rig.api.searches.lock().unwrap()[0].clone();
    assert_eq!(sent.query, "mesa cab");
    assert_eq!(sent.format, Some(Tone3000Format::Ir));
    assert_eq!(sent.gear, Some(Tone3000Gear::Cab));
    assert_eq!(sent.sort, Some(Tone3000Sort::Trending));
    assert_eq!(*rig.keys.lock().unwrap(), [KEY]);
}

#[test]
fn a_failed_search_is_reported() {
    let dir = tempfile::tempdir().unwrap();
    let mut api = ir_api(dir.path(), 106);
    api.fail_search = true;
    let rig = keyed_rig(api, dir.path());

    run(&rig.dispatcher, search("x")).unwrap();
    let events = wait_for(&rig.dispatcher, |e| {
        matches!(e, Tone3000Event::SearchFailed { .. })
    });
    assert!(events
        .iter()
        .any(|e| matches!(e, Tone3000Event::SearchFailed { .. })));

    let snapshot = rig.dispatcher.tone3000_snapshot();
    assert!(!snapshot.search.in_flight);
    assert!(snapshot.search.error.is_some());
    assert!(snapshot.search.results.is_none());
}

#[test]
fn an_install_reports_progress_then_lists_the_plugin() {
    let assets = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let rig = keyed_rig(ir_api(assets.path(), 107), root.path());

    run(&rig.dispatcher, install(107)).unwrap();
    assert_eq!(rig.dispatcher.tone3000_snapshot().installs[0].tone_id, 107);

    let events = wait_for(&rig.dispatcher, |e| {
        matches!(e, Tone3000Event::Installed { .. })
    });
    assert!(events
        .iter()
        .any(|e| matches!(e, Tone3000Event::InstallProgress { tone_id: 107, .. })));
    assert!(events.contains(&Tone3000Event::Installed {
        tone_id: 107,
        plugin_id: "tone3000_107".into()
    }));

    let snapshot = rig.dispatcher.tone3000_snapshot();
    assert!(snapshot.installs.is_empty());
    let entry = &snapshot.installed[0];
    assert_eq!(entry.plugin_id, "tone3000_107");
    assert_eq!(entry.tone_ids, vec![107]);
    assert!(entry.removable);
    assert_eq!(entry.display_name, "Fake Cab");
    assert_eq!(entry.block_type, BlockType::Cab);
    assert_eq!(entry.captures, 2);
    assert!(root.path().join("tone3000_107/manifest.yaml").is_file());
    assert!(plugin_loader::registry::find("tone3000_107").is_some());
}

#[test]
fn a_tone_already_installing_is_refused() {
    let assets = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let rig = keyed_rig(ir_api(assets.path(), 108), root.path());

    run(&rig.dispatcher, install(108)).unwrap();
    assert!(run(&rig.dispatcher, install(108)).is_err());
    wait_for(&rig.dispatcher, |e| {
        matches!(e, Tone3000Event::Installed { .. })
    });
}

#[test]
fn installing_an_installed_tone_fails() {
    let assets = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let rig = keyed_rig(ir_api(assets.path(), 109), root.path());
    run(&rig.dispatcher, install(109)).unwrap();
    wait_for(&rig.dispatcher, |e| {
        matches!(e, Tone3000Event::Installed { .. })
    });

    run(&rig.dispatcher, install(109)).unwrap();
    let events = wait_for(&rig.dispatcher, |e| {
        matches!(e, Tone3000Event::InstallFailed { .. })
    });
    assert!(events
        .iter()
        .any(|e| matches!(e, Tone3000Event::InstallFailed { tone_id: 109, .. })));
    let snapshot = rig.dispatcher.tone3000_snapshot();
    assert_eq!(snapshot.installed.len(), 1);
    assert!(snapshot.installs[0].error.is_some());
}

#[test]
fn uninstall_removes_the_package_and_the_catalog_entry() {
    let assets = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let rig = keyed_rig(ir_api(assets.path(), 110), root.path());
    run(&rig.dispatcher, install(110)).unwrap();
    wait_for(&rig.dispatcher, |e| {
        matches!(e, Tone3000Event::Installed { .. })
    });

    let events = run(
        &rig.dispatcher,
        Tone3000Command::UninstallTone3000 {
            plugin_id: "tone3000_110".into(),
        },
    )
    .unwrap();

    assert_eq!(
        tone3000_events(events),
        [Tone3000Event::Uninstalled {
            plugin_id: "tone3000_110".into()
        }]
    );
    assert!(rig.dispatcher.tone3000_snapshot().installed.is_empty());
    assert!(!root.path().join("tone3000_110").exists());
    assert!(plugin_loader::registry::find("tone3000_110").is_none());
}

#[test]
fn uninstalling_a_foreign_id_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let rig = keyed_rig(ir_api(dir.path(), 111), dir.path());
    let refused = run(
        &rig.dispatcher,
        Tone3000Command::UninstallTone3000 {
            plugin_id: "../plugins".into(),
        },
    );
    assert!(refused.is_err());
}

#[test]
fn a_restored_state_lists_what_is_already_installed() {
    let assets = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let request = InstallRequest {
        tone_id: 112,
        architecture: None,
        block_type: None,
    };
    install_tone(
        &ir_api(assets.path(), 112),
        root.path(),
        &request,
        &mut |_| {},
    )
    .unwrap();

    let rig = keyed_rig(ir_api(assets.path(), 112), root.path());
    let installed = rig.dispatcher.tone3000_snapshot().installed;
    assert_eq!(installed.len(), 1);
    assert_eq!(installed[0].plugin_id, "tone3000_112");
}

#[test]
fn an_unattached_dispatcher_cannot_install() {
    let dispatcher = LocalDispatcher::new(Rc::new(RefCell::new(Project::default())));
    let snapshot = dispatcher.tone3000_snapshot();
    assert!(!snapshot.key_configured && !snapshot.can_install);
    assert!(run(&dispatcher, install(1)).is_err());
    assert!(run(&dispatcher, search("x")).is_err());
}

#[test]
fn the_read_serves_the_browser_state() {
    let dir = tempfile::tempdir().unwrap();
    let rig = keyed_rig(ir_api(dir.path(), 113), dir.path());
    let json: serde_json::Value =
        serde_json::from_str(&tone3000_state_json(&rig.dispatcher.tone3000_snapshot())).unwrap();
    assert_eq!(json["key_configured"], true);
    assert_eq!(json["can_install"], true);
    assert!(json["installed"].as_array().unwrap().is_empty());
    assert_eq!(json["search"]["in_flight"], false);
}
