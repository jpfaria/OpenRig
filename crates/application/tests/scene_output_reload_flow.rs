//! Hypothesis: the live "scene change wipes output" bug survives the
//! dispatcher fix because the **persistence layer** writes the rig YAML
//! (`.openrig`) without the user's output endpoint, and the next reload
//! rebuilds the chain via `rig_to_legacy_project` -- which has no output
//! to project (rig.outputs is empty for the chain).
//!
//! Live sequence:
//!   1. user dispatches save_chain_output_endpoints → legacy chain gets output
//!   2. user dispatches ApplyRigNav → dispatcher preserves output in legacy chain
//!   3. autosave serializes legacy + rig; rig.outputs is still empty
//!   4. on the next reload (or runtime sync that re-projects from rig),
//!      `rig_to_legacy_project` rebuilds the chain WITHOUT outputs

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
use project::rig_sync::sync_synthetic_into_rig;

use application::command::Command;
use application::dispatcher::CommandDispatcher;
use application::local_dispatcher::LocalDispatcher;

const CHAIN_ID: &str = "rig:in";
const OUTPUT_BLOCK_ID: &str = "rig:in:out";
const INPUT_BLOCK_ID: &str = "rig:in:in";
const DEVICE: &str = "test:device";

fn user_input() -> AudioBlock {
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

fn user_output() -> AudioBlock {
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

fn fresh_rig() -> RigProject {
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

/// Build (rig, project) where the legacy chain has user's I/O + an effect block.
fn fresh_state() -> (RigProject, Project) {
    let rig = fresh_rig();
    let mut project = engine::rig_runtime::rig_to_legacy_project(&rig, &BTreeSet::new());
    for c in project.chains.iter_mut() {
        if c.id.0 == CHAIN_ID {
            c.enabled = true;
            c.blocks.retain(|b| {
                !matches!(b.kind, AudioBlockKind::Input(_) | AudioBlockKind::Output(_))
            });
            c.blocks.insert(0, user_input());
            c.blocks.push(core("amp:1"));
            c.blocks.push(user_output());
        }
    }
    (rig, project)
}

fn outputs_count(project: &Project) -> usize {
    // Chain.id is not always preserved across YAML round-trip; sum all
    // output blocks across all chains. The test rig has exactly one chain.
    project
        .chains
        .iter()
        .flat_map(|c| c.blocks.iter())
        .filter(|b| matches!(b.kind, AudioBlockKind::Output(_)))
        .count()
}

fn outputs_count_for_chain_id(project: &Project, chain_id: &str) -> usize {
    project
        .chains
        .iter()
        .find(|c| c.id.0 == chain_id)
        .map(|c| {
            c.blocks
                .iter()
                .filter(|b| matches!(b.kind, AudioBlockKind::Output(_)))
                .count()
        })
        .unwrap_or(0)
}

#[test]
fn sync_synthetic_into_rig_does_not_propagate_output_endpoint_to_rig() {
    // Documents the gap: the sync function captures effect blocks and
    // input sources back into the rig, but it does NOT write outputs.
    // After sync, rig.outputs is still empty.
    let (mut rig, project) = fresh_state();
    sync_synthetic_into_rig(&mut rig, &project);
    assert!(
        rig.outputs.is_empty(),
        "sync writes outputs to rig today? {:?}",
        rig.outputs
    );
}

#[test]
fn rig_to_legacy_project_rebuild_after_sync_has_no_output() {
    // Direct consequence of the previous test: rebuilding the chain from
    // the synced rig gives a chain WITHOUT the user's output. Any code
    // path that swaps the synthetic chain with this rebuild (without
    // explicit preservation) wipes the output.
    let (mut rig, project) = fresh_state();
    sync_synthetic_into_rig(&mut rig, &project);
    let rebuilt = engine::rig_runtime::rig_to_legacy_project(&rig, &BTreeSet::new());
    assert_eq!(
        outputs_count(&rebuilt),
        0,
        "rig→legacy produces no output for a chain whose rig.outputs is empty"
    );
}

#[test]
fn full_simulated_save_then_reload_after_rig_nav_drops_output_today() {
    // End-to-end repro of what happens between an autosave and the next
    // open: dispatcher preserves output across rig-nav, sync captures
    // edits into rig, then on next load the chain is rebuilt purely from
    // rig (where outputs are missing) and the user's output is gone.
    let rig = Rc::new(RefCell::new(fresh_rig()));
    let project = Rc::new(RefCell::new(engine::rig_runtime::rig_to_legacy_project(
        &rig.borrow(),
        &BTreeSet::new(),
    )));
    for c in project.borrow_mut().chains.iter_mut() {
        if c.id.0 == CHAIN_ID {
            c.enabled = true;
            c.blocks.retain(|b| {
                !matches!(b.kind, AudioBlockKind::Input(_) | AudioBlockKind::Output(_))
            });
            c.blocks.insert(0, user_input());
            c.blocks.push(core("amp:1"));
            c.blocks.push(user_output());
        }
    }
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    dispatcher.attach_rig(Rc::clone(&rig));

    // 1) dispatch a scene step (live action)
    let _ = dispatcher.dispatch(Command::ApplyRigNav {
        chain: ChainId(CHAIN_ID.into()),
        kind: application::command::RigNavKind::StepScene(1),
    });
    assert_eq!(
        outputs_count_for_chain_id(&project.borrow(), CHAIN_ID),
        1,
        "in-memory: dispatcher fix preserves output across rig-nav"
    );

    // 2) "autosave": persist rig + project
    let rig_yaml = serde_yaml::to_string(&*rig.borrow()).expect("serialize rig");
    let project_yaml = serde_yaml::to_string(&*project.borrow()).expect("serialize project");

    // 3) "reload": rebuild rig + project from YAML, then re-project chain via rig
    let reloaded_rig: RigProject = serde_yaml::from_str(&rig_yaml).expect("rig deserialize");
    let reloaded_project_from_rig =
        engine::rig_runtime::rig_to_legacy_project(&reloaded_rig, &BTreeSet::new());

    // The legacy YAML round-trip itself preserves the output (it's just
    // a serde struct), so reading the legacy YAML alone would keep the
    // output. The bug is the *next* layer: any code path that rebuilds
    // from rig overrides the legacy chain.
    let legacy_round_trip: Project =
        serde_yaml::from_str(&project_yaml).expect("project deserialize");
    assert_eq!(
        outputs_count(&legacy_round_trip),
        1,
        "legacy YAML alone keeps the output"
    );

    // This assertion documents the gap: the rig-driven reload loses the
    // user's output. The fix has to make the rig the source of truth for
    // I/O too, OR the load path has to merge the legacy chain's I/O onto
    // the rig-projected chain.
    assert_eq!(
        outputs_count(&reloaded_project_from_rig),
        0,
        "rig→legacy on reload drops the output (this is the wider bug class)"
    );
}
