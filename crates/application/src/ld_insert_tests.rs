//! InsertPrebuilt / Overwrite / SaveChainIo / SaveInsert / rename / audio-settings tests (issue #792 split from local_dispatcher_tests.rs).
//! Shared imports + helpers come from super::tests (pub(super)).

use super::local_dispatcher_tests::*;

// ── InsertPrebuiltBlock tests ─────────────────────────────────────────────────

#[test]
fn insert_prebuilt_block_adds_block_at_position_and_emits_event() {
    let (project, chain_id) = make_project_with_input_chain();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let new_block = make_core_block("blk_new", true);
    let new_block_id = new_block.id.clone();

    let result = dispatcher.dispatch(Command::Block(BlockCommand::InsertPrebuiltBlock {
        chain: chain_id.clone(),
        block: new_block,
        position: 0,
    }));

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    let events = result.unwrap();
    assert!(
        events.iter().any(|e| matches!(e, Event::BlockAdded { chain, block } if *chain == chain_id && *block == new_block_id)),
        "expected BlockAdded event"
    );
    let proj = project.borrow();
    let chain = proj.chains.iter().find(|c| c.id == chain_id).unwrap();
    assert_eq!(
        chain.blocks[0].id, new_block_id,
        "block must be at position 0"
    );
}

#[test]
fn insert_prebuilt_block_non_existent_chain_returns_err() {
    let (project, _) = make_project_with_input_chain();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::Block(BlockCommand::InsertPrebuiltBlock {
        chain: ChainId("chain_MISSING".to_string()),
        block: make_core_block("blk_x", true),
        position: 0,
    }));

    assert!(result.is_err(), "expected Err for missing chain, got Ok");
}

#[test]
fn insert_prebuilt_block_position_clamps_to_len() {
    let (project, chain_id) = make_project_with_input_chain();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let new_block = make_core_block("blk_tail", true);
    let result = dispatcher.dispatch(Command::Block(BlockCommand::InsertPrebuiltBlock {
        chain: chain_id.clone(),
        block: new_block,
        position: 9999,
    }));

    assert!(result.is_ok());
    let proj = project.borrow();
    let chain = proj.chains.iter().find(|c| c.id == chain_id).unwrap();
    assert_eq!(
        chain.blocks.last().unwrap().id,
        BlockId("blk_tail".to_string()),
        "block must be appended at tail when position exceeds len"
    );
}

// ── OverwriteBlock tests ──────────────────────────────────────────────────────

#[test]
fn overwrite_block_replaces_kind_preserves_id_and_emits_event() {
    let (project, chain_id) = make_project_with_input_chain();
    // Add a core block to edit
    let core_block = make_core_block("blk_edit", true);
    let block_id = core_block.id.clone();
    project
        .borrow_mut()
        .chains
        .iter_mut()
        .find(|c| c.id == chain_id)
        .unwrap()
        .blocks
        .push(core_block);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    // Overwrite with a disabled block
    let mut replacement = make_core_block("blk_replacement_id_ignored", false);
    replacement.enabled = false;
    let result = dispatcher.dispatch(Command::Block(BlockCommand::OverwriteBlock {
        chain: chain_id.clone(),
        block: block_id.clone(),
        replacement,
    }));

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    let events = result.unwrap();
    assert!(
        events.iter().any(|e| matches!(e, Event::BlockReplaced { chain, block } if *chain == chain_id && *block == block_id)),
        "expected BlockReplaced event"
    );
    let proj = project.borrow();
    let chain = proj.chains.iter().find(|c| c.id == chain_id).unwrap();
    let blk = chain.blocks.iter().find(|b| b.id == block_id).unwrap();
    assert_eq!(blk.id, block_id, "original id must be preserved");
    assert!(!blk.enabled, "enabled must be updated");
}

#[test]
fn overwrite_block_non_existent_chain_returns_err() {
    let (project, _) = make_project_with_input_chain();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::Block(BlockCommand::OverwriteBlock {
        chain: ChainId("chain_MISSING".to_string()),
        block: BlockId("blk_x".to_string()),
        replacement: make_core_block("blk_x", true),
    }));

    assert!(result.is_err(), "expected Err for missing chain");
}

