use super::*;

use domain::ids::BlockId;
use project::block::{AudioBlockKind, CoreBlock};
use project::param::ParameterSet;

const SUPERMASSIVE: &str = "vst3:ValhallaSupermassive:ValhallaSupermassive";

fn block(effect_type: &str, model: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId("issue-1081:block".into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: effect_type.into(),
            model: model.into(),
            params: ParameterSet::default(),
        }),
    }
}

fn vst3(model: &str) -> AudioBlock {
    block(block_core::EFFECT_TYPE_VST3, model)
}

#[test]
fn the_bundle_is_read_from_the_model_id() {
    assert_eq!(
        vst3_bundle(&vst3(SUPERMASSIVE)),
        Some("ValhallaSupermassive")
    );
    assert_eq!(
        vst3_bundle(&vst3("vst3:CloudReverb:CloudReverb")),
        Some("CloudReverb")
    );
    assert_eq!(vst3_bundle(&block("ir", "ir_as_martin_d35e")), None);
}

#[test]
fn a_vst3_with_no_live_instance_of_its_bundle_is_built_ahead() {
    let live = [
        LiveInstance::Vst3("CloudReverb".into()),
        LiveInstance::Unrelated,
    ];
    assert!(fresh_vst3_may_prebuild(&vst3(SUPERMASSIVE), &live));
    assert!(fresh_vst3_may_prebuild(&vst3(SUPERMASSIVE), &[]));
}

#[test]
fn a_vst3_whose_bundle_is_live_keeps_the_quiesced_path() {
    let live = [
        LiveInstance::Unrelated,
        LiveInstance::Vst3("ValhallaSupermassive".into()),
    ];
    assert!(!fresh_vst3_may_prebuild(&vst3(SUPERMASSIVE), &live));
}

#[test]
fn a_live_node_that_may_hide_a_vst3_keeps_the_quiesced_path() {
    assert!(!fresh_vst3_may_prebuild(
        &vst3(SUPERMASSIVE),
        &[LiveInstance::Opaque]
    ));
}

#[test]
fn a_vst3_whose_bundle_cannot_be_read_keeps_the_quiesced_path() {
    assert!(!fresh_vst3_may_prebuild(&vst3("vst3"), &[]));
    assert!(!fresh_vst3_may_prebuild(&vst3("vst3::Class"), &[]));
}
