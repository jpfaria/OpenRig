//! Per-effect-type dispatch for `CoreBlock` runtime node construction.
//!
//! Lifted out of `runtime_block_builders.rs` (slice 8 of the Phase 2 split)
//! so the parent file gets back under the 600 LOC cap.
//!
//! What lives here: a single function — `build_core_block_runtime_node` —
//! that dispatches on `effect_type` and calls the per-effect-type processor
//! builder from each `block_*` crate (preamp / amp / cab / body / IR /
//! gain / delay / reverb / utility / dynamics / filter / wah / modulation /
//! pitch / VST3). One responsibility: turn a `CoreBlock` into a
//! `BlockRuntimeNode` by routing through the right backend.
//!
//! What's NOT here: setup helpers shared across more than this dispatch
//! (`build_audio_processor_for_model`, `audio_block_runtime_node`,
//! `expect_*_processor`, `required_string`, `optional_string`) stay in
//! `runtime_block_builders.rs` because the Select / NAM / bypass paths
//! also use them.

use anyhow::{anyhow, Result};

use block_amp::build_amp_processor_for_layout;
use block_body::build_body_processor_for_layout;
use block_cab::build_cab_processor_for_layout;
use block_core::AudioChannelLayout;
use block_delay::build_delay_processor_for_layout;
use block_dyn::build_dynamics_processor_for_layout;
use block_filter::build_filter_processor_for_layout;
use block_full_rig::build_full_rig_processor_for_layout;
use block_gain::build_gain_processor_for_layout;
use block_ir::build_ir_processor_for_layout;
use block_mod::build_modulation_processor_for_layout;
use block_pitch::build_pitch_processor_for_layout;
use block_preamp::build_preamp_processor_for_layout;
use block_reverb::build_reverb_processor_for_layout;
use block_util::build_utility_processor_for_layout;
use block_wah::build_wah_processor_for_layout;
use project::block::CoreBlock;
use project::chain::Chain;

use crate::runtime_block_builders::{audio_block_runtime_node, build_audio_processor_for_model};
use crate::runtime_state::BlockRuntimeNode;

