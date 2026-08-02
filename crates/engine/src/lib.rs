// Snapshot of complexity debt that existed on develop before the
// #548 build break was fixed (issue #576). Refactor of long fns and
// complex types is tracked under god-file ticket #276 and follow-ups.
// Allowing crate-wide keeps the QG honest about NEW regressions
// instead of perpetually re-reporting the existing snapshot.
#![allow(clippy::too_many_lines)]
#![allow(clippy::cognitive_complexity)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::type_complexity)]

pub mod chain_quality;
pub mod di_loop;
pub mod di_output_resolve;
pub mod di_render;
pub mod elastic_prime;
pub mod input_tap;
#[cfg(test)]
#[path = "issue_85_stream_per_pair_tests.rs"]
mod issue_85_stream_per_pair_tests;
pub mod loop_pcm;
pub mod looper;
pub mod looper_bank;
pub mod metronome_state;
pub mod native_registry;
pub mod offline;
pub mod output_meter;
pub mod probe;
pub mod rig_runtime;
mod rig_runtime_normalize;
pub mod runtime;
pub mod runtime_audio_frame;
pub mod runtime_block_builders;
pub mod runtime_block_core;
pub mod runtime_block_toggle;
mod runtime_chain_state;
pub mod runtime_di_seek;
pub mod runtime_dsp;
pub mod runtime_endpoints;
pub mod runtime_graph;
mod runtime_graph_assemble;
mod runtime_graph_impl;
mod runtime_graph_update;
pub mod runtime_io;
pub mod runtime_layout;
pub mod runtime_load;
mod runtime_looper_api;
mod runtime_mid_output_tap;
pub mod runtime_probe;
mod runtime_process_segment;
mod runtime_processor_model;
pub mod runtime_segments;
pub mod runtime_state;
mod runtime_state_taps;
pub mod spsc;
pub mod stream_tap;
pub mod tone_doctor;
pub mod tone_doctor_fix;
pub mod tone_doctor_suggestion;
pub mod tone_profile_table;
pub use di_loop::{DiFrame, DiLoop, DiPcm};
pub use loop_pcm::LoopPcm;
pub use looper::{LooperSlot, LooperSpeed, LooperState, LOOPER_MAX_LAYERS};
pub use looper_bank::{LooperOp, LooperStatus, LOOPER_MAX_PER_CHAIN, LOOPER_MAX_SECONDS};
