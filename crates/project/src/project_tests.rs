
use super::*;
use crate::block::{
    schema_for_block_model, AudioBlock, AudioBlockKind, CoreBlock, InputBlock, InputEntry,
    OutputBlock, OutputEntry, SelectBlock,
};
use crate::chain::{Chain, ChainInputMode, ChainOutputMode};
use crate::param::ParameterSet;
use domain::ids::{BlockId, ChainId, DeviceId};

fn make_input_block(id: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".to_string(),
            entries: vec![InputEntry {
                device_id: DeviceId("dev".into()),
                mode: ChainInputMode::Mono,
                channels: vec![0],
            }],
        }),
    }
}

fn make_output_block(id: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".to_string(),
            entries: vec![OutputEntry {
                device_id: DeviceId("dev".into()),
                mode: ChainOutputMode::Stereo,
                channels: vec![0, 1],
            }],
        }),
    }
}

fn make_delay_block(id: &str) -> AudioBlock {
    let model = block_delay::supported_models().first().unwrap();
    let schema = schema_for_block_model("delay", model).unwrap();
    let params = ParameterSet::default().normalized_against(&schema).unwrap();
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "delay".to_string(),
            model: model.to_string(),
            params,
        }),
    }
}

fn make_reverb_block(id: &str) -> AudioBlock {
    let model = block_reverb::supported_models().first().unwrap();
    let schema = schema_for_block_model("reverb", model).unwrap();
    let params = ParameterSet::default().normalized_against(&schema).unwrap();
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "reverb".to_string(),
            model: model.to_string(),
            params,
        }),
    }
}

fn make_chain(blocks: Vec<AudioBlock>) -> Chain {
    Chain {
        id: ChainId("chain:0".to_string()),
        description: None,
        instrument: "electric_guitar".to_string(),
        enabled: true,
        blocks,
    }
}

fn make_project(chains: Vec<Chain>) -> Project {
    Project {
        name: Some("Test Project".to_string()),
        device_settings: vec![],
        chains,
    }
}

// --- find_block tests ---

#[test]
fn find_block_top_level_found() {
    let delay = make_delay_block("b:delay");
    let project = make_project(vec![make_chain(vec![
        make_input_block("b:in"),
        delay.clone(),
        make_output_block("b:out"),
    ])]);
    let found = project.find_block(&BlockId("b:delay".into()));
    assert!(found.is_some());
    assert_eq!(found.unwrap().id.0, "b:delay");
}

#[test]
fn find_block_not_found_returns_none() {
    let project = make_project(vec![make_chain(vec![
        make_input_block("b:in"),
        make_output_block("b:out"),
    ])]);
    assert!(project.find_block(&BlockId("nonexistent".into())).is_none());
}

#[test]
fn find_block_nested_select_found() {
    let d1 = make_delay_block("sel::d1");
    let d2 = make_delay_block("sel::d2");
    let select = AudioBlock {
        id: BlockId("b:sel".into()),
        enabled: true,
        kind: AudioBlockKind::Select(SelectBlock {
            selected_block_id: BlockId("sel::d1".into()),
            options: vec![d1, d2],
        }),
    };
    let project = make_project(vec![make_chain(vec![
        make_input_block("b:in"),
        select,
        make_output_block("b:out"),
    ])]);

    // Find the select block itself
    let found = project.find_block(&BlockId("b:sel".into()));
    assert!(found.is_some());

    // Find a nested option inside the select
    let found = project.find_block(&BlockId("sel::d2".into()));
    assert!(found.is_some());
    assert_eq!(found.unwrap().id.0, "sel::d2");
}

#[test]
fn find_block_across_multiple_chains() {
    let chain1 = make_chain(vec![
        make_input_block("c1:in"),
        make_delay_block("c1:delay"),
        make_output_block("c1:out"),
    ]);
    let mut chain2 = make_chain(vec![
        make_input_block("c2:in"),
        make_reverb_block("c2:reverb"),
        make_output_block("c2:out"),
    ]);
    chain2.id = ChainId("chain:1".to_string());
    let project = make_project(vec![chain1, chain2]);

    assert!(project.find_block(&BlockId("c2:reverb".into())).is_some());
    assert!(project.find_block(&BlockId("c1:delay".into())).is_some());
}

// --- parameter_descriptors tests ---

#[test]
fn parameter_descriptors_empty_project_returns_empty() {
    let project = make_project(vec![]);
    let descs = project.parameter_descriptors().unwrap();
    assert!(descs.is_empty());
}

#[test]
fn parameter_descriptors_aggregates_across_chains() {
    let chain1 = make_chain(vec![
        make_input_block("c1:in"),
        make_delay_block("c1:delay"),
        make_output_block("c1:out"),
    ]);
    let mut chain2 = make_chain(vec![
        make_input_block("c2:in"),
        make_reverb_block("c2:reverb"),
        make_output_block("c2:out"),
    ]);
    chain2.id = ChainId("chain:1".to_string());
    let project = make_project(vec![chain1, chain2]);

    let descs = project.parameter_descriptors().unwrap();
    // Should contain descriptors from both delay and reverb blocks
    assert!(!descs.is_empty());
    let block_ids: Vec<_> = descs.iter().map(|d| d.block_id.0.clone()).collect();
    assert!(block_ids.contains(&"c1:delay".to_string()));
    assert!(block_ids.contains(&"c2:reverb".to_string()));
}

