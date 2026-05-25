//! Exhaustive coverage of the "scene change wipes output" class of bug.
//!
//! Live symptom (user, multiple repros): switching scene/preset on a chain
//! erases the configured Output block. Audio engine loses its sink, meter
//! freezes at -120 dBFS.
//!
//! The original fix in `local_dispatcher_rig.rs` preserves I/O across the
//! `ApplyRigNav` swap, but the user reports the bug still reproduces after
//! the fix. This test set covers every Command variant that mutates
//! `chain.blocks` to catch whatever path is still wiping the output.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use domain::ids::{BlockId, ChainId, DeviceId};
use project::block::{
    AudioBlock, AudioBlockKind, CoreBlock, InputBlock, InputEntry, OutputBlock, OutputEntry,
};
use project::chain::{Chain, ChainInputMode, ChainOutputMode};
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

/// Rig with one input, three presets, two scenes per preset.
fn rig_with_presets_and_scenes() -> RigProject {
    let mut presets = BTreeMap::new();
    presets.insert(
        "p1".into(),
        RigPreset::from_legacy_blocks(Vec::new(), 100.0),
    );
    presets.insert(
        "p2".into(),
        RigPreset::from_legacy_blocks(Vec::new(), 100.0),
    );
    presets.insert(
        "p3".into(),
        RigPreset::from_legacy_blocks(Vec::new(), 100.0),
    );
    let mut bank = BTreeMap::new();
    bank.insert(1, "p1".into());
    bank.insert(2, "p2".into());
    bank.insert(3, "p3".into());
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

fn dispatcher_with_user_io() -> (
    LocalDispatcher,
    Rc<RefCell<Project>>,
    Rc<RefCell<RigProject>>,
) {
    let rig = Rc::new(RefCell::new(rig_with_presets_and_scenes()));
    let project = Rc::new(RefCell::new(engine::rig_runtime::rig_to_legacy_project(
        &rig.borrow(),
        &BTreeSet::new(),
    )));
    for c in project.borrow_mut().chains.iter_mut() {
        if c.id.0 == CHAIN_ID {
            c.blocks.retain(|b| {
                !matches!(b.kind, AudioBlockKind::Output(_) | AudioBlockKind::Input(_))
            });
            // Inputs at head, a few effect blocks, output at tail
            c.blocks.insert(0, user_input_block());
            c.blocks.push(core_block("filter:1"));
            c.blocks.push(core_block("amp:2"));
            c.blocks.push(user_output_block());
        }
    }
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    dispatcher.attach_rig(Rc::clone(&rig));
    (dispatcher, project, rig)
}

fn count_blocks_of_kind<F>(project: &Rc<RefCell<Project>>, chain_id: &str, pred: F) -> usize
where
    F: Fn(&AudioBlockKind) -> bool,
{
    project
        .borrow()
        .chains
        .iter()
        .find(|c| c.id.0 == chain_id)
        .map(|c| c.blocks.iter().filter(|b| pred(&b.kind)).count())
        .unwrap_or(0)
}

fn outputs_count(project: &Rc<RefCell<Project>>, chain_id: &str) -> usize {
    count_blocks_of_kind(project, chain_id, |k| {
        matches!(k, AudioBlockKind::Output(_))
    })
}

fn inputs_count(project: &Rc<RefCell<Project>>, chain_id: &str) -> usize {
    count_blocks_of_kind(project, chain_id, |k| matches!(k, AudioBlockKind::Input(_)))
}

// ── ApplyRigNav: every variant must preserve I/O ──────────────────────────

macro_rules! preserve_io_after_dispatch {
    ($name:ident, $kind:expr) => {
        #[test]
        fn $name() {
            let (d, p, _r) = dispatcher_with_user_io();
            assert_eq!(outputs_count(&p, CHAIN_ID), 1, "precondition: 1 output");
            assert_eq!(inputs_count(&p, CHAIN_ID), 1, "precondition: 1 input");
            let _ = d.dispatch(Command::ApplyRigNav {
                chain: ChainId(CHAIN_ID.into()),
                kind: $kind,
            });
            assert_eq!(
                outputs_count(&p, CHAIN_ID),
                1,
                "ApplyRigNav {:?} must preserve user output",
                $kind
            );
            assert_eq!(
                inputs_count(&p, CHAIN_ID),
                1,
                "ApplyRigNav {:?} must preserve user input",
                $kind
            );
        }
    };
}

preserve_io_after_dispatch!(apply_rig_nav_scene_step_forward, RigNavKind::StepScene(1));
preserve_io_after_dispatch!(apply_rig_nav_scene_step_backward, RigNavKind::StepScene(-1));
preserve_io_after_dispatch!(apply_rig_nav_preset_step_forward, RigNavKind::StepPreset(1));
preserve_io_after_dispatch!(
    apply_rig_nav_preset_step_backward,
    RigNavKind::StepPreset(-1)
);
preserve_io_after_dispatch!(apply_rig_nav_scene_select_first, RigNavKind::Scene(0));
preserve_io_after_dispatch!(apply_rig_nav_scene_select_second, RigNavKind::Scene(1));
preserve_io_after_dispatch!(apply_rig_nav_scene_add, RigNavKind::Scene(-1));
preserve_io_after_dispatch!(apply_rig_nav_scene_remove, RigNavKind::Scene(-2));
preserve_io_after_dispatch!(apply_rig_nav_preset_select_first, RigNavKind::Preset(0));
preserve_io_after_dispatch!(apply_rig_nav_preset_select_second, RigNavKind::Preset(1));
preserve_io_after_dispatch!(apply_rig_nav_preset_add, RigNavKind::Preset(-1));
preserve_io_after_dispatch!(apply_rig_nav_preset_remove, RigNavKind::Preset(-2));

// ── Other rig commands ─────────────────────────────────────────────────────

#[test]
fn rename_rig_preset_preserves_io() {
    let (d, p, _r) = dispatcher_with_user_io();
    let _ = d.dispatch(Command::RenameRigPreset {
        chain: ChainId(CHAIN_ID.into()),
        name: "Renamed".into(),
    });
    assert_eq!(outputs_count(&p, CHAIN_ID), 1, "rename must keep output");
    assert_eq!(inputs_count(&p, CHAIN_ID), 1, "rename must keep input");
}

#[test]
fn capture_rig_edits_preserves_io() {
    let (d, p, _r) = dispatcher_with_user_io();
    let _ = d.dispatch(Command::CaptureRigEdits);
    assert_eq!(outputs_count(&p, CHAIN_ID), 1);
    assert_eq!(inputs_count(&p, CHAIN_ID), 1);
}

// ── Block CRUD on a rig chain — must not touch I/O ─────────────────────────

#[test]
fn add_block_preserves_io() {
    let (d, p, _r) = dispatcher_with_user_io();
    let _ = d.dispatch(Command::AddBlock {
        chain: ChainId(CHAIN_ID.into()),
        kind: "gain".into(),
        model_id: "volume".into(),
        position: 2,
    });
    assert_eq!(outputs_count(&p, CHAIN_ID), 1);
    assert_eq!(inputs_count(&p, CHAIN_ID), 1);
}

#[test]
fn remove_block_preserves_io() {
    let (d, p, _r) = dispatcher_with_user_io();
    let _ = d.dispatch(Command::RemoveBlock {
        chain: ChainId(CHAIN_ID.into()),
        block: BlockId("filter:1".into()),
    });
    assert_eq!(outputs_count(&p, CHAIN_ID), 1);
    assert_eq!(inputs_count(&p, CHAIN_ID), 1);
}

#[test]
fn toggle_block_enabled_preserves_io() {
    let (d, p, _r) = dispatcher_with_user_io();
    let _ = d.dispatch(Command::ToggleBlockEnabled {
        chain: ChainId(CHAIN_ID.into()),
        block: BlockId("filter:1".into()),
    });
    assert_eq!(outputs_count(&p, CHAIN_ID), 1);
    assert_eq!(inputs_count(&p, CHAIN_ID), 1);
}

// ── YAML round-trip — persist+reload must keep outputs ─────────────────────
// (chain.id is derived on load from the input block, not serialised verbatim;
// finding the chain by its in-memory id after round-trip would need to mirror
// that derivation. We rely on the first/only chain since the test rig has
// exactly one input.)

#[test]
fn project_yaml_round_trip_preserves_io() {
    let (d, p, _r) = dispatcher_with_user_io();
    let yaml = serde_yaml::to_string(&*p.borrow()).expect("serialize");
    let reloaded: Project = serde_yaml::from_str(&yaml).expect("deserialize");
    let chain = reloaded
        .chains
        .first()
        .expect("at least one chain after round-trip");
    let outs = chain
        .blocks
        .iter()
        .filter(|b| matches!(b.kind, AudioBlockKind::Output(_)))
        .count();
    let ins = chain
        .blocks
        .iter()
        .filter(|b| matches!(b.kind, AudioBlockKind::Input(_)))
        .count();
    assert_eq!(outs, 1, "YAML round-trip must keep output");
    assert_eq!(ins, 1, "YAML round-trip must keep input");
    let _ = d;
}

// ── rig_to_legacy_project on its own (no dispatch) ────────────────────────

#[test]
fn rig_to_legacy_project_when_rig_outputs_empty_chain_has_no_output() {
    // Documents the source of truth for the bug class: when `rig.outputs`
    // is empty, the projection produces a chain *without any output block*.
    // Any code path that swaps the synthetic chain with the projection
    // therefore wipes the user's output unless it explicitly preserves it.
    let rig = rig_with_presets_and_scenes();
    let project = engine::rig_runtime::rig_to_legacy_project(&rig, &BTreeSet::new());
    let chain = project.chains.iter().find(|c| c.id.0 == CHAIN_ID).unwrap();
    let outs = chain
        .blocks
        .iter()
        .filter(|b| matches!(b.kind, AudioBlockKind::Output(_)))
        .count();
    assert_eq!(
        outs, 0,
        "today rig_to_legacy_project produces no output when rig.outputs is empty. \
         If this assertion ever flips, the fix can be simpler."
    );
}

// ── ConfigureChain / SaveChain — chain-level rewrites ─────────────────────

fn replacement_chain_without_output() -> Chain {
    Chain {
        id: ChainId(CHAIN_ID.into()),
        description: Some("replacement".into()),
        instrument: "electric_guitar".into(),
        enabled: false,
        volume: 100.0,
        blocks: vec![user_input_block(), core_block("only-effect:1")],
    }
}

#[test]
fn configure_chain_without_output_in_payload_loses_output_today() {
    // This *documents* the current behavior, not necessarily the desired
    // one: if a caller dispatches `ConfigureChain` with a payload that
    // omits the output, the existing output is wiped. The GUI scene path
    // does not call ConfigureChain, so this is informational. If it
    // becomes a UX issue, the dispatcher should merge instead of replace.
    let (d, p, _r) = dispatcher_with_user_io();
    let _ = d.dispatch(Command::ConfigureChain {
        chain: replacement_chain_without_output(),
    });
    assert_eq!(
        outputs_count(&p, CHAIN_ID),
        0,
        "ConfigureChain replaces; caller is responsible for I/O"
    );
}

// ── Save endpoint commands are explicit replacements ──────────────────────

#[test]
fn save_chain_output_endpoints_with_empty_list_clears_output() {
    let (d, p, _r) = dispatcher_with_user_io();
    let _ = d.dispatch(Command::SaveChainOutputEndpoints {
        chain: ChainId(CHAIN_ID.into()),
        output_blocks: vec![],
    });
    assert_eq!(
        outputs_count(&p, CHAIN_ID),
        0,
        "save_chain_output_endpoints([]) explicitly clears outputs"
    );
}

#[test]
fn save_chain_output_endpoints_with_block_keeps_one() {
    let (d, p, _r) = dispatcher_with_user_io();
    let _ = d.dispatch(Command::SaveChainOutputEndpoints {
        chain: ChainId(CHAIN_ID.into()),
        output_blocks: vec![user_output_block()],
    });
    assert_eq!(outputs_count(&p, CHAIN_ID), 1);
}

// ── Full end-to-end: build like CABELINHO, then iterate every rig-nav ─────

#[test]
fn full_simulated_cabelinho_flow_keeps_output_across_every_rig_nav() {
    let (d, p, _r) = dispatcher_with_user_io();
    // Loop through every possible rig-nav variant in sequence; output must
    // never drop below 1.
    let cmds = [
        RigNavKind::StepScene(1),
        RigNavKind::StepScene(-1),
        RigNavKind::StepPreset(1),
        RigNavKind::StepPreset(-1),
        RigNavKind::Scene(0),
        RigNavKind::Preset(0),
    ];
    for kind in cmds {
        let _ = d.dispatch(Command::ApplyRigNav {
            chain: ChainId(CHAIN_ID.into()),
            kind: kind.clone(),
        });
        assert!(
            outputs_count(&p, CHAIN_ID) >= 1,
            "output dropped after ApplyRigNav {kind:?}"
        );
        assert!(
            inputs_count(&p, CHAIN_ID) >= 1,
            "input dropped after ApplyRigNav {kind:?}"
        );
    }
}
