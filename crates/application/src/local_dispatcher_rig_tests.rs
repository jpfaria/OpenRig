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
