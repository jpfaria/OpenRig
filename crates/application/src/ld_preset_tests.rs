//! LoadChainPreset / invariant / SetChainVolume / MIDI tests (issue #792 split from local_dispatcher_tests.rs).
//! Shared imports + helpers come from super::tests (pub(super)).

use super::local_dispatcher_tests::*;

// ── LoadChainPreset tests ─────────────────────────────────────────────────────

#[test]
fn load_chain_preset_replaces_blocks_and_emits_event() {
    let existing_block = make_core_block("blk_old", true);
    let project = make_project("chain_preset", existing_block);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let new_block_a = make_core_block("blk_new_a", true);
    let new_block_b = make_core_block("blk_new_b", false);
    let preset_blocks = vec![new_block_a, new_block_b];

    let result = dispatcher.dispatch(Command::LoadChainPreset {
        chain: ChainId("chain_preset".to_string()),
        preset_instrument: "electric_guitar".to_string(),
        preset_blocks: preset_blocks.clone(),
    });

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    let events = result.unwrap();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::ChainPresetLoaded { chain } if chain.0 == "chain_preset")),
        "expected ChainPresetLoaded event, got {:?}",
        events
    );
    let proj = project.borrow();
    let chain = proj
        .chains
        .iter()
        .find(|c| c.id.0 == "chain_preset")
        .unwrap();
    assert_eq!(chain.blocks.len(), 2, "chain should have 2 preset blocks");
    assert_eq!(chain.blocks[0].id.0, "blk_new_a");
    assert_eq!(chain.blocks[1].id.0, "blk_new_b");
}

#[test]
fn load_chain_preset_non_existent_chain_returns_err() {
    let block = make_core_block("blk_0", true);
    let project = make_project("chain_0", block);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::LoadChainPreset {
        chain: ChainId("chain_MISSING".to_string()),
        preset_instrument: "electric_guitar".to_string(),
        preset_blocks: vec![make_core_block("new_blk", true)],
    });

    assert!(result.is_err(), "expected Err for missing chain, got Ok");
    // Original chain must be unchanged
    let proj = project.borrow();
    assert_eq!(proj.chains[0].blocks.len(), 1, "chain must not be mutated");
    assert_eq!(proj.chains[0].blocks[0].id.0, "blk_0");
}

#[test]
fn load_chain_preset_empty_blocks_succeeds() {
    let existing_block = make_core_block("blk_old", true);
    let project = make_project("chain_preset", existing_block);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    // A zero-block preset is valid — the chain becomes empty.
    let result = dispatcher.dispatch(Command::LoadChainPreset {
        chain: ChainId("chain_preset".to_string()),
        preset_instrument: "electric_guitar".to_string(),
        preset_blocks: vec![],
    });

    assert!(
        result.is_ok(),
        "empty preset should succeed, got {:?}",
        result
    );
    let events = result.unwrap();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::ChainPresetLoaded { chain } if chain.0 == "chain_preset")),
        "expected ChainPresetLoaded even for empty preset, got {:?}",
        events
    );
    let proj = project.borrow();
    let chain = proj
        .chains
        .iter()
        .find(|c| c.id.0 == "chain_preset")
        .unwrap();
    assert!(
        chain.blocks.is_empty(),
        "chain should be empty after empty preset load"
    );
}

// ── Invariant: dispatch must not be called while caller holds a Project borrow ────

#[test]
fn dispatch_panics_if_caller_holds_external_immutable_borrow_of_project() {
    // Reproduces the real bug we hit in adapter-gui (recent_projects_wiring /
    // project_file_dialog_wiring): building the Command's `project` field via
    // `project_rc.borrow().clone()` keeps the immutable borrow alive until the
    // dispatch call returns. Inside dispatch, `self.project.borrow_mut()`
    // then panics with "RefCell already borrowed".
    //
    // This test pins the invariant: callers MUST drop every project borrow
    // BEFORE invoking dispatch. The fix on the adapter side is to clone into
    // a local variable first.
    let project = Project {
        name: None,
        device_settings: vec![],
        chains: vec![],
        midi: None,
    };
    let project_rc = Rc::new(RefCell::new(project));
    let dispatcher = LocalDispatcher::new(project_rc.clone());

    let _alive = project_rc.borrow(); // simulates an adapter holding `&Project`

    let new_project = Project {
        name: Some("loaded".into()),
        device_settings: vec![],
        chains: vec![],
        midi: None,
    };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        dispatcher.dispatch(Command::LoadProject {
            project: new_project,
            path: std::path::PathBuf::from("/tmp/x"),
        })
    }));

    assert!(
        result.is_err(),
        "dispatch is expected to panic when caller still holds a project borrow"
    );
}

