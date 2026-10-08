//! #398: a scheme picked in Settings → Appearance outlives the click — on the
//! bus while a project is open, in `config.yaml` from the launcher, and in the
//! boot snapshot a later wholesale re-save writes back.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, RwLock};

use application::command::{Command, SettingsCommand};
use application::dispatcher::CommandDispatcher;
use application::event::Event;
use application::SelectionState;
use infra_filesystem::{AppConfig, Appearance, FilesystemStorage};
use project::project::Project;

use super::appearance_pick::{mirror, record, save_at};
use crate::state::ProjectSession;

#[derive(Default)]
struct SpyDispatcher {
    seen: RefCell<Vec<Command>>,
    refuse: bool,
    selection: Arc<RwLock<SelectionState>>,
}

impl CommandDispatcher for SpyDispatcher {
    fn dispatch(&self, cmd: Command) -> anyhow::Result<Vec<Event>> {
        self.seen.borrow_mut().push(cmd);
        if self.refuse {
            anyhow::bail!("refused");
        }
        Ok(vec![])
    }

    fn selection_state(&self) -> Arc<RwLock<SelectionState>> {
        Arc::clone(&self.selection)
    }
}

fn session_on(spy: Rc<SpyDispatcher>) -> Rc<RefCell<Option<ProjectSession>>> {
    let project = Rc::new(RefCell::new(Project {
        name: None,
        device_settings: Vec::new(),
        chains: Vec::new(),
        midi: None,
    }));
    Rc::new(RefCell::new(Some(ProjectSession {
        project,
        dispatcher: spy,
        project_path: None,
        config_path: None,
        presets_path: PathBuf::from("./presets"),
        rig: None,
        io_bindings: Rc::new(RefCell::new(Vec::new())),
    })))
}

fn appearances_sent(spy: &SpyDispatcher) -> Vec<Appearance> {
    spy.seen
        .borrow()
        .iter()
        .filter_map(|cmd| match cmd {
            Command::Settings(SettingsCommand::SetAppearance { appearance }) => Some(*appearance),
            _ => None,
        })
        .collect()
}

#[test]
fn a_pick_with_a_project_open_goes_on_the_bus() {
    let spy = Rc::new(SpyDispatcher::default());
    let session = session_on(spy.clone());

    record(&session, Appearance::Dark);

    assert_eq!(appearances_sent(&spy), vec![Appearance::Dark]);
}

#[test]
fn a_refused_pick_is_sent_once_and_not_retried() {
    let spy = Rc::new(SpyDispatcher {
        refuse: true,
        ..SpyDispatcher::default()
    });
    let session = session_on(spy.clone());

    record(&session, Appearance::Light);

    assert_eq!(appearances_sent(&spy), vec![Appearance::Light]);
}

#[test]
fn a_launcher_pick_lands_in_the_config_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.yaml");

    save_at(path.clone(), Appearance::Light);
    application::persist_worker::flush();

    let config = FilesystemStorage::load_app_config_at(&path).unwrap();
    assert_eq!(config.appearance, Appearance::Light);
}

#[test]
fn a_launcher_pick_keeps_every_other_setting() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.yaml");
    FilesystemStorage::update_app_config_at(&path, |config| {
        config.language = Some("pt-BR".into());
    })
    .unwrap();

    save_at(path.clone(), Appearance::Dark);
    application::persist_worker::flush();

    let config = FilesystemStorage::load_app_config_at(&path).unwrap();
    assert_eq!(config.appearance, Appearance::Dark);
    assert_eq!(config.language.as_deref(), Some("pt-BR"));
}

#[test]
fn the_boot_snapshot_follows_the_pick() {
    let config = RefCell::new(AppConfig::default());

    mirror(&config, Appearance::Dark);
    assert_eq!(config.borrow().appearance, Appearance::Dark);

    mirror(&config, Appearance::System);
    assert_eq!(config.borrow().appearance, Appearance::System);
}
