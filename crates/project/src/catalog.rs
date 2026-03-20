use crate::block::{
    build_audio_block_kind, schema_for_block_model, AudioBlockKind,
};
use crate::param::ParameterSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockTypeCatalogEntry {
    pub effect_type: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockModelCatalogEntry {
    pub effect_type: String,
    pub model_id: String,
    pub display_name: String,
}

type SupportedModelsFn = fn() -> &'static [&'static str];

#[derive(Clone, Copy)]
struct BlockRegistryEntry {
    effect_type: &'static str,
    supported_models: SupportedModelsFn,
}

fn block_registry() -> [BlockRegistryEntry; 13] {
    [
        BlockRegistryEntry {
            effect_type: "amp_head",
            supported_models: stage_amp_head::supported_models,
        },
        BlockRegistryEntry {
            effect_type: "amp_combo",
            supported_models: stage_amp_combo::supported_models,
        },
        BlockRegistryEntry {
            effect_type: "cab",
            supported_models: stage_cab::supported_models,
        },
        BlockRegistryEntry {
            effect_type: "full_rig",
            supported_models: stage_full_rig::supported_models,
        },
        BlockRegistryEntry {
            effect_type: "drive",
            supported_models: stage_gain::supported_models,
        },
        BlockRegistryEntry {
            effect_type: "compressor",
            supported_models: stage_dyn::compressor_supported_models,
        },
        BlockRegistryEntry {
            effect_type: "gate",
            supported_models: stage_dyn::gate_supported_models,
        },
        BlockRegistryEntry {
            effect_type: "eq",
            supported_models: stage_filter::supported_models,
        },
        BlockRegistryEntry {
            effect_type: "tremolo",
            supported_models: stage_mod::supported_models,
        },
        BlockRegistryEntry {
            effect_type: "delay",
            supported_models: stage_delay::supported_models,
        },
        BlockRegistryEntry {
            effect_type: "reverb",
            supported_models: stage_reverb::supported_models,
        },
        BlockRegistryEntry {
            effect_type: "tuner",
            supported_models: stage_util::supported_models,
        },
        BlockRegistryEntry {
            effect_type: "nam",
            supported_models: stage_nam::supported_models,
        },
    ]
}

pub fn supported_block_types() -> Vec<BlockTypeCatalogEntry> {
    block_registry()
        .into_iter()
        .filter(|entry| !(entry.supported_models)().is_empty())
        .map(|entry| BlockTypeCatalogEntry {
            effect_type: entry.effect_type,
        })
        .collect()
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
            Ok(BlockModelCatalogEntry {
                effect_type: effect_type.to_string(),
                model_id: (*model_id).to_string(),
                display_name: schema.display_name,
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

        assert!(effect_types.contains(&"amp_head"));
        assert!(effect_types.contains(&"delay"));
        assert!(effect_types.contains(&"nam"));
    }

    #[test]
    fn catalog_exposes_native_models_from_core() {
        let amp_models = supported_block_models("amp_head").expect("amp head catalog");
        let ids = amp_models
            .into_iter()
            .map(|entry| entry.model_id)
            .collect::<Vec<_>>();

        assert!(ids.contains(&"marshall_jcm_800_2203".to_string()));
        assert!(ids.contains(&"brit_crunch_head".to_string()));
        assert!(ids.contains(&"american_clean_head".to_string()));
        assert!(ids.contains(&"modern_high_gain_head".to_string()));
    }
}
