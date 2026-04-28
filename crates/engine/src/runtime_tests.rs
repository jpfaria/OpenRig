//! Unit tests for the engine runtime — input/output processing, chain graph
//! construction, and per-block runtime node assembly.
//!
//! Lifted out of `runtime.rs` so the production-side file (~2.6k LOC) stays
//! readable alongside its test surface (also ~2.6k LOC). All tests live
//! under `mod tests` of the runtime crate root via `#[path]`, so every
//! `super::xxx` reference (private types, helper fns) keeps resolving
//! unchanged.

use super::{
    apply_block_processor, process_audio_block,
    build_chain_runtime_state, build_runtime_graph, process_input_f32, process_output_f32,
    update_chain_runtime_state, split_chain_into_segments, effective_inputs, effective_outputs,
    AudioFrame, AudioProcessor, BlockError, BlockRuntimeNode, FadeState, ProcessorScratch, RuntimeProcessor,
    ElasticBuffer, DEFAULT_ELASTIC_TARGET, ERROR_QUEUE_CAPACITY, FADE_IN_FRAMES,
};
use crossbeam_queue::ArrayQueue;
use block_core::AudioChannelLayout;
use block_preamp::supported_models as supported_preamp_models;
use block_cab::{cab_backend_kind, supported_models as supported_cab_models, CabBackendKind};
use block_delay::supported_models as supported_delay_models;
use block_dyn::compressor_supported_models;
use block_reverb::supported_models as supported_reverb_models;
use domain::ids::{BlockId, DeviceId, ChainId};
use domain::value_objects::ParameterValue;
use project::block::{
    AudioBlock, AudioBlockKind, CoreBlock, InputBlock, InputEntry, InsertBlock, InsertEndpoint, OutputBlock, OutputEntry, SelectBlock, schema_for_block_model,
};
use project::param::ParameterSet;
use project::project::Project;
use project::chain::{Chain, ChainInputMode, ChainOutputMode};
use std::collections::HashMap;
use std::sync::Arc;

#[test]
fn runtime_graph_builds_for_chain_with_cab_block() {
    let (model, params) = any_ir_cab_defaults();

    let project = Project {
        name: None,
        device_settings: Vec::new(),
        chains: vec![Chain {
            id: ChainId("chain:0".into()),
            description: Some("Cab test".into()),
            instrument: "electric_guitar".to_string(),
            enabled: true,
            blocks: vec![AudioBlock {
                id: BlockId("chain:0:block:0".into()),
                enabled: true,
                kind: AudioBlockKind::Core(CoreBlock {
                    effect_type: "cab".to_string(),
                    model,
                    params,
                }),
            }],
        }],
    };

    let runtime = build_runtime_graph(
        &project,
        &HashMap::from([(ChainId("chain:0".into()), 48_000.0)]),
        &HashMap::new(),
    )
    .expect("runtime graph should build");
    assert_eq!(runtime.chains.len(), 1);
}

#[test]
#[ignore] // requires asset_paths initialization
fn runtime_graph_rejects_chain_when_runtime_sample_rate_does_not_match_ir() {
    let (model, params) = any_ir_cab_defaults();

    let project = Project {
        name: None,
        device_settings: Vec::new(),
        chains: vec![Chain {
            id: ChainId("chain:0".into()),
            description: Some("Cab test".into()),
            instrument: "electric_guitar".to_string(),
            enabled: true,
            blocks: vec![AudioBlock {
                id: BlockId("chain:0:block:0".into()),
                enabled: true,
                kind: AudioBlockKind::Core(CoreBlock {
                    effect_type: "cab".to_string(),
                    model,
                    params,
                }),
            }],
        }],
    };

    let error = match build_runtime_graph(
        &project,
        &HashMap::from([(ChainId("chain:0".into()), 44_100.0)]),
        &HashMap::new(),
    ) {
        Ok(_) => panic!("runtime graph should reject mismatched IR sample rate"),
        Err(error) => error,
    };

    assert!(error.to_string().contains("sample_rate"));
}

#[test]
#[ignore] // requires asset_paths initialization
fn update_chain_runtime_state_preserves_unchanged_block_instances() {
    let mut chain = tuner_track(
        "chain:0",
        vec![
            tuner_block("chain:0:block:a", 440.0),
            tuner_block("chain:0:block:b", 445.0),
        ],
    );

    let runtime =
        Arc::new(build_chain_runtime_state(&chain, 48_000.0, &[DEFAULT_ELASTIC_TARGET]).expect("runtime state should build"));
    let original_serials = {
        let locked = runtime.processing.lock().expect("runtime poisoned");
        locked
            .input_states[0]
            .blocks
            .iter()
            .map(|block| block.instance_serial)
            .collect::<Vec<_>>()
    };

    if let AudioBlockKind::Core(core) = &mut chain.blocks[1].kind {
        core.params
            .insert("reference_hz", ParameterValue::Float(432.0));
    }

    update_chain_runtime_state(&runtime, &chain, 48_000.0, false, &[DEFAULT_ELASTIC_TARGET])
        .expect("runtime update should succeed");

    let updated_serials = {
        let locked = runtime.processing.lock().expect("runtime poisoned");
        locked
            .input_states[0]
            .blocks
            .iter()
            .map(|block| block.instance_serial)
            .collect::<Vec<_>>()
    };

    assert_eq!(updated_serials[0], original_serials[0]);
    assert_ne!(updated_serials[1], original_serials[1]);
}

#[test]
#[ignore] // requires asset_paths initialization
fn update_chain_runtime_state_preserves_block_identity_when_reordered() {
    let mut chain = tuner_track(
        "chain:0",
        vec![
            tuner_block("chain:0:block:a", 440.0),
            tuner_block("chain:0:block:b", 445.0),
        ],
    );

    let runtime =
        Arc::new(build_chain_runtime_state(&chain, 48_000.0, &[DEFAULT_ELASTIC_TARGET]).expect("runtime state should build"));
    let original_by_block_id = {
        let locked = runtime.processing.lock().expect("runtime poisoned");
        locked
            .input_states[0]
            .blocks
            .iter()
            .map(|block| (block.block_id.clone(), block.instance_serial))
            .collect::<HashMap<_, _>>()
    };

    chain.blocks.swap(0, 1);

    update_chain_runtime_state(&runtime, &chain, 48_000.0, false, &[DEFAULT_ELASTIC_TARGET])
        .expect("runtime update should succeed");

    let reordered = runtime.processing.lock().expect("runtime poisoned");
    assert_eq!(reordered.input_states[0].blocks.len(), 2);
    for block in &reordered.input_states[0].blocks {
        assert_eq!(
            Some(&block.instance_serial),
            original_by_block_id.get(&block.block_id)
        );
    }
}

#[test]
fn process_input_limits_buffered_output_frames() {
    let chain = tuner_track("chain:0", Vec::new());
    let runtime =
        Arc::new(build_chain_runtime_state(&chain, 48_000.0, &[DEFAULT_ELASTIC_TARGET]).expect("runtime state should build"));
    let total_frames = DEFAULT_ELASTIC_TARGET * 2 + 64;
    let input = vec![0.25f32; total_frames];

    process_input_f32(&runtime, 0, &input, 1);

    let routes = runtime.output_routes.load();
    assert!(routes[0].buffer.len() <= DEFAULT_ELASTIC_TARGET * 2);
}

#[test]
#[ignore] // requires asset_paths initialization
fn process_output_drains_buffered_frames() {
    let chain = tuner_track("chain:0", Vec::new());
    let runtime =
        Arc::new(build_chain_runtime_state(&chain, 48_000.0, &[DEFAULT_ELASTIC_TARGET]).expect("runtime state should build"));

    process_input_f32(&runtime, 0, &[0.25, 0.5, 0.75, 1.0], 1);

    let mut out = vec![0.0f32; 4];
    process_output_f32(&runtime, 0, &mut out, 1);

    assert_eq!(out, vec![0.25, 0.5, 0.75, 1.0]);
    let routes = runtime.output_routes.load();
    assert_eq!(routes[0].buffer.len(), 0);
}

#[test]
#[ignore] // requires asset_paths initialization
fn dual_mono_chain_does_not_leak_left_into_right() {
    let chain = Chain {
        id: ChainId("chain:stereo".into()),
        description: Some("Stereo isolation".into()),
        instrument: "electric_guitar".to_string(),
        enabled: true,
        blocks: vec![
            AudioBlock {
                id: BlockId("chain:stereo:input:0".into()),
                enabled: true,
                kind: AudioBlockKind::Input(InputBlock {
                    model: "standard".to_string(),
                    entries: vec![InputEntry {
                        device_id: DeviceId("input-device".into()),
                        mode: ChainInputMode::Mono,
                        channels: vec![0, 1],
                    }],
                }),
            },
            compressor_block("chain:stereo:block:0"),
            preamp_block("chain:stereo:block:1"),
            native_cab_block("chain:stereo:block:2"),
            reverb_block("chain:stereo:block:3"),
            AudioBlock {
                id: BlockId("chain:stereo:output:0".into()),
                enabled: true,
                kind: AudioBlockKind::Output(OutputBlock {
                    model: "standard".to_string(),
                    entries: vec![OutputEntry {
                        device_id: DeviceId("output-device".into()),
                        mode: ChainOutputMode::Stereo,
                        channels: vec![0, 1],
                    }],
                }),
            },
        ],
    };
    let runtime =
        Arc::new(build_chain_runtime_state(&chain, 48_000.0, &[DEFAULT_ELASTIC_TARGET]).expect("runtime state should build"));

    let mut input = vec![0.0f32; 256 * 2];
    for frame in input.chunks_mut(2) {
        frame[0] = 0.25;
        frame[1] = 0.0;
    }
    process_input_f32(&runtime, 0, &input, 2);

    let mut output = vec![0.0f32; input.len()];
    process_output_f32(&runtime, 0, &mut output, 2);

    let right_peak = output
        .chunks_exact(2)
        .map(|frame| frame[1].abs())
        .fold(0.0f32, f32::max);
    assert!(
        right_peak <= 1.0e-6,
        "dual-mono chain leaked signal into right channel: peak={right_peak}"
    );
}

#[test]
#[ignore] // requires asset_paths initialization
fn asset_backed_dual_mono_chain_does_not_leak_left_into_right() {
    let chain = Chain {
        id: ChainId("chain:asset-backed".into()),
        description: Some("Stereo isolation asset-backed".into()),
        instrument: "electric_guitar".to_string(),
        enabled: true,
        blocks: vec![
            AudioBlock {
                id: BlockId("chain:asset-backed:input:0".into()),
                enabled: true,
                kind: AudioBlockKind::Input(InputBlock {
                    model: "standard".to_string(),
                    entries: vec![InputEntry {
                        device_id: DeviceId("input-device".into()),
                        mode: ChainInputMode::Mono,
                        channels: vec![0, 1],
                    }],
                }),
            },
            marshall_preamp_block("chain:asset-backed:block:0"),
            ir_cab_block("chain:asset-backed:block:1"),
            reverb_block("chain:asset-backed:block:2"),
            AudioBlock {
                id: BlockId("chain:asset-backed:output:0".into()),
                enabled: true,
                kind: AudioBlockKind::Output(OutputBlock {
                    model: "standard".to_string(),
                    entries: vec![OutputEntry {
                        device_id: DeviceId("output-device".into()),
                        mode: ChainOutputMode::Stereo,
                        channels: vec![0, 1],
                    }],
                }),
            },
        ],
    };
    let runtime =
        Arc::new(build_chain_runtime_state(&chain, 48_000.0, &[DEFAULT_ELASTIC_TARGET]).expect("runtime state should build"));

    let mut input = vec![0.0f32; 256 * 2];
    for frame in input.chunks_mut(2) {
        frame[0] = 0.25;
        frame[1] = 0.0;
    }
    process_input_f32(&runtime, 0, &input, 2);

    let mut output = vec![0.0f32; input.len()];
    process_output_f32(&runtime, 0, &mut output, 2);

    let right_peak = output
        .chunks_exact(2)
        .map(|frame| frame[1].abs())
        .fold(0.0f32, f32::max);
    assert!(
        right_peak <= 1.0e-6,
        "asset-backed dual-mono chain leaked signal into right channel: peak={right_peak}"
    );
}

