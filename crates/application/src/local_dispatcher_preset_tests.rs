//! Red-first (#436 F): `Command::SaveChainPreset` /
//! `Command::DeleteChainPreset` despacham e emitem
//! `Event::ChainPresetSaved` / `ChainPresetDeleted`. Precedente
//! `SaveProject` (I/O de arquivo no adapter, Command = intenção+evento).

use std::cell::RefCell;
use std::rc::Rc;

use project::project::Project;

use crate::command::Command;
use crate::dispatcher::CommandDispatcher;
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;

fn dispatcher() -> LocalDispatcher {
    LocalDispatcher::new(Rc::new(RefCell::new(Project {
        name: None,
        device_settings: Vec::new(),
        chains: Vec::new(),
    })))
}

#[test]
fn save_chain_preset_emits_event_with_name() {
    let events = dispatcher()
        .dispatch(Command::SaveChainPreset {
            name: "lead".to_string(),
        })
        .expect("SaveChainPreset deve ok");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::ChainPresetSaved { name } if name == "lead")),
        "esperava Event::ChainPresetSaved {{ name: \"lead\" }}, veio {events:?}"
    );
}

#[test]
fn delete_chain_preset_emits_event_with_name() {
    let events = dispatcher()
        .dispatch(Command::DeleteChainPreset {
            name: "old".to_string(),
        })
        .expect("DeleteChainPreset deve ok");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::ChainPresetDeleted { name } if name == "old")),
        "esperava Event::ChainPresetDeleted {{ name: \"old\" }}, veio {events:?}"
    );
}
