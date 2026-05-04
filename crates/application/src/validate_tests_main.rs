//! `validate_project` integration tests. Helpers via sibling `helpers` mod.

use super::helpers::*;

// validate_project — happy path

#[test]
fn validate_project_valid_project_succeeds() {
    let project = valid_project();
    assert!(validate_project(&project).is_ok());
}

#[test]
fn validate_project_mono_input_mono_output_succeeds() {
    let project = test_project(vec![test_chain(
        "chain:0",
        vec![
            test_input_block("dev-in", vec![0]),
            test_output_block("dev-out", vec![0]),
        ],
    )]);
    assert!(validate_project(&project).is_ok());
}

#[test]
fn validate_project_stereo_input_stereo_output_succeeds() {
    let project = test_project(vec![test_chain(
        "chain:0",
        vec![
            test_input_block("dev-in", vec![0, 1]),
            test_output_block("dev-out", vec![0, 1]),
        ],
    )]);
    assert!(validate_project(&project).is_ok());
}

#[test]
fn validate_project_multiple_chains_succeeds() {
    let chain0 = test_chain(
        "chain:0",
        vec![
            test_input_block("dev-in-a", vec![0]),
            test_output_block("dev-out-a", vec![0, 1]),
        ],
    );
    let chain1 = test_chain(
        "chain:1",
        vec![
            test_input_block("dev-in-b", vec![0]),
            test_output_block("dev-out-b", vec![0, 1]),
        ],
    );
    let project = test_project(vec![chain0, chain1]);
    assert!(validate_project(&project).is_ok());
}

// -----------------------------------------------------------------------
// validate_project — empty chains
// -----------------------------------------------------------------------

#[test]
fn validate_project_empty_chains_fails() {
    let project = Project {
        name: Some("test".to_string()),
        device_settings: Vec::new(),
        chains: Vec::new(),
    };
    let err = validate_project(&project).unwrap_err();
    assert!(err.to_string().contains("no chains configured"));
}

// -----------------------------------------------------------------------
// validate_project — missing input/output blocks
// -----------------------------------------------------------------------

#[test]
fn validate_project_no_input_block_fails() {
    let chain = test_chain("chain:0", vec![test_output_block("dev-out", vec![0, 1])]);
    let project = test_project(vec![chain]);
    let err = validate_project(&project).unwrap_err();
    assert!(err.to_string().contains("no input blocks"));
}

#[test]
fn validate_project_no_output_block_fails() {
    let chain = test_chain("chain:0", vec![test_input_block("dev-in", vec![0])]);
    let project = test_project(vec![chain]);
    let err = validate_project(&project).unwrap_err();
    assert!(err.to_string().contains("no output blocks"));
}

// -----------------------------------------------------------------------
// validate_project — empty device_id in entries
// -----------------------------------------------------------------------

#[test]
fn validate_project_empty_input_device_id_fails() {
    let chain = test_chain(
        "chain:0",
        vec![
            test_input_block("", vec![0]),
            test_output_block("dev-out", vec![0, 1]),
        ],
    );
    let project = test_project(vec![chain]);
    let err = validate_project(&project).unwrap_err();
    assert!(err.to_string().contains("missing device_id"));
}

#[test]
fn validate_project_whitespace_input_device_id_fails() {
    let chain = test_chain(
        "chain:0",
        vec![
            test_input_block("  ", vec![0]),
            test_output_block("dev-out", vec![0, 1]),
        ],
    );
    // Fix up the device settings since whitespace device_id won't auto-generate settings
    let project = Project {
        name: Some("test".to_string()),
        device_settings: vec![test_device_settings("dev-out")],
        chains: vec![chain],
    };
    let err = validate_project(&project).unwrap_err();
    assert!(err.to_string().contains("missing device_id"));
}

#[test]
fn validate_project_empty_output_device_id_fails() {
    let chain = test_chain(
        "chain:0",
        vec![
            test_input_block("dev-in", vec![0]),
            test_output_block("", vec![0, 1]),
        ],
    );
    let project = test_project(vec![chain]);
    let err = validate_project(&project).unwrap_err();
    assert!(err.to_string().contains("missing device_id"));
}

// -----------------------------------------------------------------------
// validate_project — empty channels
// -----------------------------------------------------------------------

