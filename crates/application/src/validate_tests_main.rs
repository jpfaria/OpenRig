//! `validate_project` integration tests. Helpers via sibling `helpers` mod.
//!
//! #716 (model A): tests that asserted per-block `entries` device/channel
//! validation or the cross-chain channel-conflict check were removed — device
//! endpoints no longer live on the chain; they are resolved from the I/O
//! binding registry at the activation layer, which `validate_project` does not
//! see. Only the registry-independent structural / device-settings / layout
//! checks remain.

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

// #716: a binding-bound chain resolves its I/O from the registry at runtime —
// its Input/Output blocks carry `io`/`endpoint` only. The structural validation
// must NOT reject it (that was the no-sound bug: "input 'standard' has no
// entries").
#[test]
fn validate_project_binding_bound_chain_succeeds() {
    use domain::ids::{BlockId, ChainId};
    use project::block::{AudioBlock, AudioBlockKind, InputBlock, OutputBlock};
    use project::chain::Chain;
    use project::project::Project;

    let bound_input = AudioBlock {
        id: BlockId("in".into()),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".into(),
            io: "scarlet".into(),
            endpoint: "in1".into(),
        }),
    };
    let bound_output = AudioBlock {
        id: BlockId("out".into()),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".into(),
            io: "scarlet".into(),
            endpoint: "out1".into(),
        }),
    };
    let chain = Chain {
        id: ChainId("rig:input-1".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["scarlet".into()],
        blocks: vec![bound_input, bound_output],
        di_output: None,
    };
    let project = Project {
        name: None,
        device_settings: vec![],
        chains: vec![chain],
        midi: None,
    };
    validate_project(&project)
        .expect("binding-bound chain must validate — its I/O comes from the registry");
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
        midi: None,
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
    // The processing bus is stereo (invariant #5), so pick a delay model that
    // accepts a stereo input bus.
    let delay_model = block_delay::supported_models()
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
        .expect("block-delay must expose at least one stereo-capable model");
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
    // The processing bus is stereo (invariant #5), so pick a reverb model that
    // accepts a stereo input bus.
    let reverb_model = block_reverb::supported_models()
        .iter()
        .copied()
        .find(|model| {
            project::block::schema_for_block_model("reverb", model)
                .map(|s| {
                    s.audio_mode
                        .output_layout(block_core::AudioChannelLayout::Stereo)
                        .is_some()
                })
                .unwrap_or(false)
        })
        .expect("block-reverb must expose at least one stereo-capable model");
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
            io: "fx".to_string(),
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
