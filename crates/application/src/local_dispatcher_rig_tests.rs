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
        },
    );
    RigProject {
        name: None,
        inputs,
        outputs: BTreeMap::new(),
        presets,
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
