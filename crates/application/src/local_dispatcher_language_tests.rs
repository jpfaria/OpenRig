//! Red-first (#436 F): `Command::SetLanguage` despacha e emite
//! `Event::LanguageChanged` — assim MCP/MIDI/GUI pedem a troca pela
//! mesma porta. Segue o precedente `SaveProject` (efeito no adapter,
//! Command = intenção + evento).

use std::cell::RefCell;
use std::rc::Rc;

use project::project::Project;

use crate::command::Command;
use crate::dispatcher::CommandDispatcher;
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;

fn dispatcher() -> LocalDispatcher {
    let project = Rc::new(RefCell::new(Project {
        name: None,
        device_settings: Vec::new(),
        chains: Vec::new(),
    }));
    LocalDispatcher::new(project)
}

#[test]
fn set_language_emits_language_changed_event() {
    let disp = dispatcher();
    let events = disp
        .dispatch(Command::SetLanguage {
            language: Some("pt".to_string()),
        })
        .expect("SetLanguage deve ok");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::LanguageChanged { language } if language.as_deref() == Some("pt"))),
        "esperava Event::LanguageChanged {{ language: Some(\"pt\") }}, veio {events:?}"
    );
}

#[test]
fn set_language_none_means_system_default() {
    let disp = dispatcher();
    let events = disp
        .dispatch(Command::SetLanguage { language: None })
        .expect("SetLanguage(None) deve ok");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::LanguageChanged { language: None })),
        "esperava Event::LanguageChanged {{ language: None }}, veio {events:?}"
    );
}