#[test]
#[ignore] // requires asset_paths initialization
fn select_block_builds_for_generic_delay_options() {
    let chain = select_delay_chain("chain:select", "delay_a");

    let runtime =
        build_chain_runtime_state(&chain, 48_000.0, &[DEFAULT_ELASTIC_TARGET]).expect("select delay chain should build");

    let locked = runtime.processing.lock().expect("runtime poisoned");
    assert_eq!(locked.input_states[0].blocks.len(), 1);
}

#[test]
#[ignore] // requires asset_paths initialization
fn update_chain_runtime_state_preserves_select_instance_when_switching_active_option() {
    let mut chain = select_delay_chain("chain:select", "delay_a");
    let runtime =
        Arc::new(build_chain_runtime_state(&chain, 48_000.0, &[DEFAULT_ELASTIC_TARGET]).expect("runtime state should build"));
    let original_serial = {
        let locked = runtime.processing.lock().expect("runtime poisoned");
        locked.input_states[0].blocks[0].instance_serial
    };

    if let AudioBlockKind::Select(select) = &mut chain.blocks[0].kind {
        select.selected_block_id = BlockId("chain:select:block:0::delay_b".into());
    }

    update_chain_runtime_state(&runtime, &chain, 48_000.0, false, &[DEFAULT_ELASTIC_TARGET])
        .expect("runtime update should succeed when switching select option");

    let updated_serial = {
        let locked = runtime.processing.lock().expect("runtime poisoned");
        locked.input_states[0].blocks[0].instance_serial
    };

    assert_eq!(updated_serial, original_serial);
}

fn tuner_track(chain_id: &str, blocks: Vec<AudioBlock>) -> Chain {
    Chain {
        id: ChainId(chain_id.into()),
        description: Some("Tuner chain".into()),
        instrument: "electric_guitar".to_string(),
        enabled: true,
        blocks,
    }
}

/// Test helper — builds a generic processing block. Originally backed by
/// the (now removed) `chromatic_tuner` / `spectrum_analyzer` utility
/// blocks, but those were promoted to top-bar features (#319, #320).
/// We now back it with a delay block so the tests still have a real
/// `BlockProcessor` in their chains; `reference_hz` is preserved as a
/// dummy `time_ms` for the delay so unique values per block survive
/// the rename.
fn tuner_block(block_id: &str, reference_hz: f32) -> AudioBlock {
    let delay_model = supported_delay_models()
        .first()
        .expect("block-delay must expose at least one model")
        .to_string();
    let mut params = ParameterSet::default();
    params.insert("time_ms", ParameterValue::Float(reference_hz));
    AudioBlock {
        id: BlockId(block_id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "delay".to_string(),
            model: delay_model,
            params,
        }),
    }
}

fn any_ir_cab_defaults() -> (String, ParameterSet) {
    let model = supported_cab_models()
        .iter()
        .find(|model| {
            matches!(
                cab_backend_kind(model).expect("cab backend should resolve"),
                CabBackendKind::Ir
            )
        })
        .expect("block-cab must expose at least one IR-backed model")
        .to_string();
    let schema = block_cab::cab_model_schema(&model).expect("cab schema should exist");
    let params = ParameterSet::default()
        .normalized_against(&schema)
        .expect("cab defaults should normalize");
    (model, params)
}

fn normalized_defaults(effect_type: &str, model: &str) -> ParameterSet {
    let schema =
        schema_for_block_model(effect_type, model).expect("schema should exist for test model");
    ParameterSet::default()
        .normalized_against(&schema)
        .expect("defaults should normalize")
}

fn compressor_block(block_id: &str) -> AudioBlock {
    let model = compressor_supported_models()
        .first()
        .expect("block-dyn must expose at least one compressor")
        .to_string();
    AudioBlock {
        id: BlockId(block_id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "dynamics".to_string(),
            params: normalized_defaults("dynamics", &model),
            model,
        }),
    }
}

fn native_cab_block(block_id: &str) -> AudioBlock {
    let model = supported_cab_models()
        .iter()
        .find(|model| matches!(cab_backend_kind(model).expect("cab backend"), CabBackendKind::Native))
        .expect("block-cab must expose at least one native model")
        .to_string();
    AudioBlock {
        id: BlockId(block_id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "cab".to_string(),
            params: normalized_defaults("cab", &model),
            model,
        }),
    }
}

fn preamp_block(block_id: &str) -> AudioBlock {
    let model = supported_preamp_models()
        .iter()
        .find(|model| !model.contains("marshall_jcm_800"))
        .or_else(|| supported_preamp_models().first())
        .expect("block-preamp must expose at least one model")
        .to_string();
    AudioBlock {
        id: BlockId(block_id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "preamp".to_string(),
            params: normalized_defaults("preamp", &model),
            model,
        }),
    }
}

fn marshall_preamp_block(block_id: &str) -> AudioBlock {
    let model = "marshall_jcm_800_2203".to_string();
    AudioBlock {
        id: BlockId(block_id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "preamp".to_string(),
            params: normalized_defaults("preamp", &model),
            model,
        }),
    }
}

fn ir_cab_block(block_id: &str) -> AudioBlock {
    let model = supported_cab_models()
        .iter()
        .find(|model| matches!(cab_backend_kind(model).expect("cab backend"), CabBackendKind::Ir))
        .expect("block-cab must expose at least one IR model")
        .to_string();
    AudioBlock {
        id: BlockId(block_id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "cab".to_string(),
            params: normalized_defaults("cab", &model),
            model,
        }),
    }
}

fn reverb_block(block_id: &str) -> AudioBlock {
    let model = supported_reverb_models()
        .first()
        .expect("block-reverb must expose at least one model")
        .to_string();
    AudioBlock {
        id: BlockId(block_id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "reverb".to_string(),
            params: normalized_defaults("reverb", &model),
            model,
        }),
    }
}

// --- ElasticBuffer tests ---

#[test]
fn elastic_buffer_push_pop_basic() {
    let buf = ElasticBuffer::new(256, AudioChannelLayout::Mono);
    buf.push(AudioFrame::Mono(0.5));
    buf.push(AudioFrame::Mono(0.7));
    assert_eq!(buf.len(), 2);
    let f1 = buf.pop();
    assert!(matches!(f1, AudioFrame::Mono(v) if (v - 0.5).abs() < 1e-6));
    let f2 = buf.pop();
    assert!(matches!(f2, AudioFrame::Mono(v) if (v - 0.7).abs() < 1e-6));
}

#[test]
fn elastic_buffer_underrun_repeats_last_frame() {
    let buf = ElasticBuffer::new(256, AudioChannelLayout::Mono);
    buf.push(AudioFrame::Mono(0.42));
    let _ = buf.pop(); // drain the one frame
    // Now empty — should repeat last frame, NOT silence
    let repeated = buf.pop();
    assert!(matches!(repeated, AudioFrame::Mono(v) if (v - 0.42).abs() < 1e-6));
}

#[test]
fn elastic_buffer_underrun_before_any_push_returns_silence() {
    let buf = ElasticBuffer::new(256, AudioChannelLayout::Stereo);
    let frame = buf.pop();
    assert!(matches!(frame, AudioFrame::Stereo([l, r]) if l.abs() < 1e-6 && r.abs() < 1e-6));
}

#[test]
fn elastic_buffer_overrun_drops_newest() {
    // The lock-free SPSC ring drops the newest frame when full (rather
    // than advancing the tail from the producer side, which would break
    // the single-producer invariant). Ring capacity is the next power of
    // two at or above target_level * 2.
    let target: usize = 4;
    let capacity = (target * 2).next_power_of_two();
    let buf = ElasticBuffer::new(target, AudioChannelLayout::Mono);
    for i in 0..(capacity + 4) {
        buf.push(AudioFrame::Mono(i as f32));
    }
    assert_eq!(buf.len(), capacity);
    // Oldest frames are retained — first pop returns the very first push.
    assert!(matches!(buf.pop(), AudioFrame::Mono(v) if v == 0.0));
}

#[test]
fn elastic_buffer_stabilizes_around_target() {
    let target = 256;
    let buf = ElasticBuffer::new(target, AudioChannelLayout::Mono);
    // Simulate: push slightly faster than pop
    for _ in 0..10000 {
        buf.push(AudioFrame::Mono(1.0));
        buf.push(AudioFrame::Mono(1.0)); // 2 pushes
        let _ = buf.pop(); // 1 pop — simulates input faster than output
    }
    // Should not have grown unbounded
    assert!(buf.len() <= target * 2);
}

/// A chain with proper Input and Output blocks but no effect blocks.
/// Useful for testing process_input_f32 / process_output_f32.
fn io_passthrough_chain(id: &str) -> Chain {
    Chain {
        id: ChainId(id.into()),
        description: Some("Passthrough".into()),
        instrument: "electric_guitar".to_string(),
        enabled: true,
        blocks: vec![
            AudioBlock {
                id: BlockId(format!("{id}:input:0")),
                enabled: true,
                kind: AudioBlockKind::Input(InputBlock {
                    model: "standard".into(),
                    entries: vec![InputEntry {
                        device_id: DeviceId("dev".into()),
                        mode: ChainInputMode::Mono,
                        channels: vec![0],
                    }],
                }),
            },
            AudioBlock {
                id: BlockId(format!("{id}:output:0")),
                enabled: true,
                kind: AudioBlockKind::Output(OutputBlock {
                    model: "standard".into(),
                    entries: vec![OutputEntry {
                        device_id: DeviceId("dev".into()),
                        mode: ChainOutputMode::Mono,
                        channels: vec![0],
                    }],
                }),
            },
        ],
    }
}

fn select_delay_chain(id: &str, selected_option: &str) -> Chain {
    let models = supported_delay_models();
    let first_model = models
        .first()
        .expect("block-delay must expose at least one model");
    let second_model = models.get(1).unwrap_or(first_model);

    Chain {
        id: ChainId(id.into()),
        description: Some("Delay select".into()),
        instrument: "electric_guitar".to_string(),
        enabled: true,
        blocks: vec![AudioBlock {
            id: BlockId(format!("{id}:block:0")),
            enabled: true,
            kind: AudioBlockKind::Select(SelectBlock {
                selected_block_id: BlockId(format!("{id}:block:0::{selected_option}")),
                options: vec![
                    delay_block(format!("{id}:block:0::delay_a"), first_model, 120.0),
                    delay_block(format!("{id}:block:0::delay_b"), second_model, 240.0),
                ],
            }),
        }],
    }
}

