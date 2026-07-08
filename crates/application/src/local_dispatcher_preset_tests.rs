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
            chain: domain::ids::ChainId("c1".to_string()),
            name: "lead".to_string(),
        })
        .expect("SaveChainPreset deve ok");
    // #693: preset I/O runs on the persist worker — wait before reading back.
    crate::persist_worker::flush();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::ChainPresetSaved { name } if name == "lead")),
        "esperava Event::ChainPresetSaved {{ name: \"lead\" }}, veio {events:?}"
    );
}

/// #555: with a presets_path attached AND a matching chain in the
/// project, `Command::SaveChainPreset` writes the chain's FX blocks
/// to disk as a YAML preset. The GUI used to do this `fs::write` in
/// `adapter-gui::preset_save_wiring::perform_preset_save` — a
/// violation of "tela sem regra de negócio".
#[test]
fn save_chain_preset_writes_file_when_presets_path_attached() {
    use project::chain::Chain;
    use project::project::Project;

    let tmp = tempfile::tempdir().expect("tempdir");
    let presets_dir = tmp.path().to_path_buf();
    let chain_id = domain::ids::ChainId("c1".to_string());
    let project = Project {
        name: None,
        device_settings: Vec::new(),
        chains: vec![Chain {
            id: chain_id.clone(),
            description: Some("electric guitar".to_string()),
            instrument: "electric_guitar".to_string(),
            enabled: true,
            volume: 100.0,
            io_binding_ids: vec![],
            blocks: Vec::new(),
            di_output: None,
        }],
        midi: None,
    };
    let dispatcher = LocalDispatcher::new(Rc::new(RefCell::new(project)));
    dispatcher.attach_presets_path(presets_dir.clone());

    let preset_name = "Clocks — Coldplay (rhythm)";
    let events = dispatcher
        .dispatch(Command::SaveChainPreset {
            chain: chain_id,
            name: preset_name.to_string(),
        })
        .expect("SaveChainPreset should succeed");
    // #693: preset I/O runs on the persist worker — wait before reading back.
    crate::persist_worker::flush();

    let preset_path = crate::preset_file::preset_save_path(&presets_dir, preset_name);
    assert!(
        preset_path.exists(),
        "preset file at {preset_path:?} should be written by Command::SaveChainPreset"
    );
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::ChainPresetSaved { name } if name == preset_name)));
}

#[test]
fn delete_chain_preset_emits_event_with_name() {
    let events = dispatcher()
        .dispatch(Command::DeleteChainPreset {
            name: "old".to_string(),
        })
        .expect("DeleteChainPreset deve ok");
    // #693: preset I/O runs on the persist worker — wait before reading back.
    crate::persist_worker::flush();
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
    assert!(
        preset_path.exists(),
        "fixture preset should exist before delete"
    );

    let dispatcher = dispatcher();
    dispatcher.attach_presets_path(presets_dir.clone());
    dispatcher
        .dispatch(Command::DeleteChainPreset {
            name: preset_name.to_string(),
        })
        .expect("DeleteChainPreset deve ok");
    // #693: preset I/O runs on the persist worker — wait before reading back.
    crate::persist_worker::flush();

    assert!(
        !preset_path.exists(),
        "preset file at {preset_path:?} should be gone after Command::DeleteChainPreset"
    );
}

// ── #627 Part 2: SaveChainPreset tags the preset with the chain's instrument ──

/// #627: saving a preset from an acoustic_guitar chain must write
/// `instrument: acoustic_guitar` into the preset YAML file.
#[test]
fn save_chain_preset_tags_preset_with_chain_instrument() {
    use infra_yaml::load_chain_preset_file;
    use project::chain::Chain;
    use project::project::Project;

    let tmp = tempfile::tempdir().expect("tempdir");
    let presets_dir = tmp.path().to_path_buf();
    let chain_id = domain::ids::ChainId("acoustic1".to_string());
    let project = Project {
        name: None,
        device_settings: Vec::new(),
        chains: vec![Chain {
            id: chain_id.clone(),
            description: Some("violão".to_string()),
            instrument: "acoustic_guitar".to_string(),
            enabled: true,
            volume: 100.0,
            io_binding_ids: vec![],
            blocks: Vec::new(),
            di_output: None,
        }],
        midi: None,
    };
    let dispatcher = LocalDispatcher::new(Rc::new(RefCell::new(project)));
    dispatcher.attach_presets_path(presets_dir.clone());

    dispatcher
        .dispatch(Command::SaveChainPreset {
            chain: chain_id,
            name: "Violao Clean".to_string(),
        })
        .expect("SaveChainPreset should succeed");
    // #693: preset I/O runs on the persist worker — wait before reading back.
    crate::persist_worker::flush();

    let preset_path = crate::preset_file::preset_save_path(&presets_dir, "Violao Clean");
    assert!(preset_path.exists(), "preset file must be written");
    let preset = load_chain_preset_file(&preset_path).expect("preset must load");
    assert_eq!(
        preset.instrument, "acoustic_guitar",
        "saved preset must be tagged with the chain's instrument (acoustic_guitar), got {:?}",
        preset.instrument
    );
}