pub(crate) fn build_core_block_runtime_node(
    chain: &Chain,
    block: &project::block::AudioBlock,
    core: &CoreBlock,
    input_layout: AudioChannelLayout,
    sample_rate: f32,
) -> Result<BlockRuntimeNode> {
    let effect_type = core.effect_type.as_str();
    let model = &core.model;
    let params = &core.params;

    use block_core::*;
    match effect_type {
        EFFECT_TYPE_PREAMP => Ok(audio_block_runtime_node(
            block,
            input_layout,
            build_audio_processor_for_model(
                chain,
                EFFECT_TYPE_PREAMP,
                model,
                input_layout,
                |layout| build_preamp_processor_for_layout(model, params, sample_rate, layout),
            )?,
        )),
        EFFECT_TYPE_AMP => Ok(audio_block_runtime_node(
            block,
            input_layout,
            build_audio_processor_for_model(
                chain,
                EFFECT_TYPE_AMP,
                model,
                input_layout,
                |layout| build_amp_processor_for_layout(model, params, sample_rate, layout),
            )?,
        )),
        EFFECT_TYPE_FULL_RIG => Ok(audio_block_runtime_node(
            block,
            input_layout,
            build_audio_processor_for_model(
                chain,
                EFFECT_TYPE_FULL_RIG,
                model,
                input_layout,
                |layout| build_full_rig_processor_for_layout(model, params, sample_rate, layout),
            )?,
        )),
        EFFECT_TYPE_CAB => Ok(audio_block_runtime_node(
            block,
            input_layout,
            build_audio_processor_for_model(
                chain,
                EFFECT_TYPE_CAB,
                model,
                input_layout,
                |layout| build_cab_processor_for_layout(model, params, sample_rate, layout),
            )?,
        )),
        EFFECT_TYPE_BODY => Ok(audio_block_runtime_node(
            block,
            input_layout,
            build_audio_processor_for_model(
                chain,
                EFFECT_TYPE_BODY,
                model,
                input_layout,
                |layout| build_body_processor_for_layout(model, params, sample_rate, layout),
            )?,
        )),
        EFFECT_TYPE_IR => Ok(audio_block_runtime_node(
            block,
            input_layout,
            build_audio_processor_for_model(
                chain,
                EFFECT_TYPE_IR,
                model,
                input_layout,
                |layout| build_ir_processor_for_layout(model, params, sample_rate, layout),
            )?,
        )),
        EFFECT_TYPE_GAIN => Ok(audio_block_runtime_node(
            block,
            input_layout,
            build_audio_processor_for_model(
                chain,
                EFFECT_TYPE_GAIN,
                model,
                input_layout,
                |layout| build_gain_processor_for_layout(model, params, sample_rate, layout),
            )?,
        )),
        EFFECT_TYPE_DELAY => Ok(audio_block_runtime_node(
            block,
            input_layout,
            build_audio_processor_for_model(
                chain,
                EFFECT_TYPE_DELAY,
                model,
                input_layout,
                |layout| build_delay_processor_for_layout(model, params, sample_rate, layout),
            )?,
        )),
        EFFECT_TYPE_REVERB => Ok(audio_block_runtime_node(
            block,
            input_layout,
            build_audio_processor_for_model(
                chain,
                EFFECT_TYPE_REVERB,
                model,
                input_layout,
                |layout| build_reverb_processor_for_layout(model, params, sample_rate, layout),
            )?,
        )),
        EFFECT_TYPE_UTILITY => {
            let mut captured_stream: Option<StreamHandle> = None;
            let mut outcome = build_audio_processor_for_model(
                chain,
                EFFECT_TYPE_UTILITY,
                model,
                input_layout,
                |layout| {
                    let (bp, sh) = build_utility_processor_for_layout(
                        model,
                        params,
                        sample_rate.round() as usize,
                        layout,
                    )?;
                    if captured_stream.is_none() {
                        captured_stream = sh;
                    }
                    Ok(bp)
                },
            )?;
            outcome.stream_handle = captured_stream;
            Ok(audio_block_runtime_node(block, input_layout, outcome))
        }
        EFFECT_TYPE_DYNAMICS => Ok(audio_block_runtime_node(
            block,
            input_layout,
            build_audio_processor_for_model(
                chain,
                EFFECT_TYPE_DYNAMICS,
                model,
                input_layout,
                |layout| build_dynamics_processor_for_layout(model, params, sample_rate, layout),
            )?,
        )),
        EFFECT_TYPE_FILTER => Ok(audio_block_runtime_node(
            block,
            input_layout,
            build_audio_processor_for_model(
                chain,
                EFFECT_TYPE_FILTER,
                model,
                input_layout,
                |layout| build_filter_processor_for_layout(model, params, sample_rate, layout),
            )?,
        )),
        EFFECT_TYPE_WAH => Ok(audio_block_runtime_node(
            block,
            input_layout,
            build_audio_processor_for_model(
                chain,
                EFFECT_TYPE_WAH,
                model,
                input_layout,
                |layout| build_wah_processor_for_layout(model, params, sample_rate, layout),
            )?,
        )),
        EFFECT_TYPE_MODULATION => Ok(audio_block_runtime_node(
            block,
            input_layout,
            build_audio_processor_for_model(
                chain,
                EFFECT_TYPE_MODULATION,
                model,
                input_layout,
                |layout| build_modulation_processor_for_layout(model, params, sample_rate, layout),
            )?,
        )),
        EFFECT_TYPE_PITCH => Ok(audio_block_runtime_node(
            block,
            input_layout,
            build_audio_processor_for_model(
                chain,
                EFFECT_TYPE_PITCH,
                model,
                input_layout,
                |layout| build_pitch_processor_for_layout(model, params, sample_rate, layout),
            )?,
        )),
        x if x == block_core::EFFECT_TYPE_VST3 => {
            let entry = vst3_host::find_vst3_plugin(model)
                .ok_or_else(|| anyhow!("VST3 plugin '{}' not found in catalog", model))?;
            let bundle_path = entry.info.bundle_path.clone();
            // Resolve UID lazily if not available from moduleinfo.json.
            let uid = vst3_host::resolve_uid_for_model(model)
                .map_err(|e| anyhow!("VST3 UID resolution failed for '{}': {}", model, e))?;
            // Convert stored params (path="p{id}", value=0–100%) to VST3 normalized pairs.
            let vst3_params: Vec<(u32, f64)> = params
                .values
                .iter()
                .filter_map(|(path, value)| {
                    let id_str = path.strip_prefix('p')?;
                    let id: u32 = id_str.parse().ok()?;
                    let pct = value.as_f32()?;
                    Some((id, (pct / 100.0).clamp(0.0, 1.0) as f64))
                })
                .collect();
            // Load the plugin once so we can extract the controller and library
            // Arc before building the processor. This allows the GUI to reuse
            // the same IEditController instead of creating a second instance
            // (which fails for plugins like ValhallaSupermassive).
            const VST3_BLOCK_SIZE: usize = 512;
            let plugin = vst3_host::Vst3Plugin::load(
                &bundle_path,
                &uid,
                sample_rate as f64,
                2,
                VST3_BLOCK_SIZE,
                &vst3_params,
            )
            .map_err(|e| anyhow!("VST3 load failed for '{}': {}", model, e))?;
            // Register GUI context: shared controller + library Arc + param channel.
            let param_channel = vst3_host::register_vst3_gui_context(
                model,
                plugin.controller_clone(),
                plugin.library_arc(),
            );
            // Wrap in Option so we can move the plugin out of the FnMut closure
            // (VST3 MonoToStereo schema guarantees the closure is called exactly once).
            let mut plugin_opt = Some(plugin);
            Ok(audio_block_runtime_node(
                block,
                input_layout,
                build_audio_processor_for_model(
                    chain,
                    block_core::EFFECT_TYPE_VST3,
                    model,
                    input_layout,
                    |layout| {
                        let p = plugin_opt
                            .take()
                            .ok_or_else(|| anyhow!("VST3 plugin consumed twice"))?;
                        Ok(vst3_host::build_vst3_processor_from_plugin(
                            p,
                            layout,
                            param_channel.clone(),
                        ))
                    },
                )?,
            ))
        }
        other => Err(anyhow!("unsupported core block effect_type '{}'", other)),
    }
}