#[test]
fn overwrite_block_non_existent_block_returns_err() {
    let (project, chain_id) = make_project_with_input_chain();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::Block(BlockCommand::OverwriteBlock {
        chain: chain_id.clone(),
        block: BlockId("blk_MISSING".to_string()),
        replacement: make_core_block("blk_x", true),
    }));

    assert!(result.is_err(), "expected Err for missing block");
}

// ── SaveChainIo tests ─────────────────────────────────────────────────────────

#[test]
fn save_chain_io_replaces_both_endpoints_and_emits_event() {
    let (project, chain_id) = make_project_with_io_chain();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    // io chain: [Input(idx 0), Output(idx 1)]
    let result = dispatcher.dispatch(Command::Chain(ChainCommand::SaveChainIo {
        chain: chain_id.clone(),
        input_block_index: 0,
        output_block_index: 1,
        io: "main".to_string(),
        endpoint: "Guitar In".to_string(),
    }));

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    let events = result.unwrap();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::ChainIoSaved { chain } if *chain == chain_id)),
        "expected ChainIoSaved, got {:?}",
        events
    );
    let proj = project.borrow();
    let chain = proj.chains.iter().find(|c| c.id == chain_id).unwrap();
    let inp = chain.blocks.iter().find_map(|b| {
        if let AudioBlockKind::Input(ib) = &b.kind {
            Some(ib)
        } else {
            None
        }
    });
    let out = chain.blocks.iter().find_map(|b| {
        if let AudioBlockKind::Output(ob) = &b.kind {
            Some(ob)
        } else {
            None
        }
    });
    assert!(inp.is_some() && out.is_some());
    assert_eq!(inp.unwrap().io, "main");
    assert_eq!(out.unwrap().io, "main");
}

#[test]
fn save_chain_io_non_existent_chain_returns_err() {
    let (project, _) = make_project_with_io_chain();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::Chain(ChainCommand::SaveChainIo {
        chain: ChainId("chain_MISSING".to_string()),
        input_block_index: 0,
        output_block_index: 1,
        io: String::new(),
        endpoint: String::new(),
    }));

    assert!(result.is_err(), "expected Err for missing chain, got Ok");
}

#[test]
fn save_chain_io_missing_input_block_returns_err() {
    let project = Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains: vec![Chain {
            id: ChainId("chain_no_input".to_string()),
            description: None,
            instrument: "electric_guitar".to_string(),
            enabled: false,
            volume: 100.0,
            io_binding_ids: vec![],
            blocks: vec![make_output_block("dev_b", 1)], // output only, no input
            di_output: None,
        }],
        midi: None,
    }));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    // Block at index 0 is an OutputBlock, not InputBlock → must return Err.
    let result = dispatcher.dispatch(Command::Chain(ChainCommand::SaveChainIo {
        chain: ChainId("chain_no_input".to_string()),
        input_block_index: 0,
        output_block_index: 0,
        io: String::new(),
        endpoint: String::new(),
    }));

    assert!(result.is_err(), "expected Err when chain has no InputBlock");
}

// ── SaveInsertBlock tests ─────────────────────────────────────────────────────

fn make_insert_block(id: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.to_string()),
        enabled: true,
        kind: AudioBlockKind::Insert(project::block::InsertBlock {
            model: "standard".to_string(),
            io: "fx".to_string(),
        }),
    }
}

fn make_project_with_insert() -> (Rc<RefCell<Project>>, ChainId, BlockId) {
    let insert = make_insert_block("insert_0");
    let block_id = insert.id.clone();
    let chain_id = ChainId("chain_insert".to_string());
    let project = Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains: vec![Chain {
            id: chain_id.clone(),
            description: None,
            instrument: "electric_guitar".to_string(),
            enabled: false,
            volume: 100.0,
            io_binding_ids: vec![],
            blocks: vec![insert],
            di_output: None,
        }],
        midi: None,
    }));
    (project, chain_id, block_id)
}

