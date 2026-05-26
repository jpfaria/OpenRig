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
        midi: None,
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

/// #555: with a presets_path attached, `Command::DeleteChainPreset`
/// removes the actual preset file on disk. This used to be the GUI's
/// job at `adapter-gui::chain_preset_wiring::on_preset_picker_delete`
/// — a violation of "tela sem regra de negócio".
#[test]
fn delete_chain_preset_removes_file_when_presets_path_attached() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let presets_dir = tmp.path().to_path_buf();
    let preset_name = "Clocks — Coldplay (rhythm)";
    let preset_path = crate::preset_file::preset_save_path(&presets_dir, preset_name);
    std::fs::write(&preset_path, b"id: clocks\nblocks: []\n").expect("seed preset");
    assert!(preset_path.exists(), "fixture preset should exist before delete");

    let dispatcher = dispatcher();
    dispatcher.attach_presets_path(presets_dir.clone());
    dispatcher
        .dispatch(Command::DeleteChainPreset {
            name: preset_name.to_string(),
        })
        .expect("DeleteChainPreset deve ok");

    assert!(
        !preset_path.exists(),
        "preset file at {preset_path:?} should be gone after Command::DeleteChainPreset"
    );
}

/// #555: deleting a preset that doesn't exist on disk is a silent
/// no-op (idempotent). The dispatcher still emits the event so
/// observers can refresh their UI; the file just isn't there to
/// remove.
#[test]
fn delete_chain_preset_is_idempotent_when_file_missing() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let dispatcher = dispatcher();
    dispatcher.attach_presets_path(tmp.path().to_path_buf());

    let events = dispatcher
        .dispatch(Command::DeleteChainPreset {
            name: "does-not-exist".to_string(),
        })
        .expect("DeleteChainPreset of missing file is a no-op");
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::ChainPresetDeleted { name } if name == "does-not-exist")));
}