#[test]
fn parameter_descriptors_io_blocks_contribute_nothing() {
    let project = make_project(vec![make_chain(vec![
        make_input_block("b:in"),
        make_output_block("b:out"),
    ])]);
    let descs = project.parameter_descriptors().unwrap();
    assert!(descs.is_empty());
}

// --- find_parameter_descriptor tests ---

#[test]
fn find_parameter_descriptor_existing() {
    let project = make_project(vec![make_chain(vec![
        make_input_block("b:in"),
        make_delay_block("b:delay"),
        make_output_block("b:out"),
    ])]);
    let all_descs = project.parameter_descriptors().unwrap();
    assert!(!all_descs.is_empty());

    let first_id = &all_descs[0].id;
    let found = project.find_parameter_descriptor(first_id).unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().id, *first_id);
}

#[test]
fn find_parameter_descriptor_nonexistent_returns_none() {
    let project = make_project(vec![make_chain(vec![
        make_input_block("b:in"),
        make_output_block("b:out"),
    ])]);
    let fake_id = domain::ids::ParameterId("nonexistent".to_string());
    let found = project.find_parameter_descriptor(&fake_id).unwrap();
    assert!(found.is_none());
}

// --- block_audio_descriptors tests ---

#[test]
fn block_audio_descriptors_empty_project() {
    let project = make_project(vec![]);
    let descs = project.block_audio_descriptors().unwrap();
    assert!(descs.is_empty());
}

#[test]
fn block_audio_descriptors_aggregates_effect_blocks() {
    let chain = make_chain(vec![
        make_input_block("b:in"),
        make_delay_block("b:delay"),
        make_reverb_block("b:reverb"),
        make_output_block("b:out"),
    ]);
    let project = make_project(vec![chain]);
    let descs = project.block_audio_descriptors().unwrap();
    // input/output return empty, delay+reverb return one each
    assert_eq!(descs.len(), 2);
    let types: Vec<_> = descs.iter().map(|d| d.effect_type.as_str()).collect();
    assert!(types.contains(&"delay"));
    assert!(types.contains(&"reverb"));
}

#[test]
fn block_audio_descriptors_disabled_blocks_excluded() {
    let mut delay = make_delay_block("b:delay");
    delay.enabled = false;
    let chain = make_chain(vec![
        make_input_block("b:in"),
        delay,
        make_reverb_block("b:reverb"),
        make_output_block("b:out"),
    ]);
    let project = make_project(vec![chain]);
    let descs = project.block_audio_descriptors().unwrap();
    assert_eq!(descs.len(), 1);
    assert_eq!(descs[0].effect_type, "reverb");
}

// --- chain reorder tests (issue #246) ---

fn chain_with_id(id: &str) -> Chain {
    Chain {
        id: ChainId(id.to_string()),
        description: Some(id.to_string()),
        instrument: "electric_guitar".to_string(),
        enabled: true,
        blocks: vec![],
    }
}

fn ids(project: &Project) -> Vec<String> {
    project.chains.iter().map(|c| c.id.0.clone()).collect()
}

#[test]
fn move_chain_up_swaps_with_previous() {
    let mut project = make_project(vec![
        chain_with_id("a"),
        chain_with_id("b"),
        chain_with_id("c"),
    ]);
    assert!(project.move_chain_up(2));
    assert_eq!(ids(&project), vec!["a", "c", "b"]);
}

#[test]
fn move_chain_up_at_first_is_noop() {
    let mut project = make_project(vec![chain_with_id("a"), chain_with_id("b")]);
    assert!(!project.move_chain_up(0));
    assert_eq!(ids(&project), vec!["a", "b"]);
}

#[test]
fn move_chain_up_out_of_bounds_is_noop() {
    let mut project = make_project(vec![chain_with_id("a"), chain_with_id("b")]);
    assert!(!project.move_chain_up(99));
    assert_eq!(ids(&project), vec!["a", "b"]);
}

#[test]
fn move_chain_down_swaps_with_next() {
    let mut project = make_project(vec![
        chain_with_id("a"),
        chain_with_id("b"),
        chain_with_id("c"),
    ]);
    assert!(project.move_chain_down(0));
    assert_eq!(ids(&project), vec!["b", "a", "c"]);
}

#[test]
fn move_chain_down_at_last_is_noop() {
    let mut project = make_project(vec![chain_with_id("a"), chain_with_id("b")]);
    assert!(!project.move_chain_down(1));
    assert_eq!(ids(&project), vec!["a", "b"]);
}

#[test]
fn move_chain_down_out_of_bounds_is_noop() {
    let mut project = make_project(vec![chain_with_id("a"), chain_with_id("b")]);
    assert!(!project.move_chain_down(99));
    assert_eq!(ids(&project), vec!["a", "b"]);
}

#[test]
fn move_chain_preserves_chain_data() {
    let mut a = chain_with_id("a");
    a.enabled = false;
    a.instrument = "bass".to_string();
    let mut project = make_project(vec![a, chain_with_id("b")]);
    project.move_chain_down(0);
    let moved = &project.chains[1];
    assert_eq!(moved.id.0, "a");
    assert!(!moved.enabled);
    assert_eq!(moved.instrument, "bass");
}