#[test]
fn validate_project_empty_input_channels_fails() {
    let chain = test_chain(
        "chain:0",
        vec![
            test_input_block("dev-in", vec![]),
            test_output_block("dev-out", vec![0, 1]),
        ],
    );
    let project = test_project(vec![chain]);
    let err = validate_project(&project).unwrap_err();
    assert!(err.to_string().contains("has no channels"));
}

#[test]
fn validate_project_empty_output_channels_fails() {
    let chain = test_chain(
        "chain:0",
        vec![
            test_input_block("dev-in", vec![0]),
            test_output_block("dev-out", vec![]),
        ],
    );
    let project = test_project(vec![chain]);
    let err = validate_project(&project).unwrap_err();
    assert!(err.to_string().contains("has no channels"));
}

// -----------------------------------------------------------------------
// validate_project — duplicate channels
// -----------------------------------------------------------------------

#[test]
fn validate_project_duplicate_input_channels_fails() {
    let chain = test_chain(
        "chain:0",
        vec![
            test_input_block("dev-in", vec![0, 0]),
            test_output_block("dev-out", vec![0, 1]),
        ],
    );
    let project = test_project(vec![chain]);
    let err = validate_project(&project).unwrap_err();
    assert!(err.to_string().contains("duplicated channel"));
}

#[test]
fn validate_project_duplicate_output_channels_fails() {
    let chain = test_chain(
        "chain:0",
        vec![
            test_input_block("dev-in", vec![0]),
            test_output_block("dev-out", vec![1, 1]),
        ],
    );
    let project = test_project(vec![chain]);
    let err = validate_project(&project).unwrap_err();
    assert!(err.to_string().contains("duplicated channel"));
}

// -----------------------------------------------------------------------
// validate_project — device settings validation
// -----------------------------------------------------------------------

#[test]
fn validate_project_zero_sample_rate_fails() {
    let mut project = valid_project();
    project.device_settings[0].sample_rate = 0;
    let err = validate_project(&project).unwrap_err();
    assert!(err.to_string().contains("invalid sample_rate"));
}

#[test]
fn validate_project_zero_buffer_size_fails() {
    let mut project = valid_project();
    project.device_settings[0].buffer_size_frames = 0;
    let err = validate_project(&project).unwrap_err();
    assert!(err.to_string().contains("invalid buffer_size_frames"));
}

#[test]
fn validate_project_empty_device_settings_device_id_fails() {
    let mut project = valid_project();
    project.device_settings.push(DeviceSettings {
        device_id: DeviceId("".to_string()),
        sample_rate: 48000,
        buffer_size_frames: 256,
        bit_depth: 32,
        #[cfg(target_os = "linux")]
        realtime: true,
        #[cfg(target_os = "linux")]
        rt_priority: 70,
        #[cfg(target_os = "linux")]
        nperiods: 3,
    });
    let err = validate_project(&project).unwrap_err();
    assert!(err.to_string().contains("missing device_id"));
}

#[test]
fn validate_project_duplicate_device_settings_fails() {
    let mut project = valid_project();
    let dup = project.device_settings[0].clone();
    project.device_settings.push(dup);
    let err = validate_project(&project).unwrap_err();
    assert!(err.to_string().contains("duplicated device_settings"));
}

// -----------------------------------------------------------------------
// validate_project — channel conflicts between active chains
// -----------------------------------------------------------------------

#[test]
fn validate_project_active_chains_same_input_channel_fails() {
    let chain0 = test_chain(
        "chain:0",
        vec![
            test_input_block("dev-in", vec![0]),
            test_output_block("dev-out-a", vec![0, 1]),
        ],
    );
    let chain1 = test_chain(
        "chain:1",
        vec![
            test_input_block("dev-in", vec![0]), // same device+channel
            test_output_block("dev-out-b", vec![0, 1]),
        ],
    );
    let project = test_project(vec![chain0, chain1]);
    let err = validate_project(&project).unwrap_err();
    assert!(err.to_string().contains("both use input device"));
}

#[test]
fn validate_project_active_chains_different_channels_succeeds() {
    let chain0 = test_chain(
        "chain:0",
        vec![
            test_input_block("dev-in", vec![0]),
            test_output_block("dev-out", vec![0, 1]),
        ],
    );
    let chain1 = test_chain(
        "chain:1",
        vec![
            test_input_block("dev-in", vec![1]), // different channel, same device
            test_output_block("dev-out", vec![0, 1]),
        ],
    );
    let project = test_project(vec![chain0, chain1]);
    assert!(validate_project(&project).is_ok());
}