#[test]
fn dispatch_succeeds_when_caller_drops_borrow_before_calling() {
    // Positive companion: after the borrow is dropped, dispatch goes through.
    let project = Project {
        name: None,
        device_settings: vec![],
        chains: vec![],
        midi: None,
    };
    let project_rc = Rc::new(RefCell::new(project));
    let dispatcher = LocalDispatcher::new(project_rc.clone());

    let snapshot = project_rc.borrow().clone(); // takes the borrow temporarily
    drop(snapshot); // (actually, .clone() already returns Project by value;
                    // the temporary borrow is gone after this line.)

    let new_project = Project {
        name: Some("loaded".into()),
        device_settings: vec![],
        chains: vec![],
        midi: None,
    };
    let result = dispatcher.dispatch(Command::LoadProject {
        project: new_project,
        path: std::path::PathBuf::from("/tmp/x"),
    });

    assert!(
        result.is_ok(),
        "dispatch without live borrow should succeed"
    );
    assert_eq!(
        project_rc.borrow().name.as_deref(),
        Some("loaded"),
        "loaded project replaced shared state"
    );
}

// ── SetChainVolume (issue #440, port to #295 command bus) ────────────────────

fn make_project_with_volume(chain_id: &str, volume: f32) -> Rc<RefCell<Project>> {
    Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        chains: vec![Chain {
            id: ChainId(chain_id.to_string()),
            description: None,
            instrument: "electric_guitar".to_string(),
            enabled: true,
            volume,
            io_binding_ids: vec![],
            blocks: vec![],
            di_output: None,
        }],
        midi: None,
    }))
}

#[test]
fn set_chain_volume_updates_value_and_emits_event() {
    let project = make_project_with_volume("chain_0", 100.0);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::SetChainVolume {
        chain: ChainId("chain_0".to_string()),
        value: 150.0,
    });

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    let events = result.unwrap();
    // Must emit ChainVolumeChanged + ProjectMutated
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::ChainVolumeChanged {
                chain,
                value,
            } if chain.0 == "chain_0" && (*value - 150.0).abs() < f32::EPSILON
        )),
        "expected ChainVolumeChanged event; got: {:?}",
        events
    );
    assert!(
        events.iter().any(|e| matches!(e, Event::ProjectMutated)),
        "expected ProjectMutated event; got: {:?}",
        events
    );
    assert!(
        (project.borrow().chains[0].volume - 150.0).abs() < f32::EPSILON,
        "chain.volume should be 150.0 after dispatch, got {}",
        project.borrow().chains[0].volume
    );
}

#[test]
fn set_chain_volume_non_existent_chain_returns_err() {
    let project = make_project_with_volume("chain_0", 100.0);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::SetChainVolume {
        chain: ChainId("chain_MISSING".to_string()),
        value: 150.0,
    });

    assert!(result.is_err(), "expected Err for missing chain, got Ok");
    assert!(
        (project.borrow().chains[0].volume - 100.0).abs() < f32::EPSILON,
        "volume must not be mutated when chain not found"
    );
}

#[test]
fn set_chain_volume_passes_extreme_values_verbatim() {
    // Policy: no clamp. Values 0.0 and 250.0 stored as-is.
    let project = make_project_with_volume("chain_0", 100.0);
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    dispatcher
        .dispatch(Command::SetChainVolume {
            chain: ChainId("chain_0".to_string()),
            value: 0.0,
        })
        .expect("dispatch 0.0 should succeed");
    assert!(
        project.borrow().chains[0].volume.abs() < f32::EPSILON,
        "volume=0.0 should be stored verbatim"
    );

    dispatcher
        .dispatch(Command::SetChainVolume {
            chain: ChainId("chain_0".to_string()),
            value: 250.0,
        })
        .expect("dispatch 250.0 should succeed");
    assert!(
        (project.borrow().chains[0].volume - 250.0).abs() < f32::EPSILON,
        "volume=250.0 should be stored verbatim"
    );
}

// ── #513 / #493: MIDI device + mapping + learn commands ──────────────────────

#[test]
fn save_midi_mapping_writes_bindings_into_project_midi() {
    let project = empty_project_rc();
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    let bindings = vec![project::midi::Binding {
        source: project::midi::Source::ProgramChange { program: 7 },
        command: "SaveProject".to_string(),
        args: serde_json::Value::Null,
        scale: None,
    }];

    let events = dispatcher
        .dispatch(Command::SaveMidiMapping {
            bindings: bindings.clone(),
        })
        .unwrap();

    assert_eq!(events, vec![Event::MidiMappingSaved, Event::ProjectMutated]);
    let stored = project
        .borrow()
        .midi
        .clone()
        .unwrap_or_default()
        .bindings
        .clone();
    assert_eq!(stored, bindings);
}

// #513 / #493: System / MIDI tests moved to
// `local_dispatcher_midi_system_tests.rs` (each file with its test).
// #513 / #540: System / Paths (presets + plugins) tests moved to
// `local_dispatcher_paths_tests.rs` so this file does not grow further
// (already over the per-file size cap) and so the FS-sandboxing helper
// (`$HOME` redirect) stays scoped to the tests that need it.