fn delay_block(id: impl Into<String>, model: &str, time_ms: f32) -> AudioBlock {
    let mut params = normalized_defaults("delay", model);
    params.insert("time_ms", ParameterValue::Float(time_ms));
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

#[test]
fn segments_split_by_output_position() {
    // Chain: [Input, TS9(1), Amp(2), Volume(3), Output_MIXER(4), Delay(5), Reverb(6), Output_Scarlett(7)]
    let chain = Chain {
        id: ChainId("test".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        blocks: vec![
            AudioBlock { id: BlockId("input:0".into()), enabled: true,
                kind: AudioBlockKind::Input(InputBlock { model: "standard".into(),
                    entries: vec![InputEntry { device_id: DeviceId("scarlett".into()), mode: ChainInputMode::Mono, channels: vec![0] }] }) },
            AudioBlock { id: BlockId("ts9".into()), enabled: true,
                kind: AudioBlockKind::Core(CoreBlock { effect_type: "gain".into(), model: "volume".into(), params: ParameterSet::default() }) },
            AudioBlock { id: BlockId("amp".into()), enabled: true,
                kind: AudioBlockKind::Core(CoreBlock { effect_type: "gain".into(), model: "volume".into(), params: ParameterSet::default() }) },
            AudioBlock { id: BlockId("volume".into()), enabled: true,
                kind: AudioBlockKind::Core(CoreBlock { effect_type: "gain".into(), model: "volume".into(), params: ParameterSet::default() }) },
            AudioBlock { id: BlockId("out_mixer".into()), enabled: true,
                kind: AudioBlockKind::Output(OutputBlock { model: "standard".into(),
                    entries: vec![OutputEntry { device_id: DeviceId("mixer".into()), mode: ChainOutputMode::Stereo, channels: vec![0, 1] }] }) },
            AudioBlock { id: BlockId("delay".into()), enabled: true,
                kind: AudioBlockKind::Core(CoreBlock { effect_type: "delay".into(), model: "digital_clean".into(), params: ParameterSet::default() }) },
            AudioBlock { id: BlockId("reverb".into()), enabled: true,
                kind: AudioBlockKind::Core(CoreBlock { effect_type: "reverb".into(), model: "plate_foundation".into(), params: ParameterSet::default() }) },
            AudioBlock { id: BlockId("out_scarlett".into()), enabled: true,
                kind: AudioBlockKind::Output(OutputBlock { model: "standard".into(),
                    entries: vec![OutputEntry { device_id: DeviceId("scarlett".into()), mode: ChainOutputMode::Stereo, channels: vec![0, 1] }] }) },
        ],
    };

    let (eff_inputs, eff_cpal_indices) = effective_inputs(&chain);
    let eff_outputs = effective_outputs(&chain);
    let segments = split_chain_into_segments(&chain, &eff_inputs, &eff_cpal_indices, &eff_outputs);

    // Should have 2 segments (1 input × 2 outputs)
    assert_eq!(segments.len(), 2, "expected 2 segments, got {}", segments.len());

    // Segment 0: blocks before Output_MIXER (pos 4) → [TS9(1), Amp(2), Volume(3)]
    assert_eq!(segments[0].block_indices, vec![1, 2, 3],
        "segment 0 should have blocks [1,2,3], got {:?}", segments[0].block_indices);
    assert_eq!(segments[0].output_route_indices, vec![0],
        "segment 0 should push to output 0 only");

    // Segment 1: blocks before Output_Scarlett (pos 7) → [TS9(1), Amp(2), Volume(3), Delay(5), Reverb(6)]
    assert_eq!(segments[1].block_indices, vec![1, 2, 3, 5, 6],
        "segment 1 should have blocks [1,2,3,5,6], got {:?}", segments[1].block_indices);
    assert_eq!(segments[1].output_route_indices, vec![1],
        "segment 1 should push to output 1 only");
}

// ── Panic recovery tests ──────────────────────────────────────────────────

struct PanickingProcessor;
impl block_core::MonoProcessor for PanickingProcessor {
    fn process_sample(&mut self, _: f32) -> f32 {
        panic!("simulated plugin crash");
    }
}

struct CountingProcessor {
    call_count: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}
impl block_core::MonoProcessor for CountingProcessor {
    fn process_sample(&mut self, input: f32) -> f32 {
        self.call_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        input
    }
}

fn panicking_block_node() -> BlockRuntimeNode {
    BlockRuntimeNode {
        instance_serial: 0,
        block_id: domain::ids::BlockId("test:panicking".into()),
        block_snapshot: project::block::AudioBlock {
            id: domain::ids::BlockId("test:panicking".into()),
            enabled: true,
            kind: project::block::AudioBlockKind::Core(project::block::CoreBlock {
                effect_type: "gain".into(),
                model: "volume".into(),
                params: project::param::ParameterSet::default(),
            }),
        },
        input_layout: block_core::AudioChannelLayout::Mono,
        output_layout: block_core::AudioChannelLayout::Mono,
        scratch: ProcessorScratch::Mono(Vec::new()),
        processor: RuntimeProcessor::Audio(AudioProcessor::Mono(Box::new(PanickingProcessor))),
        stream_handle: None,
        fade_state: FadeState::Active,
        faulted: false,
    }
}

fn counting_block_node(counter: std::sync::Arc<std::sync::atomic::AtomicUsize>) -> BlockRuntimeNode {
    BlockRuntimeNode {
        instance_serial: 0,
        block_id: domain::ids::BlockId("test:counting".into()),
        block_snapshot: project::block::AudioBlock {
            id: domain::ids::BlockId("test:counting".into()),
            enabled: true,
            kind: project::block::AudioBlockKind::Core(project::block::CoreBlock {
                effect_type: "gain".into(),
                model: "volume".into(),
                params: project::param::ParameterSet::default(),
            }),
        },
        input_layout: block_core::AudioChannelLayout::Mono,
        output_layout: block_core::AudioChannelLayout::Mono,
        scratch: ProcessorScratch::Mono(Vec::new()),
        processor: RuntimeProcessor::Audio(AudioProcessor::Mono(Box::new(CountingProcessor { call_count: counter }))),
        stream_handle: None,
        fade_state: FadeState::Active,
        faulted: false,
    }
}

#[test]
fn panicking_processor_does_not_crash_the_caller() {
    let mut block = panicking_block_node();
    let error_queue = ArrayQueue::<BlockError>::new(ERROR_QUEUE_CAPACITY);
    let mut frames = vec![AudioFrame::Stereo([1.0, 1.0]); 16];

    // Must not panic
    apply_block_processor(&mut block, &mut frames, &error_queue);
}

#[test]
fn panicking_processor_marks_block_as_faulted() {
    let mut block = panicking_block_node();
    let error_queue = ArrayQueue::<BlockError>::new(ERROR_QUEUE_CAPACITY);
    let mut frames = vec![AudioFrame::Stereo([1.0, 1.0]); 16];

    apply_block_processor(&mut block, &mut frames, &error_queue);

    assert!(block.faulted, "block should be marked faulted after a panic");
}

#[test]
fn panicking_processor_zeroes_output_frames() {
    let mut block = panicking_block_node();
    let error_queue = ArrayQueue::<BlockError>::new(ERROR_QUEUE_CAPACITY);
    let mut frames = vec![AudioFrame::Stereo([1.0, 1.0]); 16];

    apply_block_processor(&mut block, &mut frames, &error_queue);

    for frame in &frames {
        match frame {
            AudioFrame::Stereo([l, r]) => {
                assert_eq!(*l, 0.0, "left channel should be silent after panic");
                assert_eq!(*r, 0.0, "right channel should be silent after panic");
            }
            AudioFrame::Mono(s) => assert_eq!(*s, 0.0, "mono channel should be silent after panic"),
        }
    }
}

#[test]
fn panicking_processor_posts_error_to_queue() {
    let mut block = panicking_block_node();
    let error_queue = ArrayQueue::<BlockError>::new(ERROR_QUEUE_CAPACITY);
    let mut frames = vec![AudioFrame::Stereo([1.0, 1.0]); 16];

    apply_block_processor(&mut block, &mut frames, &error_queue);

    assert_eq!(error_queue.len(), 1, "exactly one error should be posted");
    let err = error_queue.pop().expect("error should be available");
    assert_eq!(err.block_id.0, "test:panicking");
    assert!(err.message.contains("simulated plugin crash"), "error message should contain panic message");
}

#[test]
fn faulted_block_is_permanently_bypassed() {
    let counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut block = counting_block_node(counter.clone());
    block.faulted = true; // pre-fault the block

    let error_queue = ArrayQueue::<BlockError>::new(ERROR_QUEUE_CAPACITY);
    let mut frames = vec![AudioFrame::Stereo([1.0, 1.0]); 16];

    apply_block_processor(&mut block, &mut frames, &error_queue);

    assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 0,
        "process_sample should never be called on a faulted block");
}

#[test]
fn second_call_after_panic_does_not_process_or_post_error() {
    let mut block = panicking_block_node();
    let error_queue = ArrayQueue::<BlockError>::new(ERROR_QUEUE_CAPACITY);
    let mut frames = vec![AudioFrame::Stereo([1.0, 1.0]); 16];

    // First call: panics, marks faulted, posts error
    apply_block_processor(&mut block, &mut frames, &error_queue);
    assert_eq!(error_queue.len(), 1);

    // Second call: faulted — must not post another error
    apply_block_processor(&mut block, &mut frames, &error_queue);
    assert_eq!(error_queue.len(), 1,
        "no additional error should be posted for an already-faulted block");
}

#[test]
fn process_audio_block_bypassed_state_skips_processing() {
    let counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut block = counting_block_node(counter.clone());
    block.fade_state = FadeState::Bypassed;

    let error_queue = ArrayQueue::<BlockError>::new(ERROR_QUEUE_CAPACITY);
    let mut frames = vec![AudioFrame::Stereo([1.0, 1.0]); 16];

    process_audio_block(&mut block, &mut frames, &error_queue);

    assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 0,
        "bypassed block should not call process_sample");
}

#[test]
fn process_audio_block_fading_in_applies_processing() {
    let counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut block = counting_block_node(counter.clone());
    block.fade_state = FadeState::FadingIn { frames_remaining: FADE_IN_FRAMES };

    let error_queue = ArrayQueue::<BlockError>::new(ERROR_QUEUE_CAPACITY);
    let mut frames = vec![AudioFrame::Stereo([1.0, 1.0]); 16];

    process_audio_block(&mut block, &mut frames, &error_queue);

    assert!(counter.load(std::sync::atomic::Ordering::SeqCst) > 0,
        "fading-in block should call process_sample");
}

// ── AudioFrame tests ─────────────────────────────────────────────────────

#[test]
fn audio_frame_mono_mix_mono_returns_sample() {
    let frame = AudioFrame::Mono(0.75);
    assert!((frame.mono_mix() - 0.75).abs() < 1e-6);
}

#[test]
fn audio_frame_mono_mix_stereo_returns_average() {
    let frame = AudioFrame::Stereo([0.4, 0.8]);
    assert!((frame.mono_mix() - 0.6).abs() < 1e-6);
}

// ── ElasticBuffer edge cases ─────────────────────────────────────────────

#[test]
fn elastic_buffer_target_one_limits_to_two() {
    let buf = ElasticBuffer::new(1, AudioChannelLayout::Mono);
    buf.push(AudioFrame::Mono(1.0));
    buf.push(AudioFrame::Mono(2.0));
    buf.push(AudioFrame::Mono(3.0)); // should discard oldest
    assert!(buf.len() <= 2, "buffer with target=1 should hold at most 2 frames");
}

#[test]
fn elastic_buffer_stereo_push_pop_preserves_channels() {
    let buf = ElasticBuffer::new(256, AudioChannelLayout::Stereo);
    buf.push(AudioFrame::Stereo([0.3, 0.7]));
    let frame = buf.pop();
    match frame {
        AudioFrame::Stereo([l, r]) => {
            assert!((l - 0.3).abs() < 1e-6);
            assert!((r - 0.7).abs() < 1e-6);
        }
        _ => panic!("expected stereo frame"),
    }
}

#[test]
fn elastic_buffer_multiple_pops_on_empty_repeat_last() {
    let buf = ElasticBuffer::new(256, AudioChannelLayout::Mono);
    buf.push(AudioFrame::Mono(0.99));
    let _ = buf.pop(); // drain
    // Multiple pops should all return last frame
    for _ in 0..5 {
        let f = buf.pop();
        assert!(matches!(f, AudioFrame::Mono(v) if (v - 0.99).abs() < 1e-6));
    }
}

// ── FadeState transition tests ───────────────────────────────────────────

#[test]
fn fade_in_completes_to_active_after_enough_frames() {
    let counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut block = counting_block_node(counter.clone());
    block.fade_state = FadeState::FadingIn { frames_remaining: 16 };

    let error_queue = ArrayQueue::<BlockError>::new(ERROR_QUEUE_CAPACITY);
    let mut frames = vec![AudioFrame::Stereo([1.0, 1.0]); 16];

    process_audio_block(&mut block, &mut frames, &error_queue);

    assert_eq!(block.fade_state, FadeState::Active,
        "fade-in should complete to Active when frames_remaining reaches 0");
}

