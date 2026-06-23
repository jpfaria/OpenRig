//! #436 architectural fix: rig-nav must flow through Command/dispatcher
//! (not be applied by hand in the GUI). The dispatcher owns the rig and
//! `Command::ApplyRigNav` does what the old `reproject` closure did —
//! switch the active preset/scene and re-project the synthetic chain.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use domain::ids::{BlockId, ChainId, DeviceId};

use project::block::{AudioBlock, AudioBlockKind, CoreBlock, InputEntry};
use project::chain::ChainInputMode;
use project::param::ParameterSet;
use project::rig::{RigInput, RigPreset, RigProject};

use crate::command::{Command, RigNavKind};
use crate::dispatcher::CommandDispatcher;
use crate::local_dispatcher::LocalDispatcher;

fn core(id: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "gain".into(),
            model: "volume".into(),
            params: ParameterSet::default(),
        }),
    }
}

fn rig() -> RigProject {
    let mut presets = BTreeMap::new();
    presets.insert(
        "p1".to_string(),
        RigPreset::from_legacy_blocks(vec![core("A")], 100.0),
    );
    presets.insert(
        "p2".to_string(),
        RigPreset::from_legacy_blocks(vec![core("B")], 100.0),
    );
    let mut inputs = BTreeMap::new();
    inputs.insert(
        "in".to_string(),
        RigInput {
            label: None,
            sources: vec![InputEntry {
                device_id: DeviceId("d".into()),
                mode: ChainInputMode::Mono,
                channels: vec![0],
            }],
            bank: BTreeMap::from([(1, "p1".to_string()), (2, "p2".to_string())]),
            active_preset: 1,
            active_scene: 1,
            routing: vec![],
            instrument: "electric_guitar".to_string(),
            io: String::new(),
            endpoint: String::new(),
        },
    );
    RigProject {
        name: None,
        inputs,
        outputs: BTreeMap::new(),
        presets,
        midi: None,
        chain_order: Vec::new(),
    }
}

