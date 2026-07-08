//! Issue #780 — folding live VST3 controller values into each block's
//! ParameterSet. Pure and deterministic: the live-value reader is injected as a
//! seam, so no plugin is needed. Proves the save-path fold writes `p{id}`
//! percent keyed by BlockId, clears stale entries, and leaves other blocks be.

use block_core::param::set::ParameterSet;
use domain::ids::{BlockId, ChainId};
use domain::value_objects::ParameterValue;
use project::block::{AudioBlock, AudioBlockKind, CoreBlock};
use project::chain::Chain;
use project::project::Project;
use project::vst3_capture::capture_live_vst3_params_with;

fn core_block(id: &str, effect_type: &str, params: ParameterSet) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: effect_type.into(),
            model: "vst3:Chow:Fx".into(),
            params,
        }),
    }
}

fn project_with(blocks: Vec<AudioBlock>) -> Project {
    Project {
        name: None,
        device_settings: Vec::new(),
        midi: None,
        chains: vec![Chain {
            id: ChainId("rig:gtr".into()),
            description: None,
            instrument: "electric_guitar".into(),
            enabled: true,
            volume: 100.0,
            io_binding_ids: Vec::new(),
            blocks,
            di_output: None,
        }],
    }
}

fn core_params(p: &Project) -> &ParameterSet {
    match &p.chains[0].blocks[0].kind {
        AudioBlockKind::Core(c) => &c.params,
        _ => panic!("expected core block"),
    }
}

#[test]
fn writes_live_params_as_percent_keyed_by_block_id() {
    let mut p = project_with(vec![core_block(
        "blk-A",
        block_core::EFFECT_TYPE_VST3,
        ParameterSet::default(),
    )]);
    capture_live_vst3_params_with(&mut p, |key| {
        (key == "blk-A").then(|| vec![(2u32, 0.5f64), (7u32, 1.0f64)])
    });
    let params = core_params(&p);
    assert_eq!(params.get("p2"), Some(&ParameterValue::Float(50.0)));
    assert_eq!(params.get("p7"), Some(&ParameterValue::Float(100.0)));
}

#[test]
fn clears_stale_p_entries_before_writing() {
    let mut stale = ParameterSet::default();
    stale.insert("p9", ParameterValue::Float(42.0));
    let mut p = project_with(vec![core_block(
        "blk-A",
        block_core::EFFECT_TYPE_VST3,
        stale,
    )]);
    capture_live_vst3_params_with(&mut p, |_| Some(vec![(2u32, 0.25f64)]));
    let params = core_params(&p);
    assert_eq!(params.get("p2"), Some(&ParameterValue::Float(25.0)));
    assert!(
        params.get("p9").is_none(),
        "a param returned to default must not linger as a stale p-entry"
    );
}

#[test]
fn leaves_non_vst3_blocks_untouched() {
    let mut other = ParameterSet::default();
    other.insert("mix", ParameterValue::Float(30.0));
    let mut p = project_with(vec![core_block("blk-A", "reverb", other)]);
    capture_live_vst3_params_with(&mut p, |_| Some(vec![(1u32, 0.9f64)]));
    let params = core_params(&p);
    assert_eq!(params.get("mix"), Some(&ParameterValue::Float(30.0)));
    assert!(
        params.get("p1").is_none(),
        "a non-VST3 block must not be touched by the VST3 capture"
    );
}

#[test]
fn no_context_leaves_the_block_params_as_is() {
    let mut existing = ParameterSet::default();
    existing.insert("p3", ParameterValue::Float(12.0));
    let mut p = project_with(vec![core_block(
        "blk-A",
        block_core::EFFECT_TYPE_VST3,
        existing,
    )]);
    // Reader returns None (plugin not live) → params must be preserved verbatim.
    capture_live_vst3_params_with(&mut p, |_| None);
    let params = core_params(&p);
    assert_eq!(params.get("p3"), Some(&ParameterValue::Float(12.0)));
}
