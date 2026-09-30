//! #328 — the model walkers of spec §1.4 reach the blocks inside a split's
//! paths: lookup by id, project-wide parameter descriptors, scene resolution
//! and the load-time "disable what this machine cannot build" pass (#606).

use std::collections::BTreeMap;

use domain::ids::{BlockId, ChainId, ParameterId};
use domain::value_objects::ParameterValue;
use project::block::split_params::MIX_PAN_A;
use project::block::{
    find_block_mut, for_each_block_mut, schema_for_block_model, walk_blocks, AudioBlock,
    AudioBlockKind, CoreBlock, SplitBlock, SplitEnd,
};
use project::chain::Chain;
use project::param::ParameterSet;
use project::project::Project;
use project::project_disable_unavailable::disable_unavailable_blocks;
use project::rig::{RigPreset, RigScene};

fn core(id: &str, effect_type: &str, model: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: effect_type.into(),
            model: model.into(),
            params: ParameterSet::default(),
        }),
    }
}

fn amp(id: &str, gain: f32) -> AudioBlock {
    let mut block = core(id, "amp", "m1");
    if let AudioBlockKind::Core(c) = &mut block.kind {
        c.params.insert("gain", ParameterValue::Float(gain));
    }
    block
}

fn split(a: Vec<AudioBlock>, b: Vec<AudioBlock>) -> AudioBlock {
    AudioBlock {
        id: BlockId("split".into()),
        enabled: true,
        kind: AudioBlockKind::Split(SplitBlock {
            a,
            b,
            ..SplitBlock::new(SplitEnd::Mix)
        }),
    }
}

/// Paths read straight off the split, independent of the walkers under test.
fn paths(block: &AudioBlock) -> (&[AudioBlock], &[AudioBlock]) {
    match &block.kind {
        AudioBlockKind::Split(s) => (&s.a, &s.b),
        other => panic!("expected a split, got {}", other.label()),
    }
}

fn gain(block: &AudioBlock) -> Option<f32> {
    match &block.kind {
        AudioBlockKind::Core(c) => c.params.get_f32("gain"),
        _ => None,
    }
}

fn project_of(blocks: Vec<AudioBlock>) -> Project {
    Project {
        name: None,
        device_settings: Vec::new(),
        chains: vec![Chain {
            mix: Default::default(),
            id: ChainId("c".into()),
            description: None,
            instrument: "electric_guitar".into(),
            enabled: true,
            volume: 100.0,
            io_binding_ids: Vec::new(),
            blocks,
            di_output: None,
            loopers: Vec::new(),
            disabled_endpoints: Default::default(),
        }],
        midi: None,
    }
}

#[test]
fn the_walk_visits_the_split_then_path_a_then_path_b() {
    let blocks = vec![
        amp("drive", 0.5),
        split(vec![amp("a1", 0.5), amp("a2", 0.5)], vec![amp("b1", 0.5)]),
        amp("reverb", 0.5),
    ];
    let walked: Vec<&str> = walk_blocks(&blocks)
        .into_iter()
        .map(|b| b.id.0.as_str())
        .collect();
    assert_eq!(walked, vec!["drive", "split", "a1", "a2", "b1", "reverb"]);

    let mut visited = Vec::new();
    let mut blocks = blocks;
    for_each_block_mut(&mut blocks, &mut |b| visited.push(b.id.0.clone()));
    assert_eq!(visited, vec!["drive", "split", "a1", "a2", "b1", "reverb"]);
}

#[test]
fn a_path_block_is_found_by_id() {
    let mut blocks = vec![split(vec![amp("a1", 0.5)], vec![amp("b1", 0.5)])];
    find_block_mut(&mut blocks, "b1")
        .expect("path B's block is reachable")
        .enabled = false;
    assert!(!paths(&blocks[0]).1[0].enabled);
    assert!(
        project_of(blocks)
            .find_block(&BlockId("a1".into()))
            .is_some(),
        "Project::find_block reaches path A"
    );
}

#[test]
fn the_parameters_of_path_blocks_are_addressable_project_wide() {
    let model = block_delay::supported_models()
        .first()
        .expect("a native delay")
        .to_string();
    let schema = schema_for_block_model("delay", &model).expect("delay schema");
    let params = ParameterSet::default()
        .normalized_against(&schema)
        .expect("defaults");
    let echo = AudioBlock {
        id: BlockId("echo".into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "delay".into(),
            model,
            params,
        }),
    };
    let project = project_of(vec![split(vec![echo], vec![])]);
    let nested = ParameterId::for_block_path(&BlockId("echo".into()), &schema.parameters[0].path);
    assert!(
        project
            .find_parameter_descriptor(&nested)
            .expect("describe")
            .is_some(),
        "a path block's knob is found"
    );
    let knob = ParameterId::for_block_path(&BlockId("split".into()), MIX_PAN_A);
    assert!(
        project
            .find_parameter_descriptor(&knob)
            .expect("describe")
            .is_some(),
        "the mixer's knob is found"
    );
}

#[test]
fn a_scene_reaches_the_path_blocks_and_the_split_knobs() {
    let mut preset = RigPreset::from_legacy_blocks(
        vec![split(vec![amp("amp_a", 0.5)], vec![amp("amp_b", 0.5)])],
        100.0,
    );
    preset.scene_params = vec!["amp_a.gain".into(), "split.mix_pan_a".into()];
    preset.scenes = BTreeMap::from([(
        2,
        RigScene {
            label: None,
            bypass: BTreeMap::from([("amp_b".to_string(), true)]),
            params: BTreeMap::from([
                ("amp_a.gain".to_string(), 0.9),
                ("split.mix_pan_a".to_string(), -50.0),
            ]),
            volume: None,
        },
    )]);
    let blocks = preset.apply_scene(2);
    let (a, b) = paths(&blocks[0]);
    assert_eq!(
        gain(&a[0]),
        Some(0.9),
        "path A's knob takes the scene value"
    );
    assert!(!b[0].enabled, "path B's block takes the scene bypass");
    let AudioBlockKind::Split(s) = &blocks[0].kind else {
        unreachable!()
    };
    assert_eq!(
        s.params.get_f32(MIX_PAN_A),
        Some(-50.0),
        "the mixer knob takes the scene value"
    );
}

#[test]
fn a_path_block_whose_model_is_missing_is_disabled_on_load() {
    let mut project = project_of(vec![split(
        vec![
            core("ts9", "gain", "ibanez_ts9"),
            core("missing", "gain", "nam_uninstalled_pedal_for_issue_328"),
        ],
        vec![],
    )]);
    let disabled = disable_unavailable_blocks(&mut project);
    assert_eq!(disabled, vec![BlockId("missing".into())]);
    let blocks = &project.chains[0].blocks;
    assert!(blocks[0].enabled, "the split itself stays on");
    let (a, _) = paths(&blocks[0]);
    assert!(a[0].enabled, "an available path block stays on");
    assert!(
        !a[1].enabled,
        "#606 inside a path: the unavailable block is switched off"
    );
}
