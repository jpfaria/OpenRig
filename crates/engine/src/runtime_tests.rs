//! Unit tests for the engine runtime — input/output processing, chain graph
//! construction, and per-block runtime node assembly.
//!
//! Lifted out of `runtime.rs` so the production-side file (~2.6k LOC) stays
//! readable alongside its test surface (also ~2.6k LOC). All tests live
//! under `mod tests` of the runtime crate root via `#[path]`, so every
//! `super::xxx` reference (private types, helper fns) keeps resolving
//! unchanged.
//!
//! This file is the shared fixtures/re-export hub for the 7 sibling
//! `runtime_*_tests.rs` concern files (#792 split). Which re-exports each
//! sibling consumes varies by build cfg (e.g. the `not(linux+jack)`
//! same-device isolation tests), so some re-exports are legitimately unused
//! in any single configuration — allow that here rather than churn the list.
#![allow(unused_imports)]

pub(super) use super::{
    apply_block_processor, build_chain_runtime_state, build_runtime_graph, effective_inputs,
    effective_outputs, process_audio_block, process_input_f32, process_output_f32,
    split_chain_into_segments, update_chain_runtime_state, AudioFrame, AudioProcessor, BlockError,
    BlockRuntimeNode, ElasticBuffer, FadeState, ProcessorScratch, RuntimeProcessor,
    DEFAULT_ELASTIC_TARGET, ERROR_QUEUE_CAPACITY, FADE_IN_FRAMES,
};
pub(super) use crate::runtime_endpoints::{InputEntry, OutputEntry};
pub(super) use block_cab::{cab_backend_kind, supported_models as supported_cab_models, CabBackendKind};
pub(super) use block_core::AudioChannelLayout;
pub(super) use block_delay::supported_models as supported_delay_models;
pub(super) use block_dyn::compressor_supported_models;
pub(super) use block_preamp::supported_models as supported_preamp_models;
pub(super) use block_reverb::supported_models as supported_reverb_models;
pub(super) use crossbeam_queue::ArrayQueue;
pub(super) use domain::ids::{BlockId, ChainId, DeviceId};
pub(super) use domain::value_objects::ParameterValue;
pub(super) use project::block::{
    schema_for_block_model, AudioBlock, AudioBlockKind, CoreBlock, InsertBlock, SelectBlock,
};
pub(super) use project::chain::{Chain, ChainInputMode, ChainOutputMode};
pub(super) use project::param::ParameterSet;
pub(super) use project::project::Project;
pub(super) use std::collections::HashMap;
pub(super) use std::sync::Arc;

// ── Model-A I/O binding registries (#716) ─────────────────────────────────
// A chain no longer embeds device endpoints; head input + tail output are
// resolved from the per-machine registry. These helpers mirror the device /
// channels / mode the tests previously declared inline as block `entries`.

/// Registry id every helper chain in this file references via
/// `io_binding_ids: vec!["io".into()]`.
pub(super) const IO_BINDING_ID: &str = "io";

