//! Midi-enable / SaveProject / LoadProject / CreateProject tests (issue #792 split from local_dispatcher_tests.rs).
//! Shared imports + helpers come from super::tests (pub(super)).

use super::local_dispatcher_tests::*;

// ── SetMidiEnabled / SetMcpEnabled tests (#712) ───────────────────────────────
// The Settings toggle persists the per-machine master switch into
// config.yaml so packaged builds (launched with no CLI flags) bring the
// subsystem up on next launch. State change → Command (GUI/MCP/gRPC
// parity), never a borrow_mut in the callback.

#[test]
fn set_midi_enabled_persists_true_to_config() {
    crate::local_dispatcher_paths_tests::with_tmp_home("set-midi-enabled-true", || {
        let project = Rc::new(RefCell::new(Project {
            name: None,
            device_settings: vec![],
            chains: vec![],
            midi: None,
        }));
        let dispatcher = LocalDispatcher::new(Rc::clone(&project));

        let result =
            dispatcher.dispatch(Command::Midi(MidiCommand::SetMidiEnabled { enabled: true }));
        assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
        crate::persist_worker::flush();

        let config = infra_filesystem::FilesystemStorage::load_app_config().unwrap();
        assert!(
            config.midi_enabled,
            "midi_enabled must persist to config.yaml"
        );
    });
}

#[test]
fn set_mcp_enabled_persists_true_to_config() {
    crate::local_dispatcher_paths_tests::with_tmp_home("set-mcp-enabled-true", || {
        let project = Rc::new(RefCell::new(Project {
            name: None,
            device_settings: vec![],
            chains: vec![],
            midi: None,
        }));
        let dispatcher = LocalDispatcher::new(Rc::clone(&project));

        let result = dispatcher.dispatch(Command::Settings(SettingsCommand::SetMcpEnabled {
            enabled: true,
        }));
        assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
        crate::persist_worker::flush();

        let config = infra_filesystem::FilesystemStorage::load_app_config().unwrap();
        assert!(
            config.mcp_enabled,
            "mcp_enabled must persist to config.yaml"
        );
    });
}

#[test]
fn set_midi_enabled_false_clears_the_switch_and_preserves_other_config() {
    crate::local_dispatcher_paths_tests::with_tmp_home("set-midi-enabled-false", || {
        let project = Rc::new(RefCell::new(Project {
            name: None,
            device_settings: vec![],
            chains: vec![],
            midi: None,
        }));
        let dispatcher = LocalDispatcher::new(Rc::clone(&project));

        // Seed mcp on, midi on; then turn midi off — mcp must stay on.
        dispatcher
            .dispatch(Command::Settings(SettingsCommand::SetMcpEnabled {
                enabled: true,
            }))
            .unwrap();
        dispatcher
            .dispatch(Command::Midi(MidiCommand::SetMidiEnabled { enabled: true }))
            .unwrap();
        dispatcher
            .dispatch(Command::Midi(MidiCommand::SetMidiEnabled {
                enabled: false,
            }))
            .unwrap();
        crate::persist_worker::flush();

        let config = infra_filesystem::FilesystemStorage::load_app_config().unwrap();
        assert!(!config.midi_enabled, "midi must be off");
        assert!(
            config.mcp_enabled,
            "mcp must remain on — fields are independent"
        );
    });
}

// ── SaveProject tests ─────────────────────────────────────────────────────────

#[test]
fn save_project_emits_project_saved_event() {
    // SaveProject in the dispatcher is a no-op metadata signal — actual file I/O
    // happens in the adapter. The dispatcher just emits ProjectSaved.
    let project = Rc::new(RefCell::new(Project {
        name: Some("my project".to_string()),
        device_settings: vec![],
        chains: vec![],
        midi: None,
    }));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::Project(ProjectCommand::SaveProject));

    assert!(result.is_ok(), "SaveProject must return Ok: {:?}", result);
    assert!(
        result
            .unwrap()
            .iter()
            .any(|e| matches!(e, Event::ProjectSaved)),
        "expected ProjectSaved event"
    );
}

