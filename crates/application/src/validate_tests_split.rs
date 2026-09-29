//! #328 — `validate_project` walks both paths of a split and names the path
//! an invalid block sits in.

use super::helpers::*;
use project::block::split_block::{SplitBlock, SplitEnd};
use project::block::split_params::default_split_params;

fn unknown_delay(id: &str, enabled: bool) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.to_string()),
        enabled,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "delay".to_string(),
            model: "nonexistent_model".to_string(),
            params: ParameterSet::default(),
        }),
    }
}

fn stereo_delay(id: &str) -> AudioBlock {
    let model = block_delay::supported_models()
        .iter()
        .copied()
        .find(|model| {
            project::block::schema_for_block_model("delay", model)
                .map(|s| {
                    s.audio_mode
                        .output_layout(block_core::AudioChannelLayout::Stereo)
                        .is_some()
                })
                .unwrap_or(false)
        })
        .expect("block-delay exposes a stereo-capable model");
    let schema =
        project::block::schema_for_block_model("delay", model).expect("delay schema exists");
    let params = ParameterSet::default()
        .normalized_against(&schema)
        .expect("delay defaults normalize");
    AudioBlock {
        id: BlockId(id.to_string()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "delay".to_string(),
            model: model.to_string(),
            params,
        }),
    }
}

fn chain_with_split(a: Vec<AudioBlock>, b: Vec<AudioBlock>) -> Chain {
    test_chain(
        "chain:0",
        vec![
            test_input_block("dev-in", vec![0]),
            AudioBlock {
                id: BlockId("block:split".to_string()),
                enabled: true,
                kind: AudioBlockKind::Split(SplitBlock {
                    end: SplitEnd::Mix,
                    params: default_split_params(),
                    a,
                    b,
                }),
            },
            test_output_block("dev-out", vec![0, 1]),
        ],
    )
}

#[test]
fn validate_project_names_the_split_path_that_holds_an_invalid_block() {
    let project = test_project(vec![chain_with_split(
        vec![],
        vec![unknown_delay("block:bad", true)],
    )]);

    let message = validate_project(&project)
        .expect_err("path B holds an unknown model")
        .to_string();

    assert!(message.contains("path B"), "{message}");
    assert!(message.contains("block:bad"), "{message}");
}

/// Characterization pin: a disabled block inside a path is skipped, like a
/// disabled top-level block.
#[test]
fn validate_project_skips_a_disabled_block_inside_a_path() {
    let project = test_project(vec![chain_with_split(
        vec![unknown_delay("block:off", false)],
        vec![],
    )]);
    assert!(validate_project(&project).is_ok());
}

/// Characterization pin: a split whose paths hold valid blocks validates.
#[test]
fn validate_project_accepts_a_split_with_valid_paths() {
    let project = test_project(vec![chain_with_split(
        vec![stereo_delay("block:delay_a")],
        vec![stereo_delay("block:delay_b")],
    )]);
    assert!(validate_project(&project).is_ok());
}