#[test]
fn apply_rig_nav_switches_preset_and_reprojects_the_synthetic_chain() {
    let rig = Rc::new(RefCell::new(rig()));
    // The legacy Project the dispatcher mutates is the projected rig.
    let project = Rc::new(RefCell::new(engine::rig_runtime::rig_to_legacy_project(
        &rig.borrow(),
        &std::collections::BTreeSet::new(),
    )));

    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    dispatcher.attach_rig(Rc::clone(&rig));

    // Sanity: starts on p1 → block "A".
    let core_ids = |p: &project::project::Project| {
        p.chains[0]
            .blocks
            .iter()
            .filter_map(|b| match &b.kind {
                AudioBlockKind::Core(_) => Some(b.id.0.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(core_ids(&project.borrow()), vec!["A"]);

    // The GUI dispatches this instead of mutating session.rig itself.
    let events = dispatcher
        .dispatch(Command::ApplyRigNav {
            chain: ChainId("rig:in".into()),
            // Preset position 1 → 2nd bank key (2) == "p2" (the GUI
            // sends this exact sentinel today).
            kind: RigNavKind::Preset(1),
        })
        .expect("dispatch ok");

    assert_eq!(rig.borrow().inputs["in"].active_preset, 2, "rig mutated");
    assert_eq!(
        core_ids(&project.borrow()),
        vec!["B"],
        "dispatcher re-projected the synthetic chain in the Project"
    );
    assert!(!events.is_empty(), "an event must be emitted");
}

#[test]
fn apply_rig_nav_step_preset_advances_then_wraps() {
    let rig = Rc::new(RefCell::new(rig()));
    let project = Rc::new(RefCell::new(engine::rig_runtime::rig_to_legacy_project(
        &rig.borrow(),
        &std::collections::BTreeSet::new(),
    )));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    dispatcher.attach_rig(Rc::clone(&rig));

    let core_ids = |p: &project::project::Project| {
        p.chains[0]
            .blocks
            .iter()
            .filter_map(|b| match &b.kind {
                AudioBlockKind::Core(_) => Some(b.id.0.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(core_ids(&project.borrow()), vec!["A"], "starts on p1");

    // Footswitch "next preset": relative step, no fixed position.
    dispatcher
        .dispatch(Command::ApplyRigNav {
            chain: ChainId("rig:in".into()),
            kind: RigNavKind::StepPreset(1),
        })
        .expect("dispatch ok");
    assert_eq!(rig.borrow().inputs["in"].active_preset, 2, "advanced to p2");
    assert_eq!(core_ids(&project.borrow()), vec!["B"]);

    // Next again wraps back to the first preset.
    dispatcher
        .dispatch(Command::ApplyRigNav {
            chain: ChainId("rig:in".into()),
            kind: RigNavKind::StepPreset(1),
        })
        .expect("dispatch ok");
    assert_eq!(rig.borrow().inputs["in"].active_preset, 1, "wrapped to p1");
}

#[test]
fn apply_rig_nav_step_scene_advances() {
    let r = {
        let mut r = rig();
        r.presets
            .get_mut("p1")
            .unwrap()
            .scenes
            .insert(2, project::rig::RigScene::default());
        r
    };
    let rig = Rc::new(RefCell::new(r));
    let project = Rc::new(RefCell::new(engine::rig_runtime::rig_to_legacy_project(
        &rig.borrow(),
        &std::collections::BTreeSet::new(),
    )));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    dispatcher.attach_rig(Rc::clone(&rig));

    dispatcher
        .dispatch(Command::ApplyRigNav {
            chain: ChainId("rig:in".into()),
            kind: RigNavKind::StepScene(1),
        })
        .expect("dispatch ok");
    assert_eq!(rig.borrow().inputs["in"].active_scene, 2, "scene advanced");
}

#[test]
fn remove_chain_also_drops_the_rig_input_not_just_the_legacy_chain() {
    // The GUI used to call rig.remove_input() by hand after dispatching
    // RemoveChain — business logic in the UI. RemoveChain must drop the
    // RigInput itself, or the next re-projection resurrects the chain.
    let mut r = rig();
    r.inputs.insert(
        "in-b".to_string(),
        RigInput {
            label: None,
            sources: vec![InputEntry {
                device_id: DeviceId("e".into()),
                mode: ChainInputMode::Mono,
                channels: vec![1],
            }],
            bank: BTreeMap::from([(1, "p1".to_string())]),
            active_preset: 1,
            active_scene: 1,
            routing: vec![],
            instrument: "electric_guitar".to_string(),
            io: String::new(),
            endpoint: String::new(),
        },
    );
    let rig = Rc::new(RefCell::new(r));
    let project = Rc::new(RefCell::new(engine::rig_runtime::rig_to_legacy_project(
        &rig.borrow(),
        &std::collections::BTreeSet::new(),
    )));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    dispatcher.attach_rig(Rc::clone(&rig));

    dispatcher
        .dispatch(Command::RemoveChain {
            chain: ChainId("rig:in-b".into()),
        })
        .expect("dispatch ok");

    assert!(
        !rig.borrow().inputs.contains_key("in-b"),
        "RemoveChain must drop the RigInput, not only the legacy chain"
    );
    assert!(
        !project.borrow().chains.iter().any(|c| c.id.0 == "rig:in-b"),
        "legacy chain gone too"
    );
}

#[test]
fn capture_rig_edits_writes_synthetic_chain_back_into_the_rig() {
    // The GUI save path called sync_synthetic_into_rig() by hand —
    // model mutation in the UI. It must be a Command instead.
    let rig = Rc::new(RefCell::new(rig())); // input "in", preset p1 = [A]
    let project = Rc::new(RefCell::new(engine::rig_runtime::rig_to_legacy_project(
        &rig.borrow(),
        &std::collections::BTreeSet::new(),
    )));
    // User edited the synthetic chain: structurally different blocks.
    for c in project.borrow_mut().chains.iter_mut() {
        if c.id.0 == "rig:in" {
            c.blocks
                .retain(|b| matches!(b.kind, AudioBlockKind::Input(_) | AudioBlockKind::Output(_)));
            c.blocks.push(core("EDITED"));
        }
    }

    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    dispatcher.attach_rig(Rc::clone(&rig));
    dispatcher
        .dispatch(Command::CaptureRigEdits)
        .expect("dispatch ok");

    let active = rig.borrow().inputs["in"].active_preset;
    let name = rig.borrow().inputs["in"].bank[&active].clone();
    let ids: Vec<String> = rig.borrow().presets[&name]
        .blocks
        .iter()
        .map(|b| b.id.0.clone())
        .collect();
    assert_eq!(
        ids,
        vec!["EDITED"],
        "CaptureRigEdits must fold the synthetic edit back into the rig"
    );
}

#[test]
fn rename_rig_preset_changes_what_the_select_shows() {
    // User repro: "troquei o nome para SILVERCHAIR FREAK - SCARLETT e
    // continua mostrando outro nome no select". The select shows
    // preset.name; nothing could change it after migration → there must
    // be a Command that renames the active preset.
    let rig = Rc::new(RefCell::new(rig())); // input "in", active preset key
    let project = Rc::new(RefCell::new(engine::rig_runtime::rig_to_legacy_project(
        &rig.borrow(),
        &std::collections::BTreeSet::new(),
    )));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    dispatcher.attach_rig(Rc::clone(&rig));

    dispatcher
        .dispatch(Command::RenameRigPreset {
            chain: ChainId("rig:in".into()),
            name: "SILVERCHAIR FREAK - SCARLETT".into(),
        })
        .expect("dispatch ok");

    let active = rig.borrow().inputs["in"].active_preset;
    let key = rig.borrow().inputs["in"].bank[&active].clone();
    assert_eq!(
        rig.borrow().presets[&key].name.as_deref(),
        Some("SILVERCHAIR FREAK - SCARLETT"),
        "rename must update the active preset's name (what the select shows)"
    );
}

// #436: "clicar num bloco" must be reachable by MIDI/MCP, so block
// selection has to be Command-driven and owned by the dispatcher (not
// GUI-only state). RED: Command::SelectChainBlock + a queryable
// selection don't exist yet.
#[test]
fn select_chain_block_command_sets_dispatcher_owned_selection() {
    let project = Rc::new(RefCell::new(engine::rig_runtime::rig_to_legacy_project(
        &rig(),
        &std::collections::BTreeSet::new(),
    )));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    dispatcher
        .dispatch(Command::SelectChainBlock {
            chain: ChainId("rig:in".into()),
            block_index: 2,
        })
        .expect("dispatch ok");

    assert_eq!(
        dispatcher.selected_block(&ChainId("rig:in".into())),
        Some(2),
        "the dispatcher must own the selection so MIDI/MCP can drive it"
    );
}

// ── Issue #502: chain reorder must persist for rig sessions ──────────────────

fn two_input_rig() -> RigProject {
    let mut presets = BTreeMap::new();
    presets.insert(
        "pa".to_string(),
        RigPreset::from_legacy_blocks(vec![core("A")], 100.0),
    );
    presets.insert(
        "pb".to_string(),
        RigPreset::from_legacy_blocks(vec![core("B")], 100.0),
    );
    let mut inputs = BTreeMap::new();
    for (name, preset) in &[("alpha", "pa"), ("beta", "pb")] {
        inputs.insert(
            (*name).to_string(),
            RigInput {
                label: None,
                sources: vec![InputEntry {
                    device_id: DeviceId("d".into()),
                    mode: ChainInputMode::Mono,
                    channels: vec![0],
                }],
                bank: BTreeMap::from([(1, (*preset).to_string())]),
                active_preset: 1,
                active_scene: 1,
                routing: vec![],
                instrument: "electric_guitar".to_string(),
                io: String::new(),
                endpoint: String::new(),
            },
        );
    }
    RigProject {
        name: None,
        inputs,
        outputs: BTreeMap::new(),
        presets,
        midi: None,
        chain_order: Vec::new(),
    }
}

#[test]
fn move_chain_up_then_capture_writes_chain_order_into_rig() {
    // Two inputs alphabetical = ["alpha", "beta"]; project starts with
    // synthetic chains in that order. User clicks ▲ on the "beta"
    // chain → MoveChainUp swaps them. CaptureRigEdits must persist the
    // new order to RigProject.chain_order so a later
    // `save_rig_project_file` keeps it through reload.
    let rig = Rc::new(RefCell::new(two_input_rig()));
    let project = Rc::new(RefCell::new(engine::rig_runtime::rig_to_legacy_project(
        &rig.borrow(),
        &std::collections::BTreeSet::new(),
    )));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    dispatcher.attach_rig(Rc::clone(&rig));

    dispatcher
        .dispatch(Command::MoveChainUp {
            chain: ChainId("rig:beta".into()),
        })
        .expect("MoveChainUp ok");
    dispatcher
        .dispatch(Command::CaptureRigEdits)
        .expect("CaptureRigEdits ok");

    assert_eq!(
        rig.borrow().chain_order,
        vec!["beta".to_string(), "alpha".to_string()],
        "the rig must remember the user's chain order so reload preserves it"
    );
}

#[test]
fn rig_to_legacy_project_after_capture_reflects_persisted_chain_order() {
    // Round-trip the persistence path: after the move + capture, a
    // fresh projection (what `load_project_any` + projection does on
    // reopen) must yield chains in the persisted order.
    let rig = Rc::new(RefCell::new(two_input_rig()));
    let project = Rc::new(RefCell::new(engine::rig_runtime::rig_to_legacy_project(
        &rig.borrow(),
        &std::collections::BTreeSet::new(),
    )));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    dispatcher.attach_rig(Rc::clone(&rig));

    dispatcher
        .dispatch(Command::MoveChainUp {
            chain: ChainId("rig:beta".into()),
        })
        .expect("MoveChainUp ok");
    dispatcher
        .dispatch(Command::CaptureRigEdits)
        .expect("CaptureRigEdits ok");

    let reprojected = engine::rig_runtime::rig_to_legacy_project(
        &rig.borrow(),
        &std::collections::BTreeSet::new(),
    );
    let ids: Vec<String> = reprojected.chains.iter().map(|c| c.id.0.clone()).collect();
    assert_eq!(
        ids,
        vec!["rig:beta".to_string(), "rig:alpha".to_string()],
        "the rig→project projection must honour the persisted chain order"
    );
}

// ── Issue #535: AddScene on the active preset must not bleed into siblings ──

#[test]
fn apply_rig_nav_add_scene_only_grows_active_preset_not_other_bank_presets() {
    // User repro (#535): chain "in" has presets A (slot 1) and B (slot 2);
    // A is active. Adding a 2nd scene must grow ONLY preset A — preset B
    // sits idle in the same bank and must keep its single scene.
    let rig = Rc::new(RefCell::new(rig())); // bank {1: "p1", 2: "p2"}, active=1
    let project = Rc::new(RefCell::new(engine::rig_runtime::rig_to_legacy_project(
        &rig.borrow(),
        &std::collections::BTreeSet::new(),
    )));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    dispatcher.attach_rig(Rc::clone(&rig));

    assert_eq!(rig.borrow().presets["p1"].scene_count(), 1, "A starts at 1");
    assert_eq!(rig.borrow().presets["p2"].scene_count(), 1, "B starts at 1");

    // GUI's "+" on the scene bar dispatches this exact sentinel.
    dispatcher
        .dispatch(Command::ApplyRigNav {
            chain: ChainId("rig:in".into()),
            kind: RigNavKind::Scene(-1),
        })
        .expect("dispatch ok");

    assert_eq!(
        rig.borrow().presets["p1"].scene_count(),
        2,
        "active preset A must grow to 2 scenes"
    );
    assert_eq!(
        rig.borrow().presets["p2"].scene_count(),
        1,
        "sibling preset B must NOT receive a scene — it is not the active preset"
    );
}

#[test]
fn add_scene_on_a_then_switch_to_b_keeps_b_at_one_scene_in_the_pool() {
    // User repro (#535) on the full flow: from the Chains screen, add a
    // 2nd scene to preset A, then switch the combobox to preset B. The
    // preset pool entry for B must keep its single scene (no sibling
    // contamination from the AddScene call, no re-projection side effect).
    let rig = Rc::new(RefCell::new(rig())); // bank {1: "p1", 2: "p2"}, active=1
    let project = Rc::new(RefCell::new(engine::rig_runtime::rig_to_legacy_project(
        &rig.borrow(),
        &std::collections::BTreeSet::new(),
    )));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    dispatcher.attach_rig(Rc::clone(&rig));

    dispatcher
        .dispatch(Command::ApplyRigNav {
            chain: ChainId("rig:in".into()),
            kind: RigNavKind::Scene(-1),
        })
        .expect("add scene ok");
    dispatcher
        .dispatch(Command::ApplyRigNav {
            chain: ChainId("rig:in".into()),
            kind: RigNavKind::Preset(1),
        })
        .expect("switch ok");

    assert_eq!(
        rig.borrow().presets["p2"].scene_count(),
        1,
        "preset B must still expose a single scene after the round-trip"
    );
}

#[test]
fn add_scene_on_a_switch_to_b_then_save_keeps_b_at_one_scene() {
    // User repro (#535): the leak surfaces only after a save round-trip
    // ("entro e saio do projeto ele aparece"). The save path dispatches
    // CaptureRigEdits → sync_synthetic_into_rig, which calls
    // write_back_processing_blocks on the new active preset using the
    // STALE active_scene carried over from the previous preset (A's
    // scene 2). That call's `preset.scenes.entry(scene_idx).or_default()`
    // materializes a spurious scene 2 in B.
    let rig = Rc::new(RefCell::new(rig())); // bank {1: "p1", 2: "p2"}, active=1
    let project = Rc::new(RefCell::new(engine::rig_runtime::rig_to_legacy_project(
        &rig.borrow(),
        &std::collections::BTreeSet::new(),
    )));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    dispatcher.attach_rig(Rc::clone(&rig));

    dispatcher
        .dispatch(Command::ApplyRigNav {
            chain: ChainId("rig:in".into()),
            kind: RigNavKind::Scene(-1),
        })
        .expect("add scene ok");
    dispatcher
        .dispatch(Command::ApplyRigNav {
            chain: ChainId("rig:in".into()),
            kind: RigNavKind::Preset(1),
        })
        .expect("switch ok");
    // The save path runs CaptureRigEdits before serializing.
    dispatcher
        .dispatch(Command::CaptureRigEdits)
        .expect("capture ok");

    assert_eq!(
        rig.borrow().presets["p2"].scene_count(),
        1,
        "after add-scene-on-A → switch-to-B → save, preset B must still have a single scene"
    );
}
