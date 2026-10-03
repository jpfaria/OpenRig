use super::*;

use block_core::param::ParameterSet;
use block_core::tempo_sync::{RATE_PATH, RATE_SYNC_PATH, TIME_PATH, TIME_SYNC_PATH};
use domain::ids::BlockId;
use domain::value_objects::ParameterValue;

use crate::block::{normalize_block_params, AudioBlock, AudioBlockKind, CoreBlock, SelectBlock};

fn core(id: &str, effect_type: &str, model: &str, edits: &[(&str, ParameterValue)]) -> AudioBlock {
    let mut params = ParameterSet::default();
    for (path, value) in edits {
        params.insert(*path, value.clone());
    }
    let params = normalize_block_params(effect_type, model, params).expect("params");
    AudioBlock {
        id: BlockId(id.to_string()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: effect_type.to_string(),
            model: model.to_string(),
            params,
        }),
    }
}

fn sync(value: &str) -> ParameterValue {
    ParameterValue::String(value.to_string())
}

fn f32_param(block: &AudioBlock, path: &str) -> f32 {
    let AudioBlockKind::Core(core) = &block.kind else {
        panic!("not a core block");
    };
    core.params.get_f32(path).expect(path)
}

#[test]
fn a_synced_delay_follows_the_bpm() {
    let mut blocks = vec![core(
        "d",
        "delay",
        "digital_clean",
        &[(TIME_SYNC_PATH, sync("1/4"))],
    )];
    assert!(retime_blocks(&mut blocks, 120.0));
    assert!((f32_param(&blocks[0], TIME_PATH) - 500.0).abs() < 1e-3);
}

#[test]
fn a_delay_with_sync_off_keeps_its_time() {
    let mut blocks = vec![core(
        "d",
        "delay",
        "digital_clean",
        &[(TIME_PATH, ParameterValue::Float(333.0))],
    )];
    assert!(!retime_blocks(&mut blocks, 120.0));
    assert!((f32_param(&blocks[0], TIME_PATH) - 333.0).abs() < 1e-3);
}

#[test]
fn a_synced_time_is_clamped_to_the_param_range() {
    // A whole note at 40 BPM is 6 s, past the 2 s ceiling.
    let mut blocks = vec![core(
        "d",
        "delay",
        "digital_clean",
        &[(TIME_SYNC_PATH, sync("1/1"))],
    )];
    retime_blocks(&mut blocks, 40.0);
    assert!((f32_param(&blocks[0], TIME_PATH) - 2000.0).abs() < 1e-3);
}

#[test]
fn a_synced_modulation_rate_follows_the_bpm() {
    let mut blocks = vec![core(
        "t",
        "modulation",
        "tremolo_sine",
        &[
            (RATE_SYNC_PATH, sync("1/8")),
            (RATE_PATH, ParameterValue::Float(1.0)),
        ],
    )];
    assert!(retime_blocks(&mut blocks, 120.0));
    assert!((f32_param(&blocks[0], RATE_PATH) - 4.0).abs() < 1e-4);
}

#[test]
fn retiming_twice_at_the_same_bpm_reports_no_change() {
    let mut blocks = vec![core(
        "d",
        "delay",
        "digital_clean",
        &[(TIME_SYNC_PATH, sync("1/8d"))],
    )];
    assert!(retime_blocks(&mut blocks, 100.0));
    assert!(!retime_blocks(&mut blocks, 100.0));
}

#[test]
fn every_option_of_a_select_is_retimed() {
    let inner = core(
        "d",
        "delay",
        "digital_clean",
        &[(TIME_SYNC_PATH, sync("1/4"))],
    );
    let mut blocks = vec![AudioBlock {
        id: BlockId("sel".to_string()),
        enabled: true,
        kind: AudioBlockKind::Select(SelectBlock {
            selected_block_id: BlockId("other".to_string()),
            options: vec![inner],
        }),
    }];
    assert!(retime_blocks(&mut blocks, 60.0));
    let AudioBlockKind::Select(select) = &blocks[0].kind else {
        unreachable!()
    };
    assert!((f32_param(&select.options[0], TIME_PATH) - 1000.0).abs() < 1e-3);
}

#[test]
fn retime_block_only_touches_the_named_block() {
    let mut blocks = vec![
        core(
            "a",
            "delay",
            "digital_clean",
            &[(TIME_SYNC_PATH, sync("1/4"))],
        ),
        core(
            "b",
            "delay",
            "digital_clean",
            &[(TIME_SYNC_PATH, sync("1/4"))],
        ),
    ];
    let before_b = f32_param(&blocks[1], TIME_PATH);
    assert!(retime_block(&mut blocks[0], 120.0));
    assert!((f32_param(&blocks[0], TIME_PATH) - 500.0).abs() < 1e-3);
    assert_eq!(f32_param(&blocks[1], TIME_PATH), before_b);
}