// ── #627 Part 3: LoadChainPreset rejects instrument mismatch ──────────────────

/// #627: loading an acoustic_guitar preset into an electric_guitar chain
/// must return Err and leave the chain unchanged.
#[test]
fn load_chain_preset_rejects_instrument_mismatch() {
    use project::block::{AudioBlock, AudioBlockKind, CoreBlock};
    use project::chain::Chain;
    use project::param::ParameterSet;
    use project::project::Project;

    let chain_id = domain::ids::ChainId("electric1".to_string());
    let original_block = AudioBlock {
        id: domain::ids::BlockId("original".to_string()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "gain".to_string(),
            model: "volume".to_string(),
            params: ParameterSet::default(),
        }),
    };
    let project = Rc::new(RefCell::new(Project {
        name: None,
        device_settings: Vec::new(),
        chains: vec![Chain {
            id: chain_id.clone(),
            description: Some("electric guitar chain".to_string()),
            instrument: "electric_guitar".to_string(),
            enabled: true,
            volume: 100.0,
            io_binding_ids: vec![],
            blocks: vec![original_block],
            di_output: None,
        }],
        midi: None,
    }));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::LoadChainPreset {
        chain: chain_id,
        preset_instrument: "acoustic_guitar".to_string(),
        preset_blocks: Vec::new(),
    });

    assert!(
        result.is_err(),
        "loading acoustic_guitar preset into electric_guitar chain must fail, got Ok"
    );
    // Chain must be unchanged
    let blocks_after = project.borrow().chains[0].blocks.len();
    assert_eq!(
        blocks_after, 1,
        "chain must not be mutated on mismatch reject"
    );
}

/// #627: loading a preset with matching instrument succeeds.
#[test]
fn load_chain_preset_accepts_matching_instrument() {
    use project::chain::Chain;
    use project::project::Project;

    let chain_id = domain::ids::ChainId("electric1".to_string());
    let project = Rc::new(RefCell::new(Project {
        name: None,
        device_settings: Vec::new(),
        chains: vec![Chain {
            id: chain_id.clone(),
            description: None,
            instrument: "electric_guitar".to_string(),
            enabled: true,
            volume: 100.0,
            io_binding_ids: vec![],
            blocks: Vec::new(),
            di_output: None,
        }],
        midi: None,
    }));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::LoadChainPreset {
        chain: chain_id,
        preset_instrument: "electric_guitar".to_string(),
        preset_blocks: Vec::new(),
    });

    assert!(
        result.is_ok(),
        "matching instrument must succeed, got {:?}",
        result
    );
}

/// #627: an untagged legacy preset (instrument defaults to electric_guitar)
/// loads into an electric_guitar chain without error.
#[test]
fn load_chain_preset_back_compat_untagged_defaults_to_electric_guitar() {
    use project::chain::Chain;
    use project::project::Project;

    // Simulate an untagged (legacy) preset by using the default instrument
    // value, which is "electric_guitar" per the serde default.
    let chain_id = domain::ids::ChainId("e1".to_string());
    let project = Rc::new(RefCell::new(Project {
        name: None,
        device_settings: Vec::new(),
        chains: vec![Chain {
            id: chain_id.clone(),
            description: None,
            instrument: "electric_guitar".to_string(),
            enabled: true,
            volume: 100.0,
            io_binding_ids: vec![],
            blocks: Vec::new(),
            di_output: None,
        }],
        midi: None,
    }));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    // "electric_guitar" is the default for untagged presets (serde default)
    let result = dispatcher.dispatch(Command::LoadChainPreset {
        chain: chain_id,
        preset_instrument: "electric_guitar".to_string(),
        preset_blocks: Vec::new(),
    });

    assert!(
        result.is_ok(),
        "untagged (electric_guitar default) preset into electric_guitar chain must succeed"
    );
}

/// #627: deleting a preset that doesn't exist on disk is a silent
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
    // #693: preset I/O runs on the persist worker — wait before reading back.
    crate::persist_worker::flush();
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::ChainPresetDeleted { name } if name == "does-not-exist")));
}