#[test]
fn fade_in_partial_keeps_fading_in() {
    let counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut block = counting_block_node(counter.clone());
    block.fade_state = FadeState::FadingIn { frames_remaining: 64 };

    let error_queue = ArrayQueue::<BlockError>::new(ERROR_QUEUE_CAPACITY);
    let mut frames = vec![AudioFrame::Stereo([1.0, 1.0]); 16];

    process_audio_block(&mut block, &mut frames, &error_queue);

    match block.fade_state {
        FadeState::FadingIn { frames_remaining } => {
            assert_eq!(frames_remaining, 48, "should have consumed 16 frames of fade");
        }
        other => panic!("expected FadingIn, got {:?}", other),
    }
}

#[test]
fn fade_out_completes_to_bypassed_after_enough_frames() {
    let counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut block = counting_block_node(counter.clone());
    block.fade_state = FadeState::FadingOut { frames_remaining: 16 };

    let error_queue = ArrayQueue::<BlockError>::new(ERROR_QUEUE_CAPACITY);
    let mut frames = vec![AudioFrame::Stereo([1.0, 1.0]); 16];

    process_audio_block(&mut block, &mut frames, &error_queue);

    assert_eq!(block.fade_state, FadeState::Bypassed,
        "fade-out should complete to Bypassed when frames_remaining reaches 0");
}

#[test]
fn fade_out_partial_keeps_fading_out() {
    let counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut block = counting_block_node(counter.clone());
    block.fade_state = FadeState::FadingOut { frames_remaining: 64 };

    let error_queue = ArrayQueue::<BlockError>::new(ERROR_QUEUE_CAPACITY);
    let mut frames = vec![AudioFrame::Stereo([1.0, 1.0]); 16];

    process_audio_block(&mut block, &mut frames, &error_queue);

    match block.fade_state {
        FadeState::FadingOut { frames_remaining } => {
            assert_eq!(frames_remaining, 48, "should have consumed 16 frames of fade");
        }
        other => panic!("expected FadingOut, got {:?}", other),
    }
}

#[test]
fn fade_out_applies_processing_during_transition() {
    let counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut block = counting_block_node(counter.clone());
    block.fade_state = FadeState::FadingOut { frames_remaining: FADE_IN_FRAMES };

    let error_queue = ArrayQueue::<BlockError>::new(ERROR_QUEUE_CAPACITY);
    let mut frames = vec![AudioFrame::Stereo([1.0, 1.0]); 16];

    process_audio_block(&mut block, &mut frames, &error_queue);

    assert!(counter.load(std::sync::atomic::Ordering::SeqCst) > 0,
        "fading-out block should still call process_sample during transition");
}

// ── blend_frame tests ────────────────────────────────────────────────────

#[test]
fn blend_frame_mono_interpolates_correctly() {
    use super::blend_frame;
    let mut wet = AudioFrame::Mono(1.0);
    let dry = AudioFrame::Mono(0.0);
    blend_frame(&mut wet, dry, 0.5, 0.5);
    assert!((wet.mono_mix() - 0.5).abs() < 1e-6);
}

#[test]
fn blend_frame_stereo_interpolates_correctly() {
    use super::blend_frame;
    let mut wet = AudioFrame::Stereo([1.0, 0.0]);
    let dry = AudioFrame::Stereo([0.0, 1.0]);
    blend_frame(&mut wet, dry, 0.5, 0.5);
    match wet {
        AudioFrame::Stereo([l, r]) => {
            assert!((l - 0.5).abs() < 1e-6);
            assert!((r - 0.5).abs() < 1e-6);
        }
        _ => panic!("expected stereo"),
    }
}

#[test]
fn blend_frame_layout_mismatch_passes_dry_through() {
    use super::blend_frame;
    let mut wet = AudioFrame::Mono(1.0);
    let dry = AudioFrame::Stereo([0.3, 0.7]);
    blend_frame(&mut wet, dry, 0.5, 0.5);
    // On layout mismatch, frame should be set to dry
    match wet {
        AudioFrame::Stereo([l, r]) => {
            assert!((l - 0.3).abs() < 1e-6);
            assert!((r - 0.7).abs() < 1e-6);
        }
        _ => panic!("expected stereo from dry passthrough"),
    }
}

// ── mix_frames tests ─────────────────────────────────────────────────────

#[test]
fn mix_frames_mono_mono_sums() {
    use super::mix_frames;
    let result = mix_frames(AudioFrame::Mono(0.3), AudioFrame::Mono(0.5));
    assert!(matches!(result, AudioFrame::Mono(v) if (v - 0.8).abs() < 1e-6));
}

#[test]
fn mix_frames_stereo_stereo_sums() {
    use super::mix_frames;
    let result = mix_frames(
        AudioFrame::Stereo([0.1, 0.2]),
        AudioFrame::Stereo([0.3, 0.4]),
    );
    match result {
        AudioFrame::Stereo([l, r]) => {
            assert!((l - 0.4).abs() < 1e-6);
            assert!((r - 0.6).abs() < 1e-6);
        }
        _ => panic!("expected stereo"),
    }
}

#[test]
fn mix_frames_mono_stereo_widens() {
    use super::mix_frames;
    let result = mix_frames(AudioFrame::Mono(0.5), AudioFrame::Stereo([0.1, 0.2]));
    match result {
        AudioFrame::Stereo([l, r]) => {
            assert!((l - 0.6).abs() < 1e-6);
            assert!((r - 0.7).abs() < 1e-6);
        }
        _ => panic!("expected stereo"),
    }
}

#[test]
fn mix_frames_stereo_mono_widens() {
    use super::mix_frames;
    let result = mix_frames(AudioFrame::Stereo([0.1, 0.2]), AudioFrame::Mono(0.5));
    match result {
        AudioFrame::Stereo([l, r]) => {
            assert!((l - 0.6).abs() < 1e-6);
            assert!((r - 0.7).abs() < 1e-6);
        }
        _ => panic!("expected stereo"),
    }
}

// ── output_limiter tests ─────────────────────────────────────────────────

#[test]
fn output_limiter_transparent_below_threshold() {
    use super::output_limiter;
    assert!((output_limiter(0.5) - 0.5).abs() < 1e-6);
    assert!((output_limiter(-0.5) - (-0.5)).abs() < 1e-6);
    assert!((output_limiter(0.0) - 0.0).abs() < 1e-6);
    assert!((output_limiter(0.94) - 0.94).abs() < 1e-6);
}

#[test]
fn output_limiter_saturates_above_threshold() {
    use super::output_limiter;
    let limited = output_limiter(2.0);
    assert!(limited < 2.0, "limiter should reduce values above threshold");
    assert!(limited > 0.0, "limiter should keep positive sign");
    // tanh(2.0) ≈ 0.964
    assert!((limited - 2.0f32.tanh()).abs() < 1e-6);
}

#[test]
fn output_limiter_negative_saturates_symmetrically() {
    use super::output_limiter;
    let limited = output_limiter(-2.0);
    assert!(limited > -2.0, "limiter should reduce negative values");
    assert!((limited - (-2.0f32).tanh()).abs() < 1e-6);
}

// ── apply_mixdown tests ──────────────────────────────────────────────────

#[test]
fn apply_mixdown_sum_adds_channels() {
    use super::apply_mixdown;
    use project::chain::ChainOutputMixdown;
    assert!((apply_mixdown(ChainOutputMixdown::Sum, 0.3, 0.5) - 0.8).abs() < 1e-6);
}

#[test]
fn apply_mixdown_average_averages_channels() {
    use super::apply_mixdown;
    use project::chain::ChainOutputMixdown;
    assert!((apply_mixdown(ChainOutputMixdown::Average, 0.4, 0.8) - 0.6).abs() < 1e-6);
}

#[test]
fn apply_mixdown_left_returns_left() {
    use super::apply_mixdown;
    use project::chain::ChainOutputMixdown;
    assert!((apply_mixdown(ChainOutputMixdown::Left, 0.3, 0.7) - 0.3).abs() < 1e-6);
}

#[test]
fn apply_mixdown_right_returns_right() {
    use super::apply_mixdown;
    use project::chain::ChainOutputMixdown;
    assert!((apply_mixdown(ChainOutputMixdown::Right, 0.3, 0.7) - 0.7).abs() < 1e-6);
}

// ── layout_from_channels tests ───────────────────────────────────────────

#[test]
fn layout_from_channels_mono_ok() {
    use super::layout_from_channels;
    assert_eq!(layout_from_channels(1).unwrap(), AudioChannelLayout::Mono);
}

#[test]
fn layout_from_channels_stereo_ok() {
    use super::layout_from_channels;
    assert_eq!(layout_from_channels(2).unwrap(), AudioChannelLayout::Stereo);
}

#[test]
fn layout_from_channels_invalid_errors() {
    use super::layout_from_channels;
    assert!(layout_from_channels(0).is_err());
    assert!(layout_from_channels(3).is_err());
    assert!(layout_from_channels(8).is_err());
}

// ── write_output_frame tests ─────────────────────────────────────────────

#[test]
fn write_output_frame_mono_to_single_channel() {
    use super::write_output_frame;
    use project::chain::ChainOutputMixdown;
    let mut frame = [0.0f32; 2];
    write_output_frame(AudioFrame::Mono(0.5), &[1], &mut frame, ChainOutputMixdown::Average);
    assert!((frame[0] - 0.0).abs() < 1e-6, "channel 0 should be untouched");
    assert!((frame[1] - 0.5).abs() < 1e-6, "channel 1 should have the sample");
}

#[test]
fn write_output_frame_mono_to_multiple_channels() {
    use super::write_output_frame;
    use project::chain::ChainOutputMixdown;
    let mut frame = [0.0f32; 4];
    write_output_frame(AudioFrame::Mono(0.8), &[0, 2, 3], &mut frame, ChainOutputMixdown::Average);
    assert!((frame[0] - 0.8).abs() < 1e-6);
    assert!((frame[1] - 0.0).abs() < 1e-6);
    assert!((frame[2] - 0.8).abs() < 1e-6);
    assert!((frame[3] - 0.8).abs() < 1e-6);
}

#[test]
fn write_output_frame_stereo_to_zero_channels() {
    use super::write_output_frame;
    use project::chain::ChainOutputMixdown;
    let mut frame = [0.0f32; 2];
    // Empty channels — should not write anything
    write_output_frame(AudioFrame::Stereo([0.5, 0.7]), &[], &mut frame, ChainOutputMixdown::Average);
    assert_eq!(frame, [0.0, 0.0]);
}

#[test]
fn write_output_frame_stereo_to_one_channel_uses_mixdown() {
    use super::write_output_frame;
    use project::chain::ChainOutputMixdown;
    let mut frame = [0.0f32; 2];
    write_output_frame(AudioFrame::Stereo([0.4, 0.8]), &[0], &mut frame, ChainOutputMixdown::Average);
    // Average of 0.4 and 0.8 = 0.6
    assert!((frame[0] - 0.6).abs() < 1e-6);
}

#[test]
fn write_output_frame_stereo_to_two_channels_preserves_lr() {
    use super::write_output_frame;
    use project::chain::ChainOutputMixdown;
    let mut frame = [0.0f32; 4];
    write_output_frame(AudioFrame::Stereo([0.3, 0.7]), &[1, 3], &mut frame, ChainOutputMixdown::Average);
    assert!((frame[0] - 0.0).abs() < 1e-6);
    assert!((frame[1] - 0.3).abs() < 1e-6);
    assert!((frame[2] - 0.0).abs() < 1e-6);
    assert!((frame[3] - 0.7).abs() < 1e-6);
}

// ── read_input_frame tests ───────────────────────────────────────────────

#[test]
fn read_input_frame_mono_reads_correct_channel() {
    use super::read_input_frame;
    let data = [0.1, 0.9, 0.5, 0.3];
    let frame = read_input_frame(AudioChannelLayout::Mono, &[2], &data);
    assert!(matches!(frame, AudioFrame::Mono(v) if (v - 0.5).abs() < 1e-6));
}

