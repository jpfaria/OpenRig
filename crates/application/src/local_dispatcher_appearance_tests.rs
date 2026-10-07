//! #398: the light/dark scheme goes through the bus, so MCP asks for it the
//! way the Settings screen does, and the handler keeps it in `config.yaml`.

use std::cell::RefCell;
use std::rc::Rc;

use infra_filesystem::{Appearance, FilesystemStorage};
use project::project::Project;

use crate::command::{Command, SettingsCommand};
use crate::dispatcher::CommandDispatcher;
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;

fn dispatcher() -> LocalDispatcher {
    LocalDispatcher::new(Rc::new(RefCell::new(Project {
        name: None,
        device_settings: Vec::new(),
        chains: Vec::new(),
        midi: None,
    })))
}

fn set(appearance: Appearance) -> Command {
    Command::Settings(SettingsCommand::SetAppearance { appearance })
}

#[test]
fn set_appearance_emits_appearance_changed() {
    crate::local_dispatcher_paths_tests::with_tmp_home("set-appearance-event", || {
        let events = dispatcher()
            .dispatch(set(Appearance::Dark))
            .expect("SetAppearance must succeed");
        crate::persist_worker::flush();
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::AppearanceChanged {
                    appearance: Appearance::Dark
                }
            )),
            "expected Event::AppearanceChanged {{ Dark }}, got {events:?}"
        );
    });
}

#[test]
fn set_appearance_persists_the_choice_to_config() {
    crate::local_dispatcher_paths_tests::with_tmp_home("set-appearance-persist", || {
        dispatcher()
            .dispatch(set(Appearance::Light))
            .expect("SetAppearance must succeed");
        crate::persist_worker::flush();
        let config = FilesystemStorage::load_app_config().unwrap();
        assert_eq!(config.appearance, Appearance::Light);
    });
}
