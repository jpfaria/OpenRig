use crate::block::{
    build_audio_block_kind, schema_for_block_model, AudioBlockKind,
};
use crate::param::ParameterSet;
use block_core::ModelVisualData;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockTypeCatalogEntry {
    pub effect_type: &'static str,
    pub display_label: &'static str,
    pub icon_kind: &'static str,
    pub use_panel_editor: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockModelCatalogEntry {
    pub effect_type: String,
    pub model_id: String,
    pub display_name: String,
    pub brand: String,
    pub type_label: String,
    pub panel_bg: [u8; 3],
    pub panel_text: [u8; 3],
    pub brand_strip_bg: [u8; 3],
    pub model_font: String,
}

type SupportedModelsFn = fn() -> &'static [&'static str];
type ModelVisualFn = fn(&str) -> Option<ModelVisualData>;

#[derive(Clone, Copy)]
struct BlockRegistryEntry {
    effect_type: &'static str,
    display_label: &'static str,
    icon_kind: &'static str,
    use_panel_editor: bool,
    supported_models: SupportedModelsFn,
    model_visual: ModelVisualFn,
}

fn block_registry() -> [BlockRegistryEntry; 15] {
    [
        BlockRegistryEntry {
            effect_type: "preamp",
            display_label: "PREAMP",
            icon_kind: "preamp",
            use_panel_editor: true,
            supported_models: block_preamp::supported_models,
            model_visual: block_preamp::preamp_model_visual,
        },
        BlockRegistryEntry {
            effect_type: "amp",
            display_label: "AMP",
            icon_kind: "amp",
            use_panel_editor: true,
            supported_models: block_amp::supported_models,
            model_visual: block_amp::amp_model_visual,
        },
        BlockRegistryEntry {
            effect_type: "cab",
            display_label: "CAB",
            icon_kind: "cab",
            use_panel_editor: false,
            supported_models: block_cab::supported_models,
            model_visual: block_cab::cab_model_visual,
        },
        BlockRegistryEntry {
            effect_type: "ir",
            display_label: "IR",
            icon_kind: "ir",
            use_panel_editor: false,
            supported_models: block_ir::supported_models,
            model_visual: block_ir::ir_model_visual,
        },
        BlockRegistryEntry {
            effect_type: "full_rig",
            display_label: "RIG",
            icon_kind: "full_rig",
            use_panel_editor: false,
            supported_models: block_full_rig::supported_models,
            model_visual: block_full_rig::full_rig_model_visual,
        },
        BlockRegistryEntry {
            effect_type: "gain",
            display_label: "GAIN",
            icon_kind: "gain",
            use_panel_editor: false,
            supported_models: block_gain::supported_models,
            model_visual: block_gain::gain_model_visual,
        },
        BlockRegistryEntry {
            effect_type: "dynamics",
            display_label: "DYN",
            icon_kind: "dynamics",
            use_panel_editor: false,
            supported_models: block_dyn::supported_models,
            model_visual: block_dyn::dyn_model_visual,
        },
        BlockRegistryEntry {
            effect_type: "filter",
            display_label: "FILTER",
            icon_kind: "filter",
            use_panel_editor: false,
            supported_models: block_filter::supported_models,
            model_visual: block_filter::filter_model_visual,
        },
        BlockRegistryEntry {
            effect_type: "wah",
            display_label: "WAH",
            icon_kind: "wah",
            use_panel_editor: false,
            supported_models: block_wah::supported_models,
            model_visual: block_wah::wah_model_visual,
        },
        BlockRegistryEntry {
            effect_type: "pitch",
            display_label: "PITCH",
            icon_kind: "pitch",
            use_panel_editor: false,
            supported_models: block_pitch::supported_models,
            model_visual: block_pitch::pitch_model_visual,
        },
        BlockRegistryEntry {
            effect_type: "modulation",
            display_label: "MOD",
            icon_kind: "modulation",
            use_panel_editor: false,
            supported_models: block_mod::supported_models,
            model_visual: block_mod::mod_model_visual,
        },
        BlockRegistryEntry {
            effect_type: "delay",
            display_label: "DLY",
            icon_kind: "delay",
            use_panel_editor: false,
            supported_models: block_delay::supported_models,
            model_visual: block_delay::delay_model_visual,
        },
        BlockRegistryEntry {
            effect_type: "reverb",
            display_label: "RVB",
            icon_kind: "reverb",
            use_panel_editor: false,
            supported_models: block_reverb::supported_models,
            model_visual: block_reverb::reverb_model_visual,
        },
        BlockRegistryEntry {
            effect_type: "utility",
            display_label: "UTIL",
            icon_kind: "utility",
            use_panel_editor: false,
            supported_models: block_util::supported_models,
            model_visual: block_util::util_model_visual,
        },
        BlockRegistryEntry {
            effect_type: "nam",
            display_label: "NAM",
            icon_kind: "nam",
            use_panel_editor: false,
            supported_models: block_nam::supported_models,
            model_visual: block_nam::nam_model_visual,
        },
    ]
}