#[test]
fn read_input_frame_stereo_reads_two_channels() {
    use super::read_input_frame;
    let data = [0.1, 0.2, 0.3, 0.4];
    let frame = read_input_frame(AudioChannelLayout::Stereo, &[1, 3], &data);
    match frame {
        AudioFrame::Stereo([l, r]) => {
            assert!((l - 0.2).abs() < 1e-6);
            assert!((r - 0.4).abs() < 1e-6);
        }
        _ => panic!("expected stereo"),
    }
}

#[test]
fn read_input_frame_out_of_bounds_returns_zero() {
    use super::read_input_frame;
    let data = [0.5f32; 2];
    let frame = read_input_frame(AudioChannelLayout::Mono, &[99], &data);
    assert!(matches!(frame, AudioFrame::Mono(v) if v.abs() < 1e-6));
}

// ── silent_frame tests ───────────────────────────────────────────────────

#[test]
fn silent_frame_mono_is_zero() {
    use super::silent_frame;
    let frame = silent_frame(AudioChannelLayout::Mono);
    assert!(matches!(frame, AudioFrame::Mono(v) if v.abs() < 1e-6));
}

#[test]
fn silent_frame_stereo_is_zero() {
    use super::silent_frame;
    let frame = silent_frame(AudioChannelLayout::Stereo);
    assert!(matches!(frame, AudioFrame::Stereo([l, r]) if l.abs() < 1e-6 && r.abs() < 1e-6));
}

// ── build_runtime_graph edge cases ───────────────────────────────────────

#[test]
fn build_runtime_graph_skips_disabled_chains() {
    let project = Project {
        name: None,
        device_settings: Vec::new(),
        chains: vec![Chain {
            id: ChainId("disabled".into()),
            description: None,
            instrument: "electric_guitar".to_string(),
            enabled: false,
            blocks: vec![],
        }],
    };

    let runtime = build_runtime_graph(&project, &HashMap::new(), &HashMap::new())
        .expect("build should succeed with disabled chain");
    assert!(runtime.chains.is_empty(), "disabled chains should be skipped");
}

#[test]
fn build_runtime_graph_errors_on_missing_sample_rate() {
    let project = Project {
        name: None,
        device_settings: Vec::new(),
        chains: vec![Chain {
            id: ChainId("chain:0".into()),
            description: None,
            instrument: "electric_guitar".to_string(),
            enabled: true,
            blocks: vec![],
        }],
    };

    let result = build_runtime_graph(&project, &HashMap::new(), &HashMap::new());
    assert!(result.is_err(), "should error when chain has no sample rate");
}

// ── RuntimeGraph methods ─────────────────────────────────────────────────

#[test]
fn runtime_graph_remove_chain_removes_entry() {
    let chain = tuner_track("chain:remove", Vec::new());
    let mut rates = HashMap::new();
    rates.insert(ChainId("chain:remove".into()), 48_000.0);
    let project = Project {
        name: None,
        device_settings: Vec::new(),
        chains: vec![chain],
    };
    let mut graph = build_runtime_graph(&project, &rates, &HashMap::new()).unwrap();
    assert_eq!(graph.chains.len(), 1);
    graph.remove_chain(&ChainId("chain:remove".into()));
    assert!(graph.chains.is_empty());
}

#[test]
fn runtime_graph_runtime_for_chain_returns_none_for_unknown() {
    use super::RuntimeGraph;
    let graph = RuntimeGraph { chains: HashMap::new() };
    assert!(graph.runtime_for_chain(&ChainId("nonexistent".into())).is_none());
}

#[test]
fn runtime_graph_upsert_chain_creates_new_entry() {
    use super::RuntimeGraph;
    let mut graph = RuntimeGraph { chains: HashMap::new() };
    let chain = tuner_track("chain:new", Vec::new());
    let result = graph.upsert_chain(&chain, 48_000.0, false, &[DEFAULT_ELASTIC_TARGET]);
    assert!(result.is_ok());
    assert_eq!(graph.chains.len(), 1);
}

#[test]
fn runtime_graph_upsert_chain_updates_existing() {
    use super::RuntimeGraph;
    let mut graph = RuntimeGraph { chains: HashMap::new() };
    let chain = tuner_track("chain:upsert", vec![tuner_block("b:0", 440.0)]);
    graph.upsert_chain(&chain, 48_000.0, false, &[DEFAULT_ELASTIC_TARGET]).unwrap();
    // Update — should reuse existing entry
    let chain2 = tuner_track("chain:upsert", vec![tuner_block("b:0", 445.0)]);
    let result = graph.upsert_chain(&chain2, 48_000.0, false, &[DEFAULT_ELASTIC_TARGET]);
    assert!(result.is_ok());
    assert_eq!(graph.chains.len(), 1);
}

// ── process_output_f32 edge cases ────────────────────────────────────────

#[test]
fn process_output_fills_silence_for_invalid_output_index() {
    let chain = io_passthrough_chain("chain:0");
    let runtime =
        Arc::new(build_chain_runtime_state(&chain, 48_000.0, &[DEFAULT_ELASTIC_TARGET]).expect("runtime should build"));

    let mut out = vec![1.0f32; 8];
    process_output_f32(&runtime, 999, &mut out, 1);

    assert!(out.iter().all(|&v| v == 0.0),
        "invalid output index should fill with silence");
}

#[test]
fn process_output_underrun_repeats_last_frame() {
    let chain = io_passthrough_chain("chain:underrun");
    let runtime =
        Arc::new(build_chain_runtime_state(&chain, 48_000.0, &[DEFAULT_ELASTIC_TARGET]).expect("runtime should build"));

    // Push enough frames to get past the fade-in, then push our test frames
    let warmup = vec![0.0f32; FADE_IN_FRAMES + 16];
    process_input_f32(&runtime, 0, &warmup, 1);
    // Drain warmup frames
    let mut drain = vec![0.0f32; warmup.len()];
    process_output_f32(&runtime, 0, &mut drain, 1);

    // Now push only 2 frames (no fade-in active)
    process_input_f32(&runtime, 0, &[0.5, 0.7], 1);
    // Request 4 frames — last 2 should repeat the last pushed frame
    let mut out = vec![0.0f32; 4];
    process_output_f32(&runtime, 0, &mut out, 1);

    assert!((out[0] - 0.5).abs() < 1e-6, "frame 0 should be 0.5, got {}", out[0]);
    assert!((out[1] - 0.7).abs() < 1e-6, "frame 1 should be 0.7, got {}", out[1]);
    // Frames 2 and 3 should be the last frame (0.7) repeated
    assert!((out[2] - 0.7).abs() < 1e-6, "frame 2 should repeat last: 0.7, got {}", out[2]);
    assert!((out[3] - 0.7).abs() < 1e-6, "frame 3 should repeat last: 0.7, got {}", out[3]);
}

// ── ChainRuntimeState method tests ───────────────────────────────────────

#[test]
fn measured_latency_ms_returns_zero_initially() {
    let chain = tuner_track("chain:0", Vec::new());
    let runtime = build_chain_runtime_state(&chain, 48_000.0, &[DEFAULT_ELASTIC_TARGET]).unwrap();
    assert!((runtime.measured_latency_ms() - 0.0).abs() < 1e-6);
}

#[test]
fn poll_errors_drains_and_returns_all() {
    let chain = tuner_track("chain:0", Vec::new());
    let runtime = build_chain_runtime_state(&chain, 48_000.0, &[DEFAULT_ELASTIC_TARGET]).unwrap();
    // Manually push errors
    runtime.error_queue.push(BlockError { block_id: BlockId("err:1".into()), message: "oops".into() }).unwrap();
    runtime.error_queue.push(BlockError { block_id: BlockId("err:2".into()), message: "boom".into() }).unwrap();
    let errors = runtime.poll_errors();
    assert_eq!(errors.len(), 2);
    // Second call should be empty
    let errors2 = runtime.poll_errors();
    assert!(errors2.is_empty(), "poll_errors should drain the queue");
}

#[test]
fn poll_stream_returns_none_for_unknown_block() {
    let chain = tuner_track("chain:0", Vec::new());
    let runtime = build_chain_runtime_state(&chain, 48_000.0, &[DEFAULT_ELASTIC_TARGET]).unwrap();
    assert!(runtime.poll_stream(&BlockId("nonexistent".into())).is_none());
}

// ── effective_inputs / effective_outputs with Insert blocks ───────────────

fn insert_chain() -> Chain {
    Chain {
        id: ChainId("chain:insert".into()),
        description: None,
        instrument: "electric_guitar".to_string(),
        enabled: true,
        blocks: vec![
            AudioBlock {
                id: BlockId("input:0".into()),
                enabled: true,
                kind: AudioBlockKind::Input(InputBlock {
                    model: "standard".into(),
                    entries: vec![InputEntry {
                        device_id: DeviceId("dev_in".into()),
                        mode: ChainInputMode::Mono,
                        channels: vec![0],
                    }],
                }),
            },
            AudioBlock {
                id: BlockId("comp:0".into()),
                enabled: true,
                kind: AudioBlockKind::Core(CoreBlock {
                    effect_type: "gain".into(),
                    model: "volume".into(),
                    params: ParameterSet::default(),
                }),
            },
            AudioBlock {
                id: BlockId("insert:0".into()),
                enabled: true,
                kind: AudioBlockKind::Insert(InsertBlock {
                    model: "external_loop".into(),
                    send: InsertEndpoint {
                        device_id: DeviceId("send_dev".into()),
                        mode: ChainInputMode::Stereo,
                        channels: vec![0, 1],
                    },
                    return_: InsertEndpoint {
                        device_id: DeviceId("return_dev".into()),
                        mode: ChainInputMode::Stereo,
                        channels: vec![0, 1],
                    },
                }),
            },
            AudioBlock {
                id: BlockId("delay:0".into()),
                enabled: true,
                kind: AudioBlockKind::Core(CoreBlock {
                    effect_type: "gain".into(),
                    model: "volume".into(),
                    params: ParameterSet::default(),
                }),
            },
            AudioBlock {
                id: BlockId("output:0".into()),
                enabled: true,
                kind: AudioBlockKind::Output(OutputBlock {
                    model: "standard".into(),
                    entries: vec![OutputEntry {
                        device_id: DeviceId("dev_out".into()),
                        mode: ChainOutputMode::Stereo,
                        channels: vec![0, 1],
                    }],
                }),
            },
        ],
    }
}

#[test]
fn effective_inputs_includes_insert_return() {
    let chain = insert_chain();
    let (eff_inputs, cpal_indices) = effective_inputs(&chain);
    // Should have: 1 regular input + 1 insert return = 2
    assert_eq!(eff_inputs.len(), 2);
    assert_eq!(cpal_indices.len(), 2);
}

#[test]
fn effective_outputs_includes_insert_send() {
    let chain = insert_chain();
    let eff_outputs = effective_outputs(&chain);
    // Should have: 1 regular output + 1 insert send = 2
    assert_eq!(eff_outputs.len(), 2);
    assert_eq!(eff_outputs.len(), 2);
}

#[test]
fn split_chain_with_insert_produces_two_segments() {
    let chain = insert_chain();
    let (eff_inputs, cpal_indices) = effective_inputs(&chain);
    let eff_outputs = effective_outputs(&chain);
    let segments = split_chain_into_segments(&chain, &eff_inputs, &cpal_indices, &eff_outputs);

    // Should have 2 segments: before insert and after insert
    assert_eq!(segments.len(), 2, "insert should split chain into 2 segments");

    // Segment 0: input → [comp:0] → insert send
    assert_eq!(segments[0].block_indices, vec![1],
        "first segment should contain only effect blocks before insert");

    // Segment 1: insert return → [delay:0] → output
    assert_eq!(segments[1].block_indices, vec![3],
        "second segment should contain only effect blocks after insert");
}

#[test]
fn split_chain_with_disabled_insert_produces_one_segment() {
    let mut chain = insert_chain();
    // Disable the insert block
    chain.blocks[2].enabled = false;
    let (eff_inputs, cpal_indices) = effective_inputs(&chain);
    let eff_outputs = effective_outputs(&chain);
    let segments = split_chain_into_segments(&chain, &eff_inputs, &cpal_indices, &eff_outputs);

    // Disabled insert should not split the chain
    assert_eq!(segments.len(), 1, "disabled insert should not split the chain");
}