#[test]
fn validate_project_disabled_chain_skips_conflict_check() {
    let chain0 = test_chain(
        "chain:0",
        vec![
            test_input_block("dev-in", vec![0]),
            test_output_block("dev-out", vec![0, 1]),
        ],
    );
    let mut chain1 = test_chain(
        "chain:1",
        vec![
            test_input_block("dev-in", vec![0]),
            test_output_block("dev-out", vec![0, 1]),
        ],
    );
    chain1.enabled = false; // disabled chain should be ignored for conflict
    let project = test_project(vec![chain0, chain1]);
    assert!(validate_project(&project).is_ok());
}

// -----------------------------------------------------------------------
// validate_project — input entry with empty name uses model as label
// -----------------------------------------------------------------------

#[test]
fn validate_project_input_entry_empty_name_uses_model_in_error() {
    let input = AudioBlock {
        id: BlockId("block:input".to_string()),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".to_string(),
            entries: vec![InputEntry {
                device_id: DeviceId("".to_string()),
                mode: ChainInputMode::Mono,
                channels: vec![0],
            }],
        }),
    };
    let chain = test_chain(
        "chain:0",
        vec![input, test_output_block("dev-out", vec![0, 1])],
    );
    let project = test_project(vec![chain]);
    let err = validate_project(&project).unwrap_err();
    assert!(err.to_string().contains("standard"));
}

#[test]
fn validate_project_output_entry_empty_name_uses_model_in_error() {
    let output = AudioBlock {
        id: BlockId("block:output".to_string()),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".to_string(),
            entries: vec![OutputEntry {
                device_id: DeviceId("".to_string()),
                mode: ChainOutputMode::Stereo,
                channels: vec![0, 1],
            }],
        }),
    };
    let chain = test_chain("chain:0", vec![test_input_block("dev-in", vec![0]), output]);
    let project = test_project(vec![chain]);
    let err = validate_project(&project).unwrap_err();
    assert!(err.to_string().contains("standard"));
}

// -----------------------------------------------------------------------
// validate_project — input with no entries
// -----------------------------------------------------------------------

#[test]
fn validate_project_input_no_entries_fails() {
    let input = AudioBlock {
        id: BlockId("block:input".to_string()),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".to_string(),
            entries: vec![],
        }),
    };
    let chain = test_chain(
        "chain:0",
        vec![input, test_output_block("dev-out", vec![0, 1])],
    );
    let project = test_project(vec![chain]);
    let err = validate_project(&project).unwrap_err();
    assert!(err.to_string().contains("has no entries"));
}

#[test]
fn validate_project_output_no_entries_fails() {
    let output = AudioBlock {
        id: BlockId("block:output".to_string()),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".to_string(),
            entries: vec![],
        }),
    };
    let chain = test_chain("chain:0", vec![test_input_block("dev-in", vec![0]), output]);
    let project = test_project(vec![chain]);
    let err = validate_project(&project).unwrap_err();
    assert!(err.to_string().contains("has no entries"));
}

// -----------------------------------------------------------------------
// validate_project — unsupported channel count (e.g. 3 channels)
// -----------------------------------------------------------------------

#[test]
fn validate_project_three_input_channels_fails() {
    let chain = test_chain(
        "chain:0",
        vec![
            test_input_block("dev-in", vec![0, 1, 2]),
            test_output_block("dev-out", vec![0, 1]),
        ],
    );
    let project = test_project(vec![chain]);
    let err = validate_project(&project).unwrap_err();
    assert!(err.to_string().contains("3 channels"));
}

#[test]
fn validate_project_three_output_channels_fails() {
    let chain = test_chain(
        "chain:0",
        vec![
            test_input_block("dev-in", vec![0]),
            test_output_block("dev-out", vec![0, 1, 2]),
        ],
    );
    let project = test_project(vec![chain]);
    let err = validate_project(&project).unwrap_err();
    assert!(err.to_string().contains("3 channels"));
}

// -----------------------------------------------------------------------
// validate_project — disabled chain skips block validation
// -----------------------------------------------------------------------