#[test]
fn save_project_does_not_mutate_project() {
    let project = Rc::new(RefCell::new(Project {
        name: Some("stable".to_string()),
        device_settings: vec![],
        chains: vec![],
        midi: None,
    }));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let _ = dispatcher.dispatch(Command::Project(ProjectCommand::SaveProject));

    assert_eq!(
        project.borrow().name.as_deref(),
        Some("stable"),
        "SaveProject must not mutate the project"
    );
}

#[test]
fn save_project_is_ok_with_empty_project() {
    let project = Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains: vec![],
        midi: None,
    }));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    let result = dispatcher.dispatch(Command::Project(ProjectCommand::SaveProject));
    assert!(result.is_ok());
}

// ── LoadProject tests ─────────────────────────────────────────────────────────

#[test]
fn load_project_replaces_project_and_emits_event() {
    let project = Rc::new(RefCell::new(Project {
        name: Some("old".to_string()),
        device_settings: vec![],
        chains: vec![],
        midi: None,
    }));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let new_proj = Project {
        name: Some("loaded".to_string()),
        device_settings: vec![],
        chains: vec![make_empty_chain("chain_loaded", false)],
        midi: None,
    };

    let result = dispatcher.dispatch(Command::Project(ProjectCommand::LoadProject {
        project: new_proj,
        path: std::path::PathBuf::from("/some/path.yaml"),
    }));

    assert!(result.is_ok(), "LoadProject returned Err: {:?}", result);
    let events = result.unwrap();
    assert!(
        events.iter().any(|e| matches!(e, Event::ProjectLoaded)),
        "expected ProjectLoaded event, got {:?}",
        events
    );
    let proj = project.borrow();
    assert_eq!(proj.name.as_deref(), Some("loaded"));
    assert_eq!(proj.chains.len(), 1);
    assert_eq!(proj.chains[0].id.0, "chain_loaded");
}

#[test]
fn load_project_replaces_all_state() {
    // Project starts with chains; load should replace them entirely.
    let project = Rc::new(RefCell::new(Project {
        name: Some("old".to_string()),
        device_settings: vec![make_device_settings("old_dev")],
        chains: vec![make_empty_chain("old_chain", false)],
        midi: None,
    }));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let new_proj = Project {
        name: None,
        device_settings: vec![],
        chains: vec![],
        midi: None,
    };

    let _ = dispatcher.dispatch(Command::Project(ProjectCommand::LoadProject {
        project: new_proj,
        path: std::path::PathBuf::from("/p.yaml"),
    }));

    let proj = project.borrow();
    assert!(proj.name.is_none());
    assert!(proj.device_settings.is_empty());
    assert!(proj.chains.is_empty());
}

#[test]
fn load_project_disables_blocks_with_unavailable_models() {
    // #606 parity: loading a project through the command bus (MCP/gRPC) must
    // disable blocks whose model is not installed, exactly like the GUI load
    // path — so the chain plays without a silently-faulted "on" pedal.
    let project = Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains: vec![],
        midi: None,
    }));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let mut chain = make_empty_chain("c", true);
    chain.blocks.push(AudioBlock {
        id: BlockId("ghost".into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "gain".into(),
            // No native gain model and no pack on disk → unavailable.
            model: "nam_uninstalled_pedal_for_issue_606".into(),
            params: ParameterSet::default(),
        }),
    });
    let new_proj = Project {
        name: None,
        device_settings: vec![],
        chains: vec![chain],
        midi: None,
    };

    dispatcher
        .dispatch(Command::Project(ProjectCommand::LoadProject {
            project: new_proj,
            path: std::path::PathBuf::from("/p.yaml"),
        }))
        .expect("LoadProject");

    let proj = project.borrow();
    let ghost = proj.chains[0]
        .blocks
        .iter()
        .find(|b| b.id.0 == "ghost")
        .expect("block must be preserved on load");
    assert!(
        !ghost.enabled,
        "BUG #606: LoadProject must disable a block whose model is unavailable (MCP/gRPC parity)"
    );
}

fn project_with_one_block(chain_id: &str, block: AudioBlock) -> Rc<RefCell<Project>> {
    let mut chain = make_empty_chain(chain_id, true);
    chain.blocks.push(block);
    Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains: vec![chain],
        midi: None,
    }))
}

fn unavailable_gain_block(id: &str, enabled: bool) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "gain".into(),
            model: "nam_uninstalled_pedal_for_issue_606".into(),
            params: ParameterSet::default(),
        }),
    }
}