// ── effective_inputs with mono multi-channel splitting ────────────────────

#[test]
fn effective_inputs_splits_mono_multichannel_entry() {
    let chain = Chain {
        id: ChainId("chain:split".into()),
        description: None,
        instrument: "electric_guitar".to_string(),
        enabled: true,
        blocks: vec![AudioBlock {
            id: BlockId("input:0".into()),
            enabled: true,
            kind: AudioBlockKind::Input(InputBlock {
                model: "standard".into(),
                entries: vec![InputEntry {
                    device_id: DeviceId("dev".into()),
                    mode: ChainInputMode::Mono,
                    channels: vec![0, 1, 2],
                }],
            }),
        }],
    };
    let (eff_inputs, cpal_indices) = effective_inputs(&chain);
    assert_eq!(eff_inputs.len(), 3, "mono entry with 3 channels should split into 3 entries");
    assert_eq!(eff_inputs[0].channels, vec![0]);
    assert_eq!(eff_inputs[1].channels, vec![1]);
    assert_eq!(eff_inputs[2].channels, vec![2]);
    // All should share the same CPAL stream index
    assert_eq!(cpal_indices[0], cpal_indices[1]);
    assert_eq!(cpal_indices[1], cpal_indices[2]);
}

// ── effective_inputs / outputs fallback ───────────────────────────────────

#[test]
fn effective_inputs_fallback_when_no_input_blocks() {
    let chain = Chain {
        id: ChainId("chain:fallback".into()),
        description: None,
        instrument: "electric_guitar".to_string(),
        enabled: true,
        blocks: vec![],
    };
    let (eff_inputs, cpal_indices) = effective_inputs(&chain);
    assert_eq!(eff_inputs.len(), 1, "fallback should produce exactly 1 input");
    assert_eq!(cpal_indices, vec![0]);
}

#[test]
fn effective_outputs_fallback_when_no_output_blocks() {
    let chain = Chain {
        id: ChainId("chain:fallback".into()),
        description: None,
        instrument: "electric_guitar".to_string(),
        enabled: true,
        blocks: vec![],
    };
    let eff_outputs = effective_outputs(&chain);
    assert_eq!(eff_outputs.len(), 1, "fallback should produce exactly 1 output");
}

// ── downcast_panic_message tests ─────────────────────────────────────────

#[test]
fn downcast_panic_str_message() {
    use super::downcast_panic_message;
    let payload: Box<dyn std::any::Any + Send> = Box::new("static string");
    assert_eq!(downcast_panic_message(payload), "static string");
}

#[test]
fn downcast_panic_string_message() {
    use super::downcast_panic_message;
    let payload: Box<dyn std::any::Any + Send> = Box::new(String::from("owned string"));
    assert_eq!(downcast_panic_message(payload), "owned string");
}

#[test]
fn downcast_panic_unknown_type_message() {
    use super::downcast_panic_message;
    let payload: Box<dyn std::any::Any + Send> = Box::new(42i32);
    assert_eq!(downcast_panic_message(payload), "unknown panic");
}

// ── process_input_f32 with stereo I/O ────────────────────────────────────

#[test]
fn process_input_stereo_output_preserves_channels() {
    let chain = Chain {
        id: ChainId("chain:stereo-io".into()),
        description: None,
        instrument: "electric_guitar".to_string(),
        enabled: true,
        blocks: vec![
            AudioBlock {
                id: BlockId("input:0".into()),
                enabled: true,
                kind: AudioBlockKind::Input(InputBlock {
                    model: "standard".into(),
                    entries: vec![InputEntry {
                        device_id: DeviceId("dev".into()),
                        mode: ChainInputMode::Stereo,
                        channels: vec![0, 1],
                    }],
                }),
            },
            AudioBlock {
                id: BlockId("output:0".into()),
                enabled: true,
                kind: AudioBlockKind::Output(OutputBlock {
                    model: "standard".into(),
                    entries: vec![OutputEntry {
                        device_id: DeviceId("dev".into()),
                        mode: ChainOutputMode::Stereo,
                        channels: vec![0, 1],
                    }],
                }),
            },
        ],
    };
    let runtime =
        Arc::new(build_chain_runtime_state(&chain, 48_000.0, &[DEFAULT_ELASTIC_TARGET]).expect("runtime should build"));

    // Push enough frames to get past fade-in (interleaved stereo)
    let warmup = vec![0.0f32; (FADE_IN_FRAMES + 16) * 2];
    process_input_f32(&runtime, 0, &warmup, 2);
    let mut drain = vec![0.0f32; warmup.len()];
    process_output_f32(&runtime, 0, &mut drain, 2);

    // Interleaved stereo: L=0.3, R=0.7, L=0.5, R=0.9
    let input = [0.3f32, 0.7, 0.5, 0.9];
    process_input_f32(&runtime, 0, &input, 2);

    let mut out = vec![0.0f32; 4];
    process_output_f32(&runtime, 0, &mut out, 2);

    assert!((out[0] - 0.3).abs() < 1e-6, "left ch frame 0: got {}", out[0]);
    assert!((out[1] - 0.7).abs() < 1e-6, "right ch frame 0: got {}", out[1]);
    assert!((out[2] - 0.5).abs() < 1e-6, "left ch frame 1: got {}", out[2]);
    assert!((out[3] - 0.9).abs() < 1e-6, "right ch frame 1: got {}", out[3]);
}

// ── update_chain_runtime_state with reset_output_queue ───────────────────

#[test]
fn update_chain_runtime_state_with_reset_output_queue() {
    let chain = io_passthrough_chain("chain:0");
    let runtime =
        Arc::new(build_chain_runtime_state(&chain, 48_000.0, &[DEFAULT_ELASTIC_TARGET]).expect("runtime should build"));

    // Push some data
    process_input_f32(&runtime, 0, &[0.5, 0.7], 1);

    // Update with reset_output_queue=true should clear the buffer
    update_chain_runtime_state(&runtime, &chain, 48_000.0, true, &[DEFAULT_ELASTIC_TARGET])
        .expect("update should succeed");

    let mut out = vec![0.0f32; 2];
    process_output_f32(&runtime, 0, &mut out, 1);
    // After reset, output should be silence (no frames in queue)
    assert!(out.iter().all(|&v| v.abs() < 1e-6),
        "after reset_output_queue, output should be silent");
}

// ── layout_label tests ───────────────────────────────────────────────────

#[test]
fn layout_label_returns_correct_strings() {
    use super::layout_label;
    assert_eq!(layout_label(AudioChannelLayout::Mono), "mono");
    assert_eq!(layout_label(AudioChannelLayout::Stereo), "stereo");
}

// ── insert_return_as_input_entry tests ──────────────────────────────────

#[test]
fn insert_return_as_input_entry_copies_return_endpoint() {
    use super::insert_return_as_input_entry;
    let insert = InsertBlock {
        model: "external_loop".into(),
        send: InsertEndpoint {
            device_id: DeviceId("send_dev".into()),
            mode: ChainInputMode::Stereo,
            channels: vec![0, 1],
        },
        return_: InsertEndpoint {
            device_id: DeviceId("return_dev".into()),
            mode: ChainInputMode::Mono,
            channels: vec![2],
        },
    };
    let entry = insert_return_as_input_entry(&insert);
    assert_eq!(entry.device_id.0, "return_dev");
    assert_eq!(entry.channels, vec![2]);
    assert!(matches!(entry.mode, ChainInputMode::Mono));
}

// ── insert_send_as_output_entry tests ───────────────────────────────────

#[test]
fn insert_send_as_output_entry_mono_mode() {
    use super::insert_send_as_output_entry;
    let insert = InsertBlock {
        model: "external_loop".into(),
        send: InsertEndpoint {
            device_id: DeviceId("send_dev".into()),
            mode: ChainInputMode::Mono,
            channels: vec![0],
        },
        return_: InsertEndpoint {
            device_id: DeviceId("return_dev".into()),
            mode: ChainInputMode::Stereo,
            channels: vec![0, 1],
        },
    };
    let entry = insert_send_as_output_entry(&insert);
    assert_eq!(entry.device_id.0, "send_dev");
    assert_eq!(entry.channels, vec![0]);
    assert!(matches!(entry.mode, ChainOutputMode::Mono));
}

#[test]
fn insert_send_as_output_entry_stereo_mode() {
    use super::insert_send_as_output_entry;
    let insert = InsertBlock {
        model: "external_loop".into(),
        send: InsertEndpoint {
            device_id: DeviceId("send_dev".into()),
            mode: ChainInputMode::Stereo,
            channels: vec![0, 1],
        },
        return_: InsertEndpoint {
            device_id: DeviceId("return_dev".into()),
            mode: ChainInputMode::Mono,
            channels: vec![0],
        },
    };
    let entry = insert_send_as_output_entry(&insert);
    assert!(matches!(entry.mode, ChainOutputMode::Stereo));
}

#[test]
fn insert_send_as_output_entry_dual_mono_becomes_stereo() {
    use super::insert_send_as_output_entry;
    let insert = InsertBlock {
        model: "external_loop".into(),
        send: InsertEndpoint {
            device_id: DeviceId("send_dev".into()),
            mode: ChainInputMode::DualMono,
            channels: vec![0, 1],
        },
        return_: InsertEndpoint {
            device_id: DeviceId("return_dev".into()),
            mode: ChainInputMode::Mono,
            channels: vec![0],
        },
    };
    let entry = insert_send_as_output_entry(&insert);
    assert!(matches!(entry.mode, ChainOutputMode::Stereo));
}

// ── next_block_instance_serial tests ────────────────────────────────────

#[test]
fn next_block_instance_serial_increments() {
    use super::next_block_instance_serial;
    let a = next_block_instance_serial();
    let b = next_block_instance_serial();
    assert!(b > a, "serial should increment monotonically");
}

// ── build_output_routing_state tests ────────────────────────────────────

#[test]
fn build_output_routing_state_mono_single_channel() {
    use super::build_output_routing_state;
    let output = OutputEntry {
        device_id: DeviceId("dev".into()),
        mode: ChainOutputMode::Mono,
        channels: vec![0],
    };
    let state = build_output_routing_state(&output, DEFAULT_ELASTIC_TARGET);
    assert_eq!(state.output_channels, vec![0]);
}

#[test]
fn build_output_routing_state_stereo_two_channels() {
    use super::build_output_routing_state;
    let output = OutputEntry {
        device_id: DeviceId("dev".into()),
        mode: ChainOutputMode::Stereo,
        channels: vec![0, 1],
    };
    let state = build_output_routing_state(&output, DEFAULT_ELASTIC_TARGET);
    assert_eq!(state.output_channels, vec![0, 1]);
}

#[test]
fn build_output_routing_state_mono_mode_with_two_channels_uses_mono() {
    use super::build_output_routing_state;
    let output = OutputEntry {
        device_id: DeviceId("dev".into()),
        mode: ChainOutputMode::Mono,
        channels: vec![0, 1],
    };
    let _state = build_output_routing_state(&output, DEFAULT_ELASTIC_TARGET);
    // Mono mode with 2 channels: layout should be Mono per the logic
    // Just verifying it doesn't panic and runs correctly
}

// ── read_channel edge cases ─────────────────────────────────────────────

#[test]
fn read_channel_valid_index() {
    use super::read_channel;
    let data = [0.1, 0.2, 0.3];
    assert!((read_channel(&data, 1) - 0.2).abs() < 1e-6);
}

#[test]
fn read_channel_out_of_bounds_returns_zero() {
    use super::read_channel;
    let data = [0.5, 0.7];
    assert!((read_channel(&data, 10)).abs() < 1e-6);
}

#[test]
fn read_channel_empty_data_returns_zero() {
    use super::read_channel;
    let data: [f32; 0] = [];
    assert!((read_channel(&data, 0)).abs() < 1e-6);
}

// ── runtime graph with multiple chains ──────────────────────────────────

