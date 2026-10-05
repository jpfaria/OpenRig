//! The project has one tempo: it drives every synced delay/modulation and
//! the drums, travels in `project.yaml`, and a preset load never changes it.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use block_core::tempo_sync::{TIME_PATH, TIME_SYNC_PATH};
use domain::ids::{BlockId, ChainId};
use domain::value_objects::ParameterValue;

use project::block::{normalize_block_params, AudioBlock, AudioBlockKind, CoreBlock};
use project::param::ParameterSet;
use project::project::Project;
use project::rig::{RigInput, RigPreset, RigProject};

use crate::command::{BlockCommand, Command, MetronomeCommand, RigNavKind, SelectionCommand};
use crate::dispatcher::CommandDispatcher;
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;

fn delay(id: &str, sync: &str) -> AudioBlock {
    let mut params = ParameterSet::default();
    params.insert(TIME_SYNC_PATH, ParameterValue::String(sync.to_string()));
    params.insert(TIME_PATH, ParameterValue::Float(333.0));
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "delay".into(),
            model: "digital_clean".into(),
            params: normalize_block_params("delay", "digital_clean", params).expect("params"),
        }),
    }
}

fn input(bank: &[(usize, &str)]) -> RigInput {
    RigInput {
        label: None,
        bank: bank.iter().map(|(i, k)| (*i, k.to_string())).collect(),
        active_preset: bank[0].0,
        active_scene: 1,
        routing: vec![],
        instrument: "electric_guitar".to_string(),
        io: String::new(),
        endpoint: String::new(),
        io_binding_ids: Vec::new(),
        loopers: Vec::new(),
        disabled_endpoints: Default::default(),
        mix: Default::default(),
        di_output: None,
    }
}

/// Input `a`: presets `a1` and `a2` (both a synced 1/4 delay). Input `b`:
/// preset `b1` with a free (unsynced) delay.
fn rig() -> RigProject {
    let mut presets = BTreeMap::new();
    presets.insert(
        "a1".to_string(),
        RigPreset::from_legacy_blocks(vec![delay("da1", "1/4")], 100.0),
    );
    presets.insert(
        "a2".to_string(),
        RigPreset::from_legacy_blocks(vec![delay("da2", "1/4")], 100.0),
    );
    presets.insert(
        "b1".to_string(),
        RigPreset::from_legacy_blocks(vec![delay("db1", "off")], 100.0),
    );
    let mut inputs = BTreeMap::new();
    inputs.insert("a".to_string(), input(&[(1, "a1"), (2, "a2")]));
    inputs.insert("b".to_string(), input(&[(1, "b1")]));
    RigProject {
        name: None,
        inputs,
        outputs: BTreeMap::new(),
        presets,
        midi: None,
        chain_order: Vec::new(),
        bpm: None,
    }
}

struct Fixture {
    dispatcher: LocalDispatcher,
    project: Rc<RefCell<Project>>,
    rig: Rc<RefCell<RigProject>>,
}

fn fixture() -> Fixture {
    let rig = Rc::new(RefCell::new(rig()));
    let project = Rc::new(RefCell::new(engine::rig_runtime::rig_to_legacy_project(
        &rig.borrow(),
        &BTreeSet::new(),
    )));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    dispatcher.attach_rig(Rc::clone(&rig));
    Fixture {
        dispatcher,
        project,
        rig,
    }
}

fn chain_a() -> ChainId {
    ChainId("rig:a".into())
}

fn chain_b() -> ChainId {
    ChainId("rig:b".into())
}

fn param(project: &Project, chain: &ChainId, block: &str, path: &str) -> ParameterValue {
    let chain = project
        .chains
        .iter()
        .find(|c| &c.id == chain)
        .expect("chain");
    let block = chain
        .blocks
        .iter()
        .find(|b| b.id.0 == block)
        .expect("block");
    let AudioBlockKind::Core(core) = &block.kind else {
        panic!("core block expected");
    };
    core.params.get(path).cloned().expect(path)
}

fn time_ms(f: &Fixture, chain: &ChainId, block: &str) -> f32 {
    match param(&f.project.borrow(), chain, block, TIME_PATH) {
        ParameterValue::Float(v) => v,
        other => panic!("time_ms is {other:?}"),
    }
}

fn set_bpm(f: &Fixture, bpm: f32) -> Vec<Event> {
    f.dispatcher
        .dispatch(Command::Metronome(MetronomeCommand::SetMetronomeBpm {
            bpm,
        }))
        .expect("SetMetronomeBpm")
}

fn global_bpm(f: &Fixture) -> f32 {
    f.dispatcher.metronome_snapshot().settings.bpm
}

#[test]
fn changing_the_global_bpm_retimes_synced_delays() {
    let f = fixture();
    let events = set_bpm(&f, 100.0);
    assert!((time_ms(&f, &chain_a(), "da1") - 600.0).abs() < 1e-3);
    assert!(events.contains(&Event::ChainTempoRetimed { chain: chain_a() }));
}