#[test]
fn save_insert_block_updates_binding_and_emits_event() {
    let (project, chain_id, block_id) = make_project_with_insert();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::Block(BlockCommand::SaveInsertBlock {
        chain: chain_id.clone(),
        block: block_id.clone(),
        io: "mk300".to_string(),
    }));

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    let events = result.unwrap();
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::InsertBlockSaved { chain, block }
            if *chain == chain_id && *block == block_id
        )),
        "expected InsertBlockSaved event, got {:?}",
        events
    );
    let proj = project.borrow();
    let chain = proj.chains.iter().find(|c| c.id == chain_id).unwrap();
    let block = chain.blocks.iter().find(|b| b.id == block_id).unwrap();
    if let AudioBlockKind::Insert(ib) = &block.kind {
        assert_eq!(ib.io, "mk300");
    } else {
        panic!("expected InsertBlock kind");
    }
}

#[test]
fn save_insert_block_non_existent_block_returns_err() {
    let (project, chain_id, _) = make_project_with_insert();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::Block(BlockCommand::SaveInsertBlock {
        chain: chain_id,
        block: BlockId("blk_MISSING".to_string()),
        io: "fx".to_string(),
    }));

    assert!(result.is_err(), "expected Err for missing block, got Ok");
}

#[test]
fn save_insert_block_non_insert_kind_returns_err() {
    // Block exists but is a CoreBlock, not InsertBlock.
    let project = make_project("chain_0", make_core_block("blk_0", true));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::Block(BlockCommand::SaveInsertBlock {
        chain: ChainId("chain_0".to_string()),
        block: BlockId("blk_0".to_string()),
        io: "fx".to_string(),
    }));

    assert!(result.is_err(), "expected Err for non-Insert block kind");
}

// ── UpdateProjectName tests ───────────────────────────────────────────────────

fn make_named_project(name: &str) -> Rc<RefCell<Project>> {
    Rc::new(RefCell::new(Project {
        name: Some(name.to_string()),
        device_settings: vec![],
        chains: vec![],
        midi: None,
    }))
}

#[test]
fn update_project_name_writes_name_and_emits_event() {
    let project = make_named_project("old name");
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::Project(ProjectCommand::UpdateProjectName {
        name: "new name".to_string(),
    }));

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    let events = result.unwrap();
    assert!(
        events.iter().any(|e| matches!(e, Event::ProjectMutated)),
        "expected ProjectMutated event"
    );
    assert_eq!(
        project.borrow().name.as_deref(),
        Some("new name"),
        "project name must be updated"
    );
}

#[test]
fn update_project_name_sets_name_to_none_when_empty() {
    // An empty name should set project.name = None.
    let project = make_named_project("old name");
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::Project(ProjectCommand::UpdateProjectName {
        name: "".to_string(),
    }));

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    assert_eq!(
        project.borrow().name,
        None,
        "empty name must set project.name = None"
    );
}

#[test]
fn update_project_name_trims_whitespace() {
    let project = make_named_project("old");
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::Project(ProjectCommand::UpdateProjectName {
        name: "  trimmed  ".to_string(),
    }));

    assert!(result.is_ok());
    assert_eq!(
        project.borrow().name.as_deref(),
        Some("trimmed"),
        "project name must be trimmed"
    );
}

// ── SaveAudioSettings tests ───────────────────────────────────────────────────

pub(super) fn make_device_settings(device_id: &str) -> project::device::DeviceSettings {
    project::device::DeviceSettings {
        device_id: DeviceId(device_id.to_string()),
        sample_rate: 44100,
        buffer_size_frames: 256,
        bit_depth: 32,
        #[cfg(target_os = "linux")]
        realtime: true,
        #[cfg(target_os = "linux")]
        rt_priority: 70,
        #[cfg(target_os = "linux")]
        nperiods: 3,
    }
}