#[test]
fn build_runtime_graph_with_multiple_enabled_chains() {
    let project = Project {
        name: None,
        device_settings: Vec::new(),
        chains: vec![
            tuner_track("chain:0", Vec::new()),
            tuner_track("chain:1", vec![tuner_block("b:0", 440.0)]),
        ],
    };
    let mut rates = HashMap::new();
    rates.insert(ChainId("chain:0".into()), 48_000.0);
    rates.insert(ChainId("chain:1".into()), 48_000.0);

    let runtime = build_runtime_graph(&project, &rates, &HashMap::new())
        .expect("should build with multiple chains");
    assert_eq!(runtime.chains.len(), 2);
}

#[test]
fn build_runtime_graph_mixed_enabled_and_disabled() {
    let project = Project {
        name: None,
        device_settings: Vec::new(),
        chains: vec![
            Chain {
                id: ChainId("disabled".into()),
                description: None,
                instrument: "electric_guitar".to_string(),
                enabled: false,
                blocks: vec![],
            },
            tuner_track("enabled", vec![tuner_block("b:0", 440.0)]),
        ],
    };
    let mut rates = HashMap::new();
    rates.insert(ChainId("enabled".into()), 48_000.0);

    let runtime = build_runtime_graph(&project, &rates, &HashMap::new()).unwrap();
    assert_eq!(runtime.chains.len(), 1);
    assert!(runtime.chains.contains_key(&ChainId("enabled".into())));
}

// ── process_input_f32 edge cases ────────────────────────────────────────

#[test]
fn process_input_with_empty_data_does_not_panic() {
    let chain = io_passthrough_chain("chain:0");
    let runtime =
        Arc::new(build_chain_runtime_state(&chain, 48_000.0, &[DEFAULT_ELASTIC_TARGET]).expect("runtime should build"));
    process_input_f32(&runtime, 0, &[], 1);
}

#[test]
fn process_input_with_invalid_index_does_not_panic() {
    let chain = io_passthrough_chain("chain:0");
    let runtime =
        Arc::new(build_chain_runtime_state(&chain, 48_000.0, &[DEFAULT_ELASTIC_TARGET]).expect("runtime should build"));
    process_input_f32(&runtime, 999, &[0.5, 0.7], 1);
}

// ── input tap ───────────────────────────────────────────────────────────

#[test]
fn subscribe_input_tap_receives_pre_fx_samples() {
    let chain = io_passthrough_chain("chain:0");
    let runtime = Arc::new(
        build_chain_runtime_state(&chain, 48_000.0, &[DEFAULT_ELASTIC_TARGET])
            .expect("runtime should build"),
    );

    // Subscribe to input 0, channel 0 (mono input).
    let rings = runtime.subscribe_input_tap(0, 1, &[0], 256);
    assert_eq!(rings.len(), 1, "one ring per subscribed channel");

    // Feed a known buffer through process_input_f32.
    let data = [0.1_f32, 0.2, 0.3, 0.4];
    process_input_f32(&runtime, 0, &data, 1);

    // Drain the ring and confirm the same samples landed pre-FX.
    let mut received = Vec::new();
    while let Some(s) = rings[0].pop() {
        received.push(s);
    }
    assert_eq!(received, vec![0.1, 0.2, 0.3, 0.4]);
}

#[test]
fn subscribe_input_tap_only_targets_matching_input_index() {
    let chain = io_passthrough_chain("chain:0");
    let runtime = Arc::new(
        build_chain_runtime_state(&chain, 48_000.0, &[DEFAULT_ELASTIC_TARGET])
            .expect("runtime should build"),
    );

    let rings = runtime.subscribe_input_tap(0, 1, &[0], 256);

    // Push to a *different* input index — tap should not see the samples.
    process_input_f32(&runtime, 99, &[0.5, 0.6], 1);
    assert!(rings[0].pop().is_none(), "tap on input 0 must ignore input 99");
}

#[test]
fn prune_dead_input_taps_removes_unused() {
    let chain = io_passthrough_chain("chain:0");
    let runtime = Arc::new(
        build_chain_runtime_state(&chain, 48_000.0, &[DEFAULT_ELASTIC_TARGET])
            .expect("runtime should build"),
    );

    {
        let rings = runtime.subscribe_input_tap(0, 1, &[0], 64);
        assert_eq!(rings.len(), 1);
        assert_eq!(runtime.input_taps.load().len(), 1);
    }
    // rings out of scope — only the runtime's InputTap holds the channel
    // ring Arc, so its strong_count == 1 and prune drops it.
    runtime.prune_dead_input_taps();
    assert_eq!(runtime.input_taps.load().len(), 0);
}

// ── stream tap ──────────────────────────────────────────────────────────

#[test]
fn subscribe_stream_tap_receives_post_fx_stereo() {
    let chain = io_passthrough_chain("chain:0");
    let runtime = Arc::new(
        build_chain_runtime_state(&chain, 48_000.0, &[DEFAULT_ELASTIC_TARGET])
            .expect("runtime should build"),
    );

    // Subscribe to stream 0 (the chain's single input pipeline).
    let [l_ring, r_ring] = runtime.subscribe_stream_tap(0, 256);

    // Drive the chain so the segment's frame_buffer has known data.
    process_input_f32(&runtime, 0, &[0.1_f32, 0.2, 0.3, 0.4], 1);

    // Mono input is upmixed before FX → both rings receive the same
    // samples after the (no-op) processing of the passthrough chain.
    let mut left = Vec::new();
    while let Some(s) = l_ring.pop() {
        left.push(s);
    }
    let mut right = Vec::new();
    while let Some(s) = r_ring.pop() {
        right.push(s);
    }
    assert_eq!(left.len(), 4, "left ring got {left:?}");
    assert_eq!(right.len(), 4, "right ring got {right:?}");
}

#[test]
fn subscribe_stream_tap_only_targets_matching_stream_index() {
    let chain = io_passthrough_chain("chain:0");
    let runtime = Arc::new(
        build_chain_runtime_state(&chain, 48_000.0, &[DEFAULT_ELASTIC_TARGET])
            .expect("runtime should build"),
    );

    // Subscribe to a stream that does not exist (chain has only one).
    let [l_ring, r_ring] = runtime.subscribe_stream_tap(99, 256);
    process_input_f32(&runtime, 0, &[0.1_f32, 0.2, 0.3, 0.4], 1);
    assert!(
        l_ring.pop().is_none(),
        "tap on stream 99 must ignore segment 0"
    );
    assert!(r_ring.pop().is_none());
}

#[test]
fn stream_tap_publishes_independent_of_output_mute() {
    let chain = io_passthrough_chain("chain:0");
    let runtime = Arc::new(
        build_chain_runtime_state(&chain, 48_000.0, &[DEFAULT_ELASTIC_TARGET])
            .expect("runtime should build"),
    );

    let [l_ring, r_ring] = runtime.subscribe_stream_tap(0, 256);
    // Mute the output — the device buffer will be zero, but the
    // stream tap is dispatched **before** the output stage so the
    // analyzer keeps receiving samples.
    runtime.set_output_muted(true);
    process_input_f32(&runtime, 0, &[0.1_f32, 0.2, 0.3, 0.4], 1);
    let mut out = vec![0.0_f32; 4];
    process_output_f32(&runtime, 0, &mut out, 1);

    assert!(
        out.iter().all(|&s| s == 0.0),
        "muted device output must be zero, got {:?}",
        out
    );
    // Stream tap kept feeding the analyzer.
    let mut left = Vec::new();
    while let Some(s) = l_ring.pop() {
        left.push(s);
    }
    let mut right = Vec::new();
    while let Some(s) = r_ring.pop() {
        right.push(s);
    }
    assert_eq!(left.len(), 4);
    assert_eq!(right.len(), 4);
}

#[test]
fn prune_dead_stream_taps_removes_unused() {
    let chain = io_passthrough_chain("chain:0");
    let runtime = Arc::new(
        build_chain_runtime_state(&chain, 48_000.0, &[DEFAULT_ELASTIC_TARGET])
            .expect("runtime should build"),
    );

    {
        let _rings = runtime.subscribe_stream_tap(0, 64);
        assert_eq!(runtime.stream_taps.load().len(), 1);
    }
    runtime.prune_dead_stream_taps();
    assert_eq!(runtime.stream_taps.load().len(), 0);
}

// ── output_muted flag ────────────────────────────────────────────────────

#[test]
fn output_muted_defaults_to_false() {
    let chain = io_passthrough_chain("chain:0");
    let runtime = Arc::new(
        build_chain_runtime_state(&chain, 48_000.0, &[DEFAULT_ELASTIC_TARGET])
            .expect("runtime should build"),
    );

    assert!(!runtime.is_output_muted());
}

#[test]
fn set_output_muted_round_trips() {
    let chain = io_passthrough_chain("chain:0");
    let runtime = Arc::new(
        build_chain_runtime_state(&chain, 48_000.0, &[DEFAULT_ELASTIC_TARGET])
            .expect("runtime should build"),
    );

    runtime.set_output_muted(true);
    assert!(runtime.is_output_muted());

    runtime.set_output_muted(false);
    assert!(!runtime.is_output_muted());
}

#[test]
fn output_muted_zeros_process_output_buffer() {
    let chain = io_passthrough_chain("chain:0");
    let runtime = Arc::new(
        build_chain_runtime_state(&chain, 48_000.0, &[DEFAULT_ELASTIC_TARGET])
            .expect("runtime should build"),
    );

    // Push some samples through the input so the chain has data
    // available when process_output_f32 runs.
    process_input_f32(&runtime, 0, &[0.1, 0.2, 0.3, 0.4], 1);

    runtime.set_output_muted(true);
    let mut out = vec![0.5_f32; 4];
    process_output_f32(&runtime, 0, &mut out, 1);
    assert!(
        out.iter().all(|&s| s == 0.0),
        "muted output must be all zero, got {:?}",
        out
    );
}

#[test]
fn output_muted_unset_does_not_zero_buffer() {
    let chain = io_passthrough_chain("chain:0");
    let runtime = Arc::new(
        build_chain_runtime_state(&chain, 48_000.0, &[DEFAULT_ELASTIC_TARGET])
            .expect("runtime should build"),
    );

    // Drive a known signal through input → passthrough → output.
    process_input_f32(&runtime, 0, &[0.1, 0.2, 0.3, 0.4], 1);
    let mut out = vec![0.0_f32; 4];
    process_output_f32(&runtime, 0, &mut out, 1);

    // With the mute flag off, the output buffer must contain the
    // forwarded samples — at least one frame is non-zero.
    assert!(
        out.iter().any(|&s| s != 0.0),
        "unmuted passthrough output must have non-zero samples, got {:?}",
        out
    );
}

// ── effective_inputs with stereo entry does not split ────────────────────

#[test]
fn effective_inputs_stereo_entry_not_split() {
    let chain = Chain {
        id: ChainId("chain:stereo".into()),
        description: None,
        instrument: "electric_guitar".to_string(),
        enabled: true,
        blocks: vec![AudioBlock {
            id: BlockId("input:0".into()),
            enabled: true,
            kind: AudioBlockKind::Input(InputBlock {
                model: "standard".into(),
                entries: vec![InputEntry {
                    device_id: DeviceId("dev".into()),
                    mode: ChainInputMode::Stereo,
                    channels: vec![0, 1],
                }],
            }),
        }],
    };
    let (eff_inputs, _) = effective_inputs(&chain);
    assert_eq!(eff_inputs.len(), 1, "stereo entry should not be split");
    assert_eq!(eff_inputs[0].channels, vec![0, 1]);
}

// ── effective_inputs with disabled input block ──────────────────────────