pub fn supported_block_types() -> Vec<BlockTypeCatalogEntry> {
    block_registry()
        .into_iter()
        .filter(|entry| !(entry.supported_models)().is_empty())
        .map(|entry| BlockTypeCatalogEntry {
            effect_type: entry.effect_type,
            display_label: entry.display_label,
            icon_kind: entry.icon_kind,
            use_panel_editor: entry.use_panel_editor,
        })
        .collect()
}

pub fn supported_block_type(effect_type: &str) -> Option<BlockTypeCatalogEntry> {
    block_registry()
        .into_iter()
        .find(|entry| entry.effect_type == effect_type)
        .map(|entry| BlockTypeCatalogEntry {
            effect_type: entry.effect_type,
            display_label: entry.display_label,
            icon_kind: entry.icon_kind,
            use_panel_editor: entry.use_panel_editor,
        })
}

pub fn supported_block_models(effect_type: &str) -> Result<Vec<BlockModelCatalogEntry>, String> {
    let entry = block_registry()
        .into_iter()
        .find(|entry| entry.effect_type == effect_type)
        .ok_or_else(|| format!("unsupported effect type '{}'", effect_type))?;

    (entry.supported_models)()
        .iter()
        .map(|model_id| {
            let schema = schema_for_block_model(effect_type, model_id)?;
            let visual = (entry.model_visual)(model_id);
            Ok(BlockModelCatalogEntry {
                effect_type: effect_type.to_string(),
                model_id: (*model_id).to_string(),
                display_name: schema.display_name,
                brand: visual.as_ref().map(|v| v.brand.to_string()).unwrap_or_default(),
                type_label: visual.as_ref().map(|v| v.type_label.to_string()).unwrap_or_default(),
                panel_bg: visual.as_ref().map(|v| v.panel_bg).unwrap_or([0x2c, 0x2e, 0x34]),
                panel_text: visual.as_ref().map(|v| v.panel_text).unwrap_or([0x80, 0x90, 0xa0]),
                brand_strip_bg: visual.as_ref().map(|v| v.brand_strip_bg).unwrap_or([0x1a, 0x1a, 0x1a]),
                model_font: visual.as_ref().map(|v| v.model_font.to_string()).unwrap_or_default(),
            })
        })
        .collect()
}

pub fn build_block_kind(
    effect_type: &str,
    model_id: &str,
    params: ParameterSet,
) -> Result<AudioBlockKind, String> {
    build_audio_block_kind(effect_type, model_id, params)
}

#[cfg(test)]
mod tests {
    use super::{supported_block_models, supported_block_types};

    #[test]
    fn catalog_exposes_supported_types() {
        let effect_types = supported_block_types()
            .into_iter()
            .map(|entry| entry.effect_type)
            .collect::<Vec<_>>();

        assert!(effect_types.contains(&"preamp"));
        assert!(effect_types.contains(&"delay"));
        assert!(effect_types.contains(&"nam"));
        assert!(effect_types.contains(&"ir"));
        assert!(effect_types.contains(&"wah"));
        assert!(effect_types.contains(&"pitch"));
    }

    #[test]
    fn catalog_mirrors_core_supported_models() {
        let amp_model_ids = supported_block_models("preamp")
            .expect("preamp catalog")
            .into_iter()
            .map(|entry| entry.model_id)
            .collect::<Vec<_>>();
        let expected = block_preamp::supported_models()
            .iter()
            .map(|model| (*model).to_string())
            .collect::<Vec<_>>();

        assert_eq!(amp_model_ids, expected);

        let delay_model_ids = supported_block_models("delay")
            .expect("delay catalog")
            .into_iter()
            .map(|entry| entry.model_id)
            .collect::<Vec<_>>();
        let expected = block_delay::supported_models()
            .iter()
            .map(|model| (*model).to_string())
            .collect::<Vec<_>>();

        assert_eq!(delay_model_ids, expected);
    }
}