// #581: SaveAudioSettings now persists into the per-machine
// `config.yaml`. Each test redirects `$HOME` to a unique tempdir so the
// FS write stays out of the developer's real `~/Library/Application
// Support/OpenRig/`.
#[test]
fn save_audio_settings_writes_device_settings_and_emits_event() {
    crate::local_dispatcher_paths_tests::with_tmp_home("save-audio-emit", || {
        let project = Rc::new(RefCell::new(Project {
            name: None,
            device_settings: vec![],
            chains: vec![],
            midi: None,
        }));
        let dispatcher = LocalDispatcher::new(Rc::clone(&project));

        let settings = vec![make_device_settings("dev_a"), make_device_settings("dev_b")];
        let result = dispatcher.dispatch(Command::Settings(SettingsCommand::SaveAudioSettings {
            input_devices: settings.clone(),
            output_devices: vec![],
        }));

        assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
        let events = result.unwrap();
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::AudioSettingsSaved)),
            "expected AudioSettingsSaved event, got {:?}",
            events
        );
        let proj = project.borrow();
        assert_eq!(proj.device_settings.len(), 2);
        assert_eq!(proj.device_settings[0].device_id.0, "dev_a");
        assert_eq!(proj.device_settings[1].device_id.0, "dev_b");
    });
}

#[test]
fn save_audio_settings_replaces_previous_settings() {
    crate::local_dispatcher_paths_tests::with_tmp_home("save-audio-replace", || {
        let project = Rc::new(RefCell::new(Project {
            name: None,
            device_settings: vec![make_device_settings("old_dev")],
            chains: vec![],
            midi: None,
        }));
        let dispatcher = LocalDispatcher::new(Rc::clone(&project));

        let result = dispatcher.dispatch(Command::Settings(SettingsCommand::SaveAudioSettings {
            input_devices: vec![make_device_settings("new_dev")],
            output_devices: vec![],
        }));

        assert!(result.is_ok());
        let proj = project.borrow();
        assert_eq!(proj.device_settings.len(), 1);
        assert_eq!(proj.device_settings[0].device_id.0, "new_dev");
    });
}

#[test]
fn save_audio_settings_empty_clears_settings() {
    crate::local_dispatcher_paths_tests::with_tmp_home("save-audio-empty", || {
        let project = Rc::new(RefCell::new(Project {
            name: None,
            device_settings: vec![make_device_settings("dev_a")],
            chains: vec![],
            midi: None,
        }));
        let dispatcher = LocalDispatcher::new(Rc::clone(&project));

        let result = dispatcher.dispatch(Command::Settings(SettingsCommand::SaveAudioSettings {
            input_devices: vec![],
            output_devices: vec![],
        }));

        assert!(result.is_ok());
        assert!(project.borrow().device_settings.is_empty());
    });
}

// Regression: the same physical interface enumerates with a DIFFERENT id per
// direction (e.g. CoreAudio input `dev:1` vs output `dev:2`). The command must
// carry the input/output split so the handler persists each list into its own
// `config.yaml` field — collapsing both into one flat list corrupts the saved
// selection and the device fails to re-match on reopen (#581 follow-up).
#[test]
fn save_audio_settings_persists_input_and_output_separately() {
    crate::local_dispatcher_paths_tests::with_tmp_home("save-audio-split", || {
        let project = Rc::new(RefCell::new(Project {
            name: None,
            device_settings: vec![],
            chains: vec![],
            midi: None,
        }));
        let dispatcher = LocalDispatcher::new(Rc::clone(&project));

        let result = dispatcher.dispatch(Command::Settings(SettingsCommand::SaveAudioSettings {
            input_devices: vec![make_device_settings("dev:in")],
            output_devices: vec![make_device_settings("dev:out")],
        }));
        assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
        // #693: the config write is queued to the persist worker — wait
        // for durability before reading it back.
        crate::persist_worker::flush();

        let config = infra_filesystem::FilesystemStorage::load_app_config().unwrap();
        let in_ids: Vec<&str> = config
            .input_devices
            .iter()
            .map(|d| d.device_id.as_str())
            .collect();
        let out_ids: Vec<&str> = config
            .output_devices
            .iter()
            .map(|d| d.device_id.as_str())
            .collect();
        assert_eq!(
            in_ids,
            vec!["dev:in"],
            "input devices must persist input ids only"
        );
        assert_eq!(
            out_ids,
            vec!["dev:out"],
            "output devices must persist output ids only"
        );
    });
}