#[test]
fn effective_inputs_ignores_disabled_blocks() {
    let chain = Chain {
        id: ChainId("chain:disabled".into()),
        description: None,
        instrument: "electric_guitar".to_string(),
        enabled: true,
        blocks: vec![
            AudioBlock {
                id: BlockId("input:0".into()),
                enabled: false,
                kind: AudioBlockKind::Input(InputBlock {
                    model: "standard".into(),
                    entries: vec![InputEntry {
                        device_id: DeviceId("dev".into()),
                        mode: ChainInputMode::Mono,
                        channels: vec![0],
                    }],
                }),
            },
            AudioBlock {
                id: BlockId("output:0".into()),
                enabled: true,
                kind: AudioBlockKind::Output(OutputBlock {
                    model: "standard".into(),
                    entries: vec![OutputEntry {
                        device_id: DeviceId("dev".into()),
                        mode: ChainOutputMode::Mono,
                        channels: vec![0],
                    }],
                }),
            },
        ],
    };
    let (eff_inputs, _) = effective_inputs(&chain);
    // Disabled input block is ignored, so fallback
    assert_eq!(eff_inputs.len(), 1);
    assert_eq!(eff_inputs[0].device_id.0, "", "should fall back to default input");
}

// ── effective_outputs with disabled output block ─────────────────────────

#[test]
fn effective_outputs_ignores_disabled_blocks() {
    let chain = Chain {
        id: ChainId("chain:disabled-out".into()),
        description: None,
        instrument: "electric_guitar".to_string(),
        enabled: true,
        blocks: vec![AudioBlock {
            id: BlockId("output:0".into()),
            enabled: false,
            kind: AudioBlockKind::Output(OutputBlock {
                model: "standard".into(),
                entries: vec![OutputEntry {
                    device_id: DeviceId("dev".into()),
                    mode: ChainOutputMode::Mono,
                    channels: vec![0],
                }],
            }),
        }],
    };
    let eff_outputs = effective_outputs(&chain);
    assert_eq!(eff_outputs.len(), 1);
    assert_eq!(eff_outputs[0].device_id.0, "", "should fall back to default output");
}

// ── effective_inputs with multiple input blocks ─────────────────────────

#[test]
fn effective_inputs_multiple_input_blocks() {
    let chain = Chain {
        id: ChainId("chain:multi-in".into()),
        description: None,
        instrument: "electric_guitar".to_string(),
        enabled: true,
        blocks: vec![
            AudioBlock {
                id: BlockId("input:0".into()),
                enabled: true,
                kind: AudioBlockKind::Input(InputBlock {
                    model: "standard".into(),
                    entries: vec![InputEntry {
                        device_id: DeviceId("dev1".into()),
                        mode: ChainInputMode::Mono,
                        channels: vec![0],
                    }],
                }),
            },
            AudioBlock {
                id: BlockId("input:1".into()),
                enabled: true,
                kind: AudioBlockKind::Input(InputBlock {
                    model: "standard".into(),
                    entries: vec![InputEntry {
                        device_id: DeviceId("dev2".into()),
                        mode: ChainInputMode::Mono,
                        channels: vec![0],
                    }],
                }),
            },
        ],
    };
    let (eff_inputs, cpal_indices) = effective_inputs(&chain);
    assert_eq!(eff_inputs.len(), 2);
    assert_eq!(eff_inputs[0].device_id.0, "dev1");
    assert_eq!(eff_inputs[1].device_id.0, "dev2");
    // Different devices should have different CPAL indices
    assert_ne!(cpal_indices[0], cpal_indices[1]);
}

// ── effective_inputs same device shares cpal index ──────────────────────

#[test]
fn effective_inputs_same_device_shares_cpal_index() {
    let chain = Chain {
        id: ChainId("chain:same-dev".into()),
        description: None,
        instrument: "electric_guitar".to_string(),
        enabled: true,
        blocks: vec![AudioBlock {
            id: BlockId("input:0".into()),
            enabled: true,
            kind: AudioBlockKind::Input(InputBlock {
                model: "standard".into(),
                entries: vec![
                    InputEntry {
                        device_id: DeviceId("same_dev".into()),
                        mode: ChainInputMode::Mono,
                        channels: vec![0],
                    },
                    InputEntry {
                        device_id: DeviceId("same_dev".into()),
                        mode: ChainInputMode::Mono,
                        channels: vec![1],
                    },
                ],
            }),
        }],
    };
    let (eff_inputs, cpal_indices) = effective_inputs(&chain);
    assert_eq!(eff_inputs.len(), 2);
    assert_eq!(cpal_indices[0], cpal_indices[1], "same device should share CPAL index");
}

// ── build_chain_runtime_state with only effects (no I/O blocks) ─────────

#[test]
fn build_chain_runtime_state_no_io_blocks_uses_fallback() {
    let chain = Chain {
        id: ChainId("chain:no-io".into()),
        description: None,
        instrument: "electric_guitar".to_string(),
        enabled: true,
        blocks: vec![tuner_block("b:0", 440.0)],
    };
    let runtime = build_chain_runtime_state(&chain, 48_000.0, &[DEFAULT_ELASTIC_TARGET]);
    assert!(runtime.is_ok(), "should build with fallback I/O");
}

// ── process passthrough chain round-trip ────────────────────────────────

#[test]
fn passthrough_chain_round_trip_preserves_signal() {
    let chain = io_passthrough_chain("chain:rt");
    let runtime =
        Arc::new(build_chain_runtime_state(&chain, 48_000.0, &[DEFAULT_ELASTIC_TARGET]).expect("runtime should build"));

    // Warm up past fade-in
    let warmup = vec![0.0f32; FADE_IN_FRAMES + 64];
    process_input_f32(&runtime, 0, &warmup, 1);
    let mut drain = vec![0.0f32; warmup.len()];
    process_output_f32(&runtime, 0, &mut drain, 1);

    // Now test actual signal
    let input = [0.1f32, 0.2, 0.3, 0.4];
    process_input_f32(&runtime, 0, &input, 1);
    let mut out = vec![0.0f32; 4];
    process_output_f32(&runtime, 0, &mut out, 1);

    for (i, (&expected, &actual)) in input.iter().zip(out.iter()).enumerate() {
        assert!((expected - actual).abs() < 1e-6,
            "frame {i}: expected {expected}, got {actual}");
    }
}

// ── ChainRuntimeState measured_latency_ms with stored nanos ─────────────

#[test]
fn measured_latency_ms_converts_nanos_correctly() {
    let chain = tuner_track("chain:lat", Vec::new());
    let runtime = build_chain_runtime_state(&chain, 48_000.0, &[DEFAULT_ELASTIC_TARGET]).unwrap();
    // Store 5ms worth of nanos
    runtime.measured_latency_nanos.store(5_000_000, std::sync::atomic::Ordering::Relaxed);
    let ms = runtime.measured_latency_ms();
    assert!((ms - 5.0).abs() < 1e-3, "expected ~5.0ms, got {ms}");
}

// ── Regression: clear_draining re-arms callback after teardown (#316) ────

#[test]
fn chain_runtime_state_clear_draining_resets_flag() {
    // The draining flag is set by infra-cpal::teardown_active_chain_for_rebuild
    // before dropping the old streams. The Arc<ChainRuntimeState> is then
    // reused by the rebuild — without clear_draining the new streams' first
    // callbacks observe is_draining()==true and silence audio for every
    // segment (including sibling InputEntries on the same chain), until the
    // chain is fully removed and re-added. See issue #316.
    let chain = tuner_track("chain:drain", Vec::new());
    let runtime = build_chain_runtime_state(&chain, 48_000.0, &[DEFAULT_ELASTIC_TARGET])
        .expect("runtime should build");
    runtime.set_draining();
    assert!(runtime.is_draining(), "set_draining should arm the flag");
    runtime.clear_draining();
    assert!(
        !runtime.is_draining(),
        "clear_draining must reset the flag so a runtime reused across a teardown+rebuild can resume audio (#316)"
    );
}

// ── Empty chain builds successfully ─────────────────────────────────────

#[test]
fn build_chain_runtime_state_empty_chain_succeeds() {
    let chain = Chain {
        id: ChainId("chain:empty".into()),
        description: None,
        instrument: "electric_guitar".to_string(),
        enabled: true,
        blocks: vec![],
    };
    let runtime = build_chain_runtime_state(&chain, 48_000.0, &[DEFAULT_ELASTIC_TARGET]);
    assert!(runtime.is_ok(), "empty chain should build successfully");
}

// ── ElasticBuffer push/pop FIFO order ───────────────────────────────────

#[test]
fn elastic_buffer_fifo_order() {
    let buf = ElasticBuffer::new(256, AudioChannelLayout::Mono);
    for i in 0..10 {
        buf.push(AudioFrame::Mono(i as f32 * 0.1));
    }
    for i in 0..10 {
        let frame = buf.pop();
        let expected = i as f32 * 0.1;
        assert!(matches!(frame, AudioFrame::Mono(v) if (v - expected).abs() < 1e-6),
            "frame {i}: expected {expected}");
    }
}

// ── RuntimeGraph remove non-existent chain ──────────────────────────────

#[test]
fn runtime_graph_remove_nonexistent_chain_no_panic() {
    use super::RuntimeGraph;
    let mut graph = RuntimeGraph { chains: HashMap::new() };
    graph.remove_chain(&ChainId("does_not_exist".into()));
    assert!(graph.chains.is_empty());
}

// ── bypass_runtime_node tests ───────────────────────────────────────────

#[test]
fn bypass_runtime_node_has_bypass_processor() {
    use super::bypass_runtime_node;
    let block = AudioBlock {
        id: BlockId("test:bypass".into()),
        enabled: false,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "gain".into(),
            model: "volume".into(),
            params: ParameterSet::default(),
        }),
    };
    let node = bypass_runtime_node(&block, AudioChannelLayout::Mono);
    assert!(matches!(node.processor, RuntimeProcessor::Bypass));
    assert_eq!(node.block_id.0, "test:bypass");
    assert_eq!(node.input_layout, AudioChannelLayout::Mono);
    assert_eq!(node.output_layout, AudioChannelLayout::Mono);
}

// ── SelectRuntimeState selected_node_mut ────────────────────────────────

#[test]
fn select_runtime_state_finds_selected_option() {
    use super::SelectRuntimeState;
    let mut state = SelectRuntimeState {
        selected_block_id: BlockId("opt:b".into()),
        options: vec![
            counting_block_node(std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0))),
            {
                let mut node = counting_block_node(std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)));
                node.block_id = BlockId("opt:b".into());
                node
            },
        ],
    };
    let found = state.selected_node_mut();
    assert!(found.is_some());
    assert_eq!(found.unwrap().block_id.0, "opt:b");
}

#[test]
fn select_runtime_state_returns_none_when_no_match() {
    use super::SelectRuntimeState;
    let mut state = SelectRuntimeState {
        selected_block_id: BlockId("nonexistent".into()),
        options: vec![
            counting_block_node(std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0))),
        ],
    };
    assert!(state.selected_node_mut().is_none());
}

// ── processor_scratch tests ─────────────────────────────────────────────

#[test]
fn processor_scratch_mono_creates_mono_scratch() {
    use super::processor_scratch;
    struct NoopMono;
    impl block_core::MonoProcessor for NoopMono {
        fn process_sample(&mut self, s: f32) -> f32 { s }
    }
    let proc = AudioProcessor::Mono(Box::new(NoopMono));
    let scratch = processor_scratch(&proc);
    assert!(matches!(scratch, ProcessorScratch::Mono(_)));
}

#[test]
fn processor_scratch_stereo_creates_stereo_scratch() {
    use super::processor_scratch;
    struct NoopStereo;
    impl block_core::StereoProcessor for NoopStereo {
        fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] { input }
    }
    let proc = AudioProcessor::Stereo(Box::new(NoopStereo));
    let scratch = processor_scratch(&proc);
    assert!(matches!(scratch, ProcessorScratch::Stereo(_)));
}

#[test]
fn processor_scratch_dual_mono_creates_dual_mono_scratch() {
    use super::processor_scratch;
    struct NoopMono;
    impl block_core::MonoProcessor for NoopMono {
        fn process_sample(&mut self, s: f32) -> f32 { s }
    }
    let proc = AudioProcessor::DualMono {
        left: Box::new(NoopMono),
        right: Box::new(NoopMono),
    };
    let scratch = processor_scratch(&proc);
    assert!(matches!(scratch, ProcessorScratch::DualMono { .. }));
}
