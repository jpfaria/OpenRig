//! Bug repro (user, post-fix): switching scene **with the chain enabled**
//! still wipes the user's output. The earlier `local_dispatcher_rig.rs`
//! preservation fix passes when the chain's `enabled` flag is `false`, but
//! the live behaviour with `enabled = true` regressed independently.
//!
//! This file pins the enabled-chain path: every rig-nav variant must keep
//! the user's I/O endpoints intact regardless of whether the chain is
//! enabled (i.e. the audio runtime is processing).

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use domain::ids::{BlockId, ChainId, DeviceId};
use project::block::{
    AudioBlock, AudioBlockKind, CoreBlock, InputBlock, InputEntry, OutputBlock, OutputEntry,
};
use project::chain::{ChainInputMode, ChainOutputMode};
use project::param::ParameterSet;
use project::project::Project;
use project::rig::{RigInput, RigPreset, RigProject};

use application::command::{Command, RigNavKind};
use application::dispatcher::CommandDispatcher;
use application::local_dispatcher::LocalDispatcher;

const CHAIN_ID: &str = "rig:in";
const OUTPUT_BLOCK_ID: &str = "rig:in:out";
const INPUT_BLOCK_ID: &str = "rig:in:in";
const DEVICE: &str = "test:device";

fn user_input_block() -> AudioBlock {
    AudioBlock {
        id: BlockId(INPUT_BLOCK_ID.into()),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".into(),
            io: String::new(),
            endpoint: String::new(),
            entries: vec![InputEntry {
                device_id: DeviceId(DEVICE.into()),
                mode: ChainInputMode::Mono,
                channels: vec![0],
            }],
        }),
    }
}

fn user_output_block() -> AudioBlock {
    AudioBlock {
        id: BlockId(OUTPUT_BLOCK_ID.into()),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".into(),
            io: String::new(),
            endpoint: String::new(),
            entries: vec![OutputEntry {
                device_id: DeviceId(DEVICE.into()),
                mode: ChainOutputMode::Stereo,
                channels: vec![0, 1],
            }],
        }),
    }
}

