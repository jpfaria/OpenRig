//! The static block-type registry table (issue #792 split from `catalog.rs`).
//!
//! One `BlockRegistryEntry` per native block-* crate, wiring each effect type
//! to its `supported_models` / `model_visual` / `model_color_override`
//! functions. `catalog.rs` holds the query functions that walk this table.

use block_core::{ModelColorOverride, ModelVisualData};

/// Used in the BlockRegistryEntry rows for block crates that have no
/// native model overrides (block-body, block-full-rig, block-gain,
/// block-ir, block-nam, block-pitch, block-util). Returning `None`
/// here makes the resolution fall through to the brand colors only.
fn no_color_override(_: &str) -> Option<ModelColorOverride> {
    None
}

type SupportedModelsFn = fn() -> &'static [&'static str];
type ModelVisualFn = fn(&str) -> Option<ModelVisualData>;
type ModelColorOverrideFn = fn(&str) -> Option<ModelColorOverride>;

#[derive(Clone, Copy)]
pub(crate) struct BlockRegistryEntry {
    pub(crate) effect_type: &'static str,
    pub(crate) display_label: &'static str,
    pub(crate) icon_kind: &'static str,
    pub(crate) use_panel_editor: bool,
    pub(crate) supported_models: SupportedModelsFn,
    pub(crate) model_visual: ModelVisualFn,
    pub(crate) model_color_override: ModelColorOverrideFn,
}

pub(crate) fn block_registry() -> [BlockRegistryEntry; 16] {
    use block_core::*;
    [
        BlockRegistryEntry {
            effect_type: EFFECT_TYPE_PREAMP,
            display_label: "PREAMP",
            icon_kind: EFFECT_TYPE_PREAMP,
            use_panel_editor: true,
            supported_models: block_preamp::supported_models,
            model_visual: block_preamp::preamp_model_visual,
            model_color_override: block_preamp::model_visual::model_color_override,
        },
        BlockRegistryEntry {
            effect_type: EFFECT_TYPE_AMP,
            display_label: "AMP",
            icon_kind: EFFECT_TYPE_AMP,
            use_panel_editor: true,
            supported_models: block_amp::supported_models,
            model_visual: block_amp::amp_model_visual,
            model_color_override: block_amp::model_visual::model_color_override,
        },
        BlockRegistryEntry {
            effect_type: EFFECT_TYPE_CAB,
            display_label: "CAB",
            icon_kind: EFFECT_TYPE_CAB,
            use_panel_editor: true,
            supported_models: block_cab::supported_models,
            model_visual: block_cab::cab_model_visual,
            model_color_override: block_cab::model_visual::model_color_override,
        },
        BlockRegistryEntry {
            effect_type: EFFECT_TYPE_BODY,
            display_label: "BODY",
            icon_kind: "body",
            use_panel_editor: true,
            supported_models: block_body::supported_models,
            model_visual: block_body::body_model_visual,
            model_color_override: no_color_override,
        },
        BlockRegistryEntry {
            effect_type: EFFECT_TYPE_IR,
            display_label: "IR",
            icon_kind: EFFECT_TYPE_IR,
            use_panel_editor: true,
            supported_models: block_ir::supported_models,
            model_visual: block_ir::ir_model_visual,
            model_color_override: no_color_override,
        },
        BlockRegistryEntry {
            effect_type: EFFECT_TYPE_FULL_RIG,
            display_label: "RIG",
            icon_kind: EFFECT_TYPE_FULL_RIG,
            use_panel_editor: true,
            supported_models: block_full_rig::supported_models,
            model_visual: block_full_rig::full_rig_model_visual,
            model_color_override: no_color_override,
        },
        BlockRegistryEntry {
            effect_type: EFFECT_TYPE_GAIN,
            display_label: "GAIN",
            icon_kind: EFFECT_TYPE_GAIN,
            use_panel_editor: true,
            supported_models: block_gain::supported_models,
            model_visual: block_gain::gain_model_visual,
            model_color_override: no_color_override,
        },
        BlockRegistryEntry {
            effect_type: EFFECT_TYPE_DYNAMICS,
            display_label: "DYN",
            icon_kind: EFFECT_TYPE_DYNAMICS,
            use_panel_editor: true,
            supported_models: block_dyn::supported_models,
            model_visual: block_dyn::dyn_model_visual,
            model_color_override: block_dyn::model_visual::model_color_override,
        },
        BlockRegistryEntry {
            effect_type: EFFECT_TYPE_FILTER,
            display_label: "FILTER",
            icon_kind: EFFECT_TYPE_FILTER,
            use_panel_editor: true,
            supported_models: block_filter::supported_models,
            model_visual: block_filter::filter_model_visual,
            model_color_override: block_filter::model_visual::model_color_override,
        },
        BlockRegistryEntry {
            effect_type: EFFECT_TYPE_WAH,
            display_label: "WAH",
            icon_kind: EFFECT_TYPE_WAH,
            use_panel_editor: true,
            supported_models: block_wah::supported_models,
            model_visual: block_wah::wah_model_visual,
            model_color_override: block_wah::model_visual::model_color_override,
        },
        BlockRegistryEntry {
            effect_type: EFFECT_TYPE_PITCH,
            display_label: "PITCH",
            icon_kind: EFFECT_TYPE_PITCH,
            use_panel_editor: true,
            supported_models: block_pitch::supported_models,
            model_visual: block_pitch::pitch_model_visual,
            model_color_override: no_color_override,
        },
        BlockRegistryEntry {
            effect_type: EFFECT_TYPE_MODULATION,
            display_label: "MOD",
            icon_kind: EFFECT_TYPE_MODULATION,
            use_panel_editor: true,
            supported_models: block_mod::supported_models,
            model_visual: block_mod::mod_model_visual,
            model_color_override: block_mod::model_visual::model_color_override,
        },
        BlockRegistryEntry {
            effect_type: EFFECT_TYPE_DELAY,
            display_label: "DLY",
            icon_kind: EFFECT_TYPE_DELAY,
            use_panel_editor: true,
            supported_models: block_delay::supported_models,
            model_visual: block_delay::delay_model_visual,
            model_color_override: block_delay::model_visual::model_color_override,
        },
        BlockRegistryEntry {
            effect_type: EFFECT_TYPE_REVERB,
            display_label: "RVB",
            icon_kind: EFFECT_TYPE_REVERB,
            use_panel_editor: true,
            supported_models: block_reverb::supported_models,
            model_visual: block_reverb::reverb_model_visual,
            model_color_override: block_reverb::model_visual::model_color_override,
        },
        BlockRegistryEntry {
            effect_type: EFFECT_TYPE_UTILITY,
            display_label: "UTIL",
            icon_kind: EFFECT_TYPE_UTILITY,
            use_panel_editor: true,
            supported_models: block_util::supported_models,
            model_visual: block_util::util_model_visual,
            model_color_override: no_color_override,
        },
        BlockRegistryEntry {
            effect_type: EFFECT_TYPE_NAM,
            display_label: "NAM",
            icon_kind: EFFECT_TYPE_NAM,
            use_panel_editor: true,
            supported_models: block_nam::supported_models,
            model_visual: block_nam::nam_model_visual,
            model_color_override: no_color_override,
        },
    ]
}
