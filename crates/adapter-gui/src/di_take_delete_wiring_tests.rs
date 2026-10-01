//! The DI panel's trash dispatches `DeleteLooperTake` —
//! the same command an MCP client sends — and the open panel drops the row
//! only when the core agreed.

use std::cell::RefCell;
use std::rc::Rc;

use application::command::{Command, LooperCommand};
use application::dispatcher::CommandDispatcher;
use application::event::Event;
use application::looper_take_library::TakeDeleteError;
use project::project::Project;
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};

use super::wire_main;
use crate::state::ProjectSession;
use crate::{AppWindow, DiPanel};

#[derive(Default)]
struct SpyDispatcher {
    seen: RefCell<Vec<Command>>,
    refusal: RefCell<Option<TakeDeleteError>>,
    selection: std::sync::Arc<std::sync::RwLock<application::SelectionState>>,
}

impl CommandDispatcher for SpyDispatcher {
    fn dispatch(&self, cmd: Command) -> anyhow::Result<Vec<Event>> {
        self.seen.borrow_mut().push(cmd);
        match self.refusal.borrow().clone() {
            Some(err) => Err(anyhow::Error::new(err)),
            None => Ok(vec![]),
        }
    }

    fn selection_state(&self) -> std::sync::Arc<std::sync::RwLock<application::SelectionState>> {
        std::sync::Arc::clone(&self.selection)
    }
}

fn wired(refusal: Option<TakeDeleteError>) -> (AppWindow, Rc<SpyDispatcher>) {
    i_slint_backend_testing::init_no_event_loop();
    let window = AppWindow::new().expect("window");
    let spy = Rc::new(SpyDispatcher::default());
    *spy.refusal.borrow_mut() = refusal;
    let session = ProjectSession::with_dispatcher(
        Project {
            name: None,
            device_settings: vec![],
            chains: vec![],
            midi: None,
        },
        Rc::clone(&spy) as Rc<dyn CommandDispatcher>,
        None,
        None,
        std::path::PathBuf::from("./presets"),
    );
    wire_main(
        &window,
        &Rc::new(RefCell::new(Some(session))),
        &Rc::new(slint::Timer::default()),
    );
    let panel = window.global::<DiPanel>();
    panel.set_sources(ModelRc::new(VecModel::from(vec![
        SharedString::from("funk"),
        SharedString::from("riff.wav"),
        SharedString::from("Choose file…"),
    ])));
    panel.set_take_rows(ModelRc::new(VecModel::from(vec![false, true, false])));
    panel.set_selected_index(1);
    panel.set_playing(true);
    (window, spy)
}

fn sources(window: &AppWindow) -> Vec<String> {
    window
        .global::<DiPanel>()
        .get_sources()
        .iter()
        .map(|s| s.to_string())
        .collect()
}

#[test]
fn the_trash_dispatches_the_delete_and_drops_the_row() {
    let (window, spy) = wired(None);

    window
        .global::<DiPanel>()
        .invoke_delete_take("riff.wav".into());

    assert!(matches!(
        spy.seen.borrow().as_slice(),
        [Command::Looper(LooperCommand::DeleteLooperTake { name })] if name == "riff.wav"
    ));
    let panel = window.global::<DiPanel>();
    assert_eq!(sources(&window), vec!["funk", "Choose file…"]);
    assert_eq!(
        panel.get_take_rows().iter().collect::<Vec<_>>(),
        vec![false, false]
    );
    assert_eq!(panel.get_selected_index(), -1);
    assert!(!panel.get_playing(), "the deleted take was this chain's DI");
}

#[test]
fn a_refused_delete_keeps_the_row() {
    let (window, _spy) = wired(Some(TakeDeleteError::NotFound("riff.wav".into())));

    window
        .global::<DiPanel>()
        .invoke_delete_take("riff.wav".into());

    assert_eq!(sources(&window), vec!["funk", "riff.wav", "Choose file…"]);
    assert!(window.global::<DiPanel>().get_playing());
}