#[test]
fn validate_project_disabled_chain_skips_block_validation() {
    // A chain with a Core block referencing a non-existent model should be
    // fine if the chain is disabled (validate_chain_blocks returns early).
    let bad_core = AudioBlock {
        id: BlockId("block:core".to_string()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "delay".to_string(),
            model: "nonexistent_model".to_string(),
            params: ParameterSet::default(),
        }),
    };
    let mut chain = test_chain(
        "chain:0",
        vec![
            test_input_block("dev-in", vec![0]),
            bad_core,
            test_output_block("dev-out", vec![0, 1]),
        ],
    );
    chain.enabled = false;
    let project = test_project(vec![chain]);
    assert!(validate_project(&project).is_ok());
}

// -----------------------------------------------------------------------
// validate_project — disabled block skips layout propagation
// -----------------------------------------------------------------------

#[test]
fn validate_project_disabled_block_skipped_in_layout_propagation() {
    let disabled_core = AudioBlock {
        id: BlockId("block:core".to_string()),
        enabled: false,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "delay".to_string(),
            model: "nonexistent_model".to_string(),
            params: ParameterSet::default(),
        }),
    };
    let chain = test_chain(
        "chain:0",
        vec![
            test_input_block("dev-in", vec![0]),
            disabled_core,
            test_output_block("dev-out", vec![0, 1]),
        ],
    );
    let project = test_project(vec![chain]);
    assert!(validate_project(&project).is_ok());
}

// -----------------------------------------------------------------------
// validate_project — layout propagation with real block types
// -----------------------------------------------------------------------

#[test]
fn validate_project_with_delay_block_succeeds() {
    let delay_model = block_delay::supported_models()
        .first()
        .expect("block-delay must expose at least one model");
    let schema = project::block::schema_for_block_model("delay", delay_model)
        .expect("delay schema should exist");
    let params = ParameterSet::default()
        .normalized_against(&schema)
        .expect("delay defaults should normalize");

    let delay_block = AudioBlock {
        id: BlockId("block:delay".to_string()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "delay".to_string(),
            model: delay_model.to_string(),
            params,
        }),
    };
    let chain = test_chain(
        "chain:0",
        vec![
            test_input_block("dev-in", vec![0]),
            delay_block,
            test_output_block("dev-out", vec![0, 1]),
        ],
    );
    let project = test_project(vec![chain]);
    assert!(validate_project(&project).is_ok());
}

#[test]
fn validate_project_with_reverb_block_succeeds() {
    let reverb_model = block_reverb::supported_models()
        .first()
        .expect("block-reverb must expose at least one model");
    let schema = project::block::schema_for_block_model("reverb", reverb_model)
        .expect("reverb schema should exist");
    let params = ParameterSet::default()
        .normalized_against(&schema)
        .expect("reverb defaults should normalize");

    let reverb_block = AudioBlock {
        id: BlockId("block:reverb".to_string()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "reverb".to_string(),
            model: reverb_model.to_string(),
            params,
        }),
    };
    let chain = test_chain(
        "chain:0",
        vec![
            test_input_block("dev-in", vec![0]),
            reverb_block,
            test_output_block("dev-out", vec![0, 1]),
        ],
    );
    let project = test_project(vec![chain]);
    assert!(validate_project(&project).is_ok());
}

// -----------------------------------------------------------------------
// validate_project — Insert blocks are skipped in layout propagation
// -----------------------------------------------------------------------

#[test]
fn validate_project_with_insert_block_succeeds() {
    let insert = AudioBlock {
        id: BlockId("block:insert".to_string()),
        enabled: true,
        kind: AudioBlockKind::Insert(InsertBlock {
            model: "external_loop".to_string(),
            send: InsertEndpoint {
                device_id: DeviceId("send-dev".to_string()),
                mode: ChainInputMode::Stereo,
                channels: vec![0, 1],
            },
            return_: InsertEndpoint {
                device_id: DeviceId("return-dev".to_string()),
                mode: ChainInputMode::Stereo,
                channels: vec![0, 1],
            },
        }),
    };
    let chain = test_chain(
        "chain:0",
        vec![
            test_input_block("dev-in", vec![0]),
            insert,
            test_output_block("dev-out", vec![0, 1]),
        ],
    );
    let project = test_project(vec![chain]);
    assert!(validate_project(&project).is_ok());
}