#[test]
fn a_free_delay_ignores_the_global_bpm() {
    let f = fixture();
    let events = set_bpm(&f, 120.0);
    assert!((time_ms(&f, &chain_b(), "db1") - 333.0).abs() < 1e-3);
    assert!(!events.contains(&Event::ChainTempoRetimed { chain: chain_b() }));
}

#[test]
fn picking_a_sync_value_retimes_the_block_at_once() {
    let f = fixture();
    set_bpm(&f, 100.0);
    f.dispatcher
        .dispatch(Command::Block(BlockCommand::SelectBlockParameterOption {
            chain: chain_b(),
            block: BlockId("db1".into()),
            path: TIME_SYNC_PATH.into(),
            value: "1/8".into(),
            index: 8,
        }))
        .expect("select sync");
    assert!((time_ms(&f, &chain_b(), "db1") - 300.0).abs() < 1e-3);
}

#[test]
fn picking_a_sync_value_on_a_block_saved_without_one_retimes_it() {
    let f = fixture();
    {
        let mut project = f.project.borrow_mut();
        let chain = project
            .chains
            .iter_mut()
            .find(|c| c.id == chain_b())
            .expect("chain");
        let block = chain
            .blocks
            .iter_mut()
            .find(|b| b.id.0 == "db1")
            .expect("block");
        let AudioBlockKind::Core(core) = &mut block.kind else {
            panic!("core block expected");
        };
        core.params.values.remove(TIME_SYNC_PATH);
    }
    set_bpm(&f, 100.0);
    f.dispatcher
        .dispatch(Command::Block(BlockCommand::SelectBlockParameterOption {
            chain: chain_b(),
            block: BlockId("db1".into()),
            path: TIME_SYNC_PATH.into(),
            value: "1/4".into(),
            index: 5,
        }))
        .expect("select sync on a block saved without one");
    assert!((time_ms(&f, &chain_b(), "db1") - 600.0).abs() < 1e-3);
}

#[test]
fn turning_the_time_knob_by_hand_turns_sync_off() {
    let f = fixture();
    f.dispatcher
        .dispatch(Command::Block(BlockCommand::SetBlockParameterNumber {
            chain: chain_a(),
            block: BlockId("da1".into()),
            path: TIME_PATH.into(),
            value: 410.0,
        }))
        .expect("set time");
    assert_eq!(
        param(&f.project.borrow(), &chain_a(), "da1", TIME_SYNC_PATH),
        ParameterValue::String("off".into())
    );
    set_bpm(&f, 120.0);
    assert!((time_ms(&f, &chain_a(), "da1") - 410.0).abs() < 1e-3);
}

#[test]
fn changing_the_bpm_stores_it_in_the_project() {
    let f = fixture();
    let events = set_bpm(&f, 97.0);
    assert_eq!(f.rig.borrow().bpm, Some(97.0));
    assert!(events.contains(&Event::ProjectMutated));
}

#[test]
fn the_drums_follow_the_project_bpm() {
    let f = fixture();
    set_bpm(&f, 97.0);
    assert_eq!(f.dispatcher.drums_snapshot().bpm, 97.0);
}

#[test]
fn attaching_a_project_with_a_bpm_sets_the_tempo() {
    let mut loaded = rig();
    loaded.bpm = Some(90.0);
    let rig = Rc::new(RefCell::new(loaded));
    let project = Rc::new(RefCell::new(engine::rig_runtime::rig_to_legacy_project(
        &rig.borrow(),
        &BTreeSet::new(),
    )));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    dispatcher.attach_rig(Rc::clone(&rig));
    assert_eq!(dispatcher.metronome_snapshot().settings.bpm, 90.0);
    assert_eq!(dispatcher.drums_snapshot().bpm, 90.0);
    let f = Fixture {
        dispatcher,
        project,
        rig,
    };
    assert!((time_ms(&f, &chain_a(), "da1") - 60000.0 / 90.0).abs() < 1e-2);
}

#[test]
fn a_project_without_a_bpm_starts_at_the_default_tempo() {
    let f = fixture();
    assert_eq!(global_bpm(&f), feature_dsp::metronome::BPM_DEFAULT);
}

#[test]
fn loading_a_preset_keeps_the_project_tempo_and_retimes() {
    let f = fixture();
    set_bpm(&f, 150.0);
    let events = f
        .dispatcher
        .dispatch(Command::Selection(SelectionCommand::ApplyRigNav {
            chain: chain_a(),
            kind: RigNavKind::Preset(1),
        }))
        .expect("switch to a2");
    assert_eq!(global_bpm(&f), 150.0);
    assert!(!events
        .iter()
        .any(|e| matches!(e, Event::MetronomeBpmChanged { .. })));
    assert!((time_ms(&f, &chain_a(), "da2") - 400.0).abs() < 1e-3);
}

#[test]
fn a_scene_switch_keeps_the_project_tempo() {
    let f = fixture();
    set_bpm(&f, 140.0);
    f.dispatcher
        .dispatch(Command::Selection(SelectionCommand::ApplyRigNav {
            chain: chain_a(),
            kind: RigNavKind::Scene(1),
        }))
        .expect("scene 1");
    assert_eq!(global_bpm(&f), 140.0);
}