/// A bare chain with no blocks and no binding selection. Used by the
/// `effective_inputs`/`effective_outputs` unit tests, which feed the resolved
/// endpoint slice directly (the binding registry's resolved view).
pub(super) fn empty_chain(id: &str) -> Chain {
    Chain {
        id: ChainId(id.into()),
        description: None,
        instrument: "electric_guitar".to_string(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks: vec![],
        di_output: None,
        loopers: vec![],
    }
}

/// Mono input (ch0) + mono output (ch0) — mirrors `io_passthrough_chain`.
pub(super) fn io_registry_mono() -> Vec<domain::io_binding::IoBinding> {
    vec![domain::io_binding::IoBinding {
        id: IO_BINDING_ID.into(),
        name: "IO".into(),
        inputs: vec![domain::io_binding::IoEndpoint {
            name: "in0".into(),
            device_id: DeviceId("dev".into()),
            mode: domain::io_binding::ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs: vec![domain::io_binding::IoEndpoint {
            name: "out0".into(),
            device_id: DeviceId("dev".into()),
            mode: domain::io_binding::ChannelMode::Mono,
            channels: vec![0],
        }],
    }]
}

/// Split-mono input (one Mono endpoint spanning ch0,1) + stereo output —
/// mirrors the legacy dual-mono isolation chains (`mode: Mono, channels [0,1]`).
pub(super) fn io_registry_split_dual() -> Vec<domain::io_binding::IoBinding> {
    vec![domain::io_binding::IoBinding {
        id: IO_BINDING_ID.into(),
        name: "IO".into(),
        inputs: vec![domain::io_binding::IoEndpoint {
            name: "in0".into(),
            device_id: DeviceId("input-device".into()),
            mode: domain::io_binding::ChannelMode::Mono,
            channels: vec![0, 1],
        }],
        outputs: vec![domain::io_binding::IoEndpoint {
            name: "out0".into(),
            device_id: DeviceId("output-device".into()),
            mode: domain::io_binding::ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    }]
}

/// Two distinct-device mono inputs (scarlett + teyun) + a stereo output —
/// the "two guitars" rig that yields two isolated per-input runtimes.
pub(super) fn io_registry_two_device() -> Vec<domain::io_binding::IoBinding> {
    vec![domain::io_binding::IoBinding {
        id: IO_BINDING_ID.into(),
        name: "IO".into(),
        inputs: vec![
            domain::io_binding::IoEndpoint {
                name: "in0".into(),
                device_id: DeviceId("scarlett".into()),
                mode: domain::io_binding::ChannelMode::Mono,
                channels: vec![0],
            },
            domain::io_binding::IoEndpoint {
                name: "in1".into(),
                device_id: DeviceId("teyun".into()),
                mode: domain::io_binding::ChannelMode::Mono,
                channels: vec![0],
            },
        ],
        outputs: vec![domain::io_binding::IoEndpoint {
            name: "out0".into(),
            device_id: DeviceId("scarlett".into()),
            mode: domain::io_binding::ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    }]
}

/// Stereo input (ch0,1) + stereo output (ch0,1).
pub(super) fn io_registry_stereo() -> Vec<domain::io_binding::IoBinding> {
    vec![domain::io_binding::IoBinding {
        id: IO_BINDING_ID.into(),
        name: "IO".into(),
        inputs: vec![domain::io_binding::IoEndpoint {
            name: "in0".into(),
            device_id: DeviceId("dev".into()),
            mode: domain::io_binding::ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
        outputs: vec![domain::io_binding::IoEndpoint {
            name: "out0".into(),
            device_id: DeviceId("dev".into()),
            mode: domain::io_binding::ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    }]
}



pub(super) fn tuner_track(chain_id: &str, blocks: Vec<AudioBlock>) -> Chain {
    Chain {
        id: ChainId(chain_id.into()),
        description: Some("Tuner chain".into()),
        instrument: "electric_guitar".to_string(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks,
        di_output: None,
        loopers: vec![],
    }
}


/// Test helper — builds a generic processing block. Originally backed by
/// the (now removed) `chromatic_tuner` / `spectrum_analyzer` utility
/// blocks, but those were promoted to top-bar features (#319, #320).
/// We now back it with a delay block so the tests still have a real
/// `BlockProcessor` in their chains; `reference_hz` is preserved as a
/// dummy `time_ms` for the delay so unique values per block survive
/// the rename.
pub(super) fn tuner_block(block_id: &str, reference_hz: f32) -> AudioBlock {
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


pub(super) fn any_ir_cab_defaults() -> (String, ParameterSet) {
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


pub(super) fn normalized_defaults(effect_type: &str, model: &str) -> ParameterSet {
    let schema =
        schema_for_block_model(effect_type, model).expect("schema should exist for test model");
    ParameterSet::default()
        .normalized_against(&schema)
        .expect("defaults should normalize")
}


pub(super) fn compressor_block(block_id: &str) -> AudioBlock {
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


pub(super) fn native_cab_block(block_id: &str) -> AudioBlock {
    let model = supported_cab_models()
        .iter()
        .find(|model| {
            matches!(
                cab_backend_kind(model).expect("cab backend"),
                CabBackendKind::Native
            )
        })
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


pub(super) fn preamp_block(block_id: &str) -> AudioBlock {
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


pub(super) fn marshall_preamp_block(block_id: &str) -> AudioBlock {
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


pub(super) fn ir_cab_block(block_id: &str) -> AudioBlock {
    let model = supported_cab_models()
        .iter()
        .find(|model| {
            matches!(
                cab_backend_kind(model).expect("cab backend"),
                CabBackendKind::Ir
            )
        })
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


pub(super) fn reverb_block(block_id: &str) -> AudioBlock {
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


/// A chain with proper Input and Output blocks but no effect blocks.
/// Useful for testing process_input_f32 / process_output_f32.
pub(super) fn io_passthrough_chain(id: &str) -> Chain {
    Chain {
        id: ChainId(id.into()),
        description: Some("Passthrough".into()),
        instrument: "electric_guitar".to_string(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec![IO_BINDING_ID.into()],
        blocks: vec![],
        di_output: None,
        loopers: vec![],
    }
}


pub(super) fn select_delay_chain(id: &str, selected_option: &str) -> Chain {
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
        volume: 100.0,
        io_binding_ids: vec![],
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
        di_output: None,
        loopers: vec![],
    }
}


pub(super) fn delay_block(id: impl Into<String>, model: &str, time_ms: f32) -> AudioBlock {
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


// ── Panic recovery tests ──────────────────────────────────────────────────

pub(super) struct PanickingProcessor;

impl block_core::MonoProcessor for PanickingProcessor {
    fn process_sample(&mut self, _: f32) -> f32 {
        panic!("simulated plugin crash");
    }
}


pub(super) struct CountingProcessor {
    call_count: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl block_core::MonoProcessor for CountingProcessor {
    fn process_sample(&mut self, input: f32) -> f32 {
        self.call_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        input
    }
}


pub(super) fn panicking_block_node() -> BlockRuntimeNode {
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
        content_mono: true,
        output_layout: block_core::AudioChannelLayout::Mono,
        scratch: ProcessorScratch::Mono(Vec::new()),
        processor: RuntimeProcessor::Audio(AudioProcessor::Mono(Box::new(PanickingProcessor))),
        stream_handle: None,
        fade_state: FadeState::Active,
        fade_dry_buffer: Vec::new(),
        faulted: false,
        fault_reason: None,
    }
}


pub(super) fn counting_block_node(
    counter: std::sync::Arc<std::sync::atomic::AtomicUsize>,
) -> BlockRuntimeNode {
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
        content_mono: true,
        output_layout: block_core::AudioChannelLayout::Mono,
        scratch: ProcessorScratch::Mono(Vec::new()),
        processor: RuntimeProcessor::Audio(AudioProcessor::Mono(Box::new(CountingProcessor {
            call_count: counter,
        }))),
        stream_handle: None,
        fade_state: FadeState::Active,
        fade_dry_buffer: Vec::new(),
        faulted: false,
        fault_reason: None,
    }
}


// ── effective_inputs / effective_outputs with Insert blocks ───────────────

/// Registry for `insert_chain`: mono input `in0` (dev_in) + stereo output
/// `out0` (dev_out). The chain references these by endpoint name from
/// in-position Input/Output blocks so the Insert-segmentation block indices
/// stay identical to the legacy layout. The `"fx"` binding mirrors the legacy
/// inline insert send/return (#716, model A): send = the binding's stereo
/// OUTPUT (send_dev), return = its stereo INPUT (return_dev).
pub(super) fn insert_registry() -> Vec<domain::io_binding::IoBinding> {
    vec![
        domain::io_binding::IoBinding {
            id: IO_BINDING_ID.into(),
            name: "IO".into(),
            inputs: vec![domain::io_binding::IoEndpoint {
                name: "in0".into(),
                device_id: DeviceId("dev_in".into()),
                mode: domain::io_binding::ChannelMode::Mono,
                channels: vec![0],
            }],
            outputs: vec![domain::io_binding::IoEndpoint {
                name: "out0".into(),
                device_id: DeviceId("dev_out".into()),
                mode: domain::io_binding::ChannelMode::Stereo,
                channels: vec![0, 1],
            }],
        },
        domain::io_binding::IoBinding {
            id: "fx".into(),
            name: "FX".into(),
            inputs: vec![domain::io_binding::IoEndpoint {
                name: "ret".into(),
                device_id: DeviceId("return_dev".into()),
                mode: domain::io_binding::ChannelMode::Stereo,
                channels: vec![0, 1],
            }],
            outputs: vec![domain::io_binding::IoEndpoint {
                name: "snd".into(),
                device_id: DeviceId("send_dev".into()),
                mode: domain::io_binding::ChannelMode::Stereo,
                channels: vec![0, 1],
            }],
        },
    ]
}


pub(super) fn insert_chain() -> Chain {
    Chain {
        id: ChainId("chain:insert".into()),
        description: None,
        instrument: "electric_guitar".to_string(),
        enabled: true,
        volume: 100.0,
        // Head/tail resolve from the in-chain Input/Output blocks below
        // (which reference `insert_registry()` by endpoint name), so the
        // block layout — and the Insert-split indices — is unchanged.
        io_binding_ids: vec![],
        blocks: vec![
            AudioBlock {
                id: BlockId("input:0".into()),
                enabled: true,
                kind: AudioBlockKind::Input(project::block::InputBlock {
                    model: "standard".into(),
                    io: IO_BINDING_ID.into(),
                    endpoint: "in0".into(),
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
                    io: "fx".into(),
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
                kind: AudioBlockKind::Output(project::block::OutputBlock {
                    model: "standard".into(),
                    io: IO_BINDING_ID.into(),
                    endpoint: "out0".into(),
                }),
            },
        ],
        di_output: None,
        loopers: vec![],
    }
}


// ── insert_return_as_input_entry tests ──────────────────────────────────

/// Build a one-binding `fx` registry mirroring the legacy inline insert
/// send/return — model A (#716): send = the binding's OUTPUT, return = its
/// INPUT. `(send_mode, send_channels)` and `(ret_mode, ret_channels)` map to
/// the corresponding endpoint.
pub(super) fn fx_registry(
    send_mode: domain::io_binding::ChannelMode,
    send_channels: Vec<usize>,
    ret_mode: domain::io_binding::ChannelMode,
    ret_channels: Vec<usize>,
) -> Vec<domain::io_binding::IoBinding> {
    vec![domain::io_binding::IoBinding {
        id: "fx".into(),
        name: "FX".into(),
        inputs: vec![domain::io_binding::IoEndpoint {
            name: "ret".into(),
            device_id: DeviceId("return_dev".into()),
            mode: ret_mode,
            channels: ret_channels,
        }],
        outputs: vec![domain::io_binding::IoEndpoint {
            name: "snd".into(),
            device_id: DeviceId("send_dev".into()),
            mode: send_mode,
            channels: send_channels,
        }],
    }]
}


pub(super) fn fx_insert() -> InsertBlock {
    InsertBlock {
        model: "external_loop".into(),
        io: "fx".into(),
    }
}