#[test]
fn toggle_block_enabled_refuses_to_enable_unavailable_model() {
    // #606: a disabled block whose pack is not installed must NOT be
    // enable-able — the user can't activate a pedal that cannot build.
    let project = project_with_one_block("c", unavailable_gain_block("ghost", false));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let events = dispatcher
        .dispatch(Command::Block(BlockCommand::ToggleBlockEnabled {
            chain: ChainId("c".into()),
            block: BlockId("ghost".into()),
        }))
        .expect("ToggleBlockEnabled");

    assert!(
        !project.borrow().chains[0].blocks[0].enabled,
        "BUG #606: toggling an unavailable block must NOT enable it"
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::BlockEnabledChanged { enabled: false, .. })),
        "the emitted event must report the block stayed disabled, got {events:?}"
    );
}

#[test]
fn toggle_block_enabled_still_disables_an_unavailable_block_that_is_on() {
    // Disabling is always allowed, even for an unavailable model (e.g. a
    // pack uninstalled while the block was on) — only ENABLING is blocked.
    let project = project_with_one_block("c", unavailable_gain_block("ghost", true));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    dispatcher
        .dispatch(Command::Block(BlockCommand::ToggleBlockEnabled {
            chain: ChainId("c".into()),
            block: BlockId("ghost".into()),
        }))
        .expect("ToggleBlockEnabled");

    assert!(
        !project.borrow().chains[0].blocks[0].enabled,
        "toggling an ON unavailable block must turn it OFF (disabling is allowed)"
    );
}

#[test]
fn load_project_emits_project_mutated() {
    let project = Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains: vec![],
        midi: None,
    }));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::Project(ProjectCommand::LoadProject {
        project: Project {
            name: None,
            device_settings: vec![],
            chains: vec![],
            midi: None,
        },
        path: std::path::PathBuf::from("/p.yaml"),
    }));

    assert!(result.is_ok());
    assert!(
        result
            .unwrap()
            .iter()
            .any(|e| matches!(e, Event::ProjectMutated)),
        "expected ProjectMutated"
    );
}

// ── CreateProject tests ───────────────────────────────────────────────────────

#[test]
fn create_project_replaces_project_and_emits_event() {
    let project = Rc::new(RefCell::new(Project {
        name: Some("old".to_string()),
        device_settings: vec![],
        chains: vec![make_empty_chain("old_chain", false)],
        midi: None,
    }));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let new_proj = Project {
        name: Some("brand new".to_string()),
        device_settings: vec![],
        chains: vec![],
        midi: None,
    };

    let result = dispatcher.dispatch(Command::Project(ProjectCommand::CreateProject {
        project: new_proj,
    }));

    assert!(result.is_ok(), "CreateProject returned Err: {:?}", result);
    let events = result.unwrap();
    assert!(
        events.iter().any(|e| matches!(e, Event::ProjectCreated)),
        "expected ProjectCreated event, got {:?}",
        events
    );
    let proj = project.borrow();
    assert_eq!(proj.name.as_deref(), Some("brand new"));
    assert!(proj.chains.is_empty());
}

#[test]
fn create_project_emits_project_mutated() {
    let project = Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains: vec![],
        midi: None,
    }));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::Project(ProjectCommand::CreateProject {
        project: Project {
            name: Some("new".to_string()),
            device_settings: vec![],
            chains: vec![],
            midi: None,
        },
    }));

    assert!(result.is_ok());
    assert!(
        result
            .unwrap()
            .iter()
            .any(|e| matches!(e, Event::ProjectMutated)),
        "expected ProjectMutated"
    );
}

#[test]
fn create_project_replaces_all_prior_state() {
    let project = Rc::new(RefCell::new(Project {
        name: Some("old".to_string()),
        device_settings: vec![make_device_settings("dev_old")],
        chains: vec![make_empty_chain("c", false)],
        midi: None,
    }));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let _ = dispatcher.dispatch(Command::Project(ProjectCommand::CreateProject {
        project: Project {
            name: None,
            device_settings: vec![],
            chains: vec![],
            midi: None,
        },
    }));

    let proj = project.borrow();
    assert!(proj.name.is_none());
    assert!(proj.device_settings.is_empty());
    assert!(proj.chains.is_empty());
}