fn core_block(id: &str) -> AudioBlock {
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

fn rig_with_two_presets_two_scenes() -> RigProject {
    let mut presets = BTreeMap::new();
    presets.insert(
        "p1".into(),
        RigPreset::from_legacy_blocks(Vec::new(), 100.0),
    );
    presets.insert(
        "p2".into(),
        RigPreset::from_legacy_blocks(Vec::new(), 100.0),
    );
    let mut bank = BTreeMap::new();
    bank.insert(1, "p1".into());
    bank.insert(2, "p2".into());
    let mut inputs = BTreeMap::new();
    inputs.insert(
        "in".into(),
        RigInput {
            label: None,
            sources: vec![InputEntry {
                device_id: DeviceId(DEVICE.into()),
                mode: ChainInputMode::Mono,
                channels: vec![0],
            }],
            bank,
            active_preset: 1,
            active_scene: 1,
            routing: vec![],
            instrument: "electric_guitar".to_string(),
        },
    );
    RigProject {
        name: None,
        inputs,
        presets,
        outputs: BTreeMap::new(),
        chain_order: Vec::new(),
        midi: None,
    }
}

/// Build a dispatcher whose chain is **enabled** and has the user's I/O.
fn dispatcher_with_enabled_chain() -> (
    LocalDispatcher,
    Rc<RefCell<Project>>,
    Rc<RefCell<RigProject>>,
) {
    let rig = Rc::new(RefCell::new(rig_with_two_presets_two_scenes()));
    let project = Rc::new(RefCell::new(engine::rig_runtime::rig_to_legacy_project(
        &rig.borrow(),
        &BTreeSet::new(),
    )));
    for c in project.borrow_mut().chains.iter_mut() {
        if c.id.0 == CHAIN_ID {
            c.enabled = true; // <-- the bit that triggers the live bug
            c.blocks.retain(|b| {
                !matches!(b.kind, AudioBlockKind::Output(_) | AudioBlockKind::Input(_))
            });
            c.blocks.insert(0, user_input_block());
            c.blocks.push(core_block("eq:1"));
            c.blocks.push(core_block("amp:2"));
            c.blocks.push(user_output_block());
        }
    }
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    dispatcher.attach_rig(Rc::clone(&rig));
    (dispatcher, project, rig)
}

fn outputs_count(project: &Rc<RefCell<Project>>) -> usize {
    project
        .borrow()
        .chains
        .iter()
        .find(|c| c.id.0 == CHAIN_ID)
        .map(|c| {
            c.blocks
                .iter()
                .filter(|b| matches!(b.kind, AudioBlockKind::Output(_)))
                .count()
        })
        .unwrap_or(0)
}

fn inputs_count(project: &Rc<RefCell<Project>>) -> usize {
    project
        .borrow()
        .chains
        .iter()
        .find(|c| c.id.0 == CHAIN_ID)
        .map(|c| {
            c.blocks
                .iter()
                .filter(|b| matches!(b.kind, AudioBlockKind::Input(_)))
                .count()
        })
        .unwrap_or(0)
}

fn chain_enabled(project: &Rc<RefCell<Project>>) -> bool {
    project
        .borrow()
        .chains
        .iter()
        .find(|c| c.id.0 == CHAIN_ID)
        .map(|c| c.enabled)
        .unwrap_or(false)
}

macro_rules! enabled_chain_preserves_io {
    ($name:ident, $kind:expr) => {
        #[test]
        fn $name() {
            let (d, p, _r) = dispatcher_with_enabled_chain();
            assert!(chain_enabled(&p), "precondition: chain is enabled");
            assert_eq!(outputs_count(&p), 1, "precondition: 1 output");
            assert_eq!(inputs_count(&p), 1, "precondition: 1 input");
            let _ = d.dispatch(Command::ApplyRigNav {
                chain: ChainId(CHAIN_ID.into()),
                kind: $kind,
            });
            assert!(chain_enabled(&p), "enabled flag must survive the rig-nav");
            assert_eq!(
                outputs_count(&p),
                1,
                "ApplyRigNav {:?} on ENABLED chain must preserve user output \
                 (live bug repro)",
                $kind
            );
            assert_eq!(
                inputs_count(&p),
                1,
                "ApplyRigNav {:?} on ENABLED chain must preserve user input",
                $kind
            );
        }
    };
}

enabled_chain_preserves_io!(enabled_chain_scene_step_forward, RigNavKind::StepScene(1));
enabled_chain_preserves_io!(enabled_chain_scene_step_backward, RigNavKind::StepScene(-1));
enabled_chain_preserves_io!(enabled_chain_preset_step_forward, RigNavKind::StepPreset(1));
enabled_chain_preserves_io!(
    enabled_chain_preset_step_backward,
    RigNavKind::StepPreset(-1)
);
enabled_chain_preserves_io!(enabled_chain_scene_select_zero, RigNavKind::Scene(0));
enabled_chain_preserves_io!(enabled_chain_scene_select_one, RigNavKind::Scene(1));
enabled_chain_preserves_io!(enabled_chain_scene_add, RigNavKind::Scene(-1));
enabled_chain_preserves_io!(enabled_chain_scene_remove, RigNavKind::Scene(-2));
enabled_chain_preserves_io!(enabled_chain_preset_select_zero, RigNavKind::Preset(0));
enabled_chain_preserves_io!(enabled_chain_preset_select_one, RigNavKind::Preset(1));
enabled_chain_preserves_io!(enabled_chain_preset_add, RigNavKind::Preset(-1));
enabled_chain_preserves_io!(enabled_chain_preset_remove, RigNavKind::Preset(-2));

#[test]
fn enabled_chain_rename_rig_preset_preserves_io() {
    let (d, p, _r) = dispatcher_with_enabled_chain();
    let _ = d.dispatch(Command::RenameRigPreset {
        chain: ChainId(CHAIN_ID.into()),
        name: "Coldplay - Clocks".into(),
    });
    assert!(chain_enabled(&p));
    assert_eq!(outputs_count(&p), 1);
    assert_eq!(inputs_count(&p), 1);
}

#[test]
fn enabled_chain_capture_rig_edits_preserves_io() {
    let (d, p, _r) = dispatcher_with_enabled_chain();
    let _ = d.dispatch(Command::CaptureRigEdits);
    assert!(chain_enabled(&p));
    assert_eq!(outputs_count(&p), 1);
    assert_eq!(inputs_count(&p), 1);
}
