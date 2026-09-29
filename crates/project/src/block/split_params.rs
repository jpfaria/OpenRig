//! Responsibility: declares the knob schema of a split block.
//!
//! #328 (spec §1.2): the knobs of the Ampero II split node and mixer node.
//! They live in `SplitBlock.params` and are edited through the ordinary
//! `SetBlockParameter*` commands, so MIDI mapping, scenes and MCP reach them
//! with no split-specific path. Defaults are the Ampero defaults. A split is
//! not a catalog effect type: this schema never reaches the model registry.

use block_core::ModelAudioMode;
use domain::ids::BlockId;
use domain::value_objects::ParameterValue;

use crate::param::{
    bool_parameter, enum_parameter, float_parameter, BlockParameterDescriptor, MaterializeContext,
    ModelParameterSchema, ParameterSet, ParameterSpec, ParameterUnit,
};

pub const SPLIT_MODE: &str = "split_mode";
pub const LEVEL_TO_A: &str = "level_to_a";
pub const LEVEL_TO_B: &str = "level_to_b";
pub const BALANCE_A: &str = "balance_a";
pub const BALANCE_B: &str = "balance_b";
pub const MIX_LEVEL_A: &str = "mix_level_a";
pub const MIX_LEVEL_B: &str = "mix_level_b";
pub const MIX_PAN_A: &str = "mix_pan_a";
pub const MIX_PAN_B: &str = "mix_pan_b";
pub const MIX_B_POLARITY: &str = "mix_b_polarity";
pub const MIX_MASTER: &str = "mix_master";
pub const MIX_MASTER_SUM: &str = "mix_master_sum";

/// `split_mode`: Ampero Mode I — both paths get the bus.
pub const SPLIT_MODE_SAME: &str = "same";
/// `split_mode`: Ampero Mode II — each path gets a dual-mono feed picked by its balance.
pub const SPLIT_MODE_DUAL_MONO: &str = "dual_mono";
pub const POLARITY_NORMAL: &str = "normal";
pub const POLARITY_INVERT: &str = "invert";
/// Effect type and model name the split's descriptors and errors carry.
pub const SPLIT_SCHEMA_ID: &str = "split";

const SPLIT_GROUP: &str = "split";
const MIXER_GROUP: &str = "mixer";

/// Every split and mixer knob, in the spec table's order. The split editor
/// shows the `split` group, the mixer editor the `mixer` group.
pub fn split_param_specs() -> Vec<ParameterSpec> {
    let split = Some(SPLIT_GROUP);
    let mixer = Some(MIXER_GROUP);
    let percent = |path, label, group| {
        float_parameter(
            path,
            label,
            group,
            Some(100.0),
            0.0,
            100.0,
            1.0,
            ParameterUnit::Percent,
        )
    };
    let side = |path, label, group| {
        float_parameter(
            path,
            label,
            group,
            Some(0.0),
            -50.0,
            50.0,
            1.0,
            ParameterUnit::None,
        )
    };
    vec![
        enum_parameter(
            SPLIT_MODE,
            "Mode",
            split,
            Some(SPLIT_MODE_SAME),
            &[
                (SPLIT_MODE_SAME, "Same"),
                (SPLIT_MODE_DUAL_MONO, "Dual mono"),
            ],
        ),
        percent(LEVEL_TO_A, "Level to A", split),
        percent(LEVEL_TO_B, "Level to B", split),
        side(BALANCE_A, "Balance A", split),
        side(BALANCE_B, "Balance B", split),
        percent(MIX_LEVEL_A, "Level A", mixer),
        percent(MIX_LEVEL_B, "Level B", mixer),
        side(MIX_PAN_A, "Pan A", mixer),
        side(MIX_PAN_B, "Pan B", mixer),
        enum_parameter(
            MIX_B_POLARITY,
            "B polarity",
            mixer,
            Some(POLARITY_NORMAL),
            &[(POLARITY_NORMAL, "Normal"), (POLARITY_INVERT, "Invert")],
        ),
        float_parameter(
            MIX_MASTER,
            "Master",
            mixer,
            Some(50.0),
            0.0,
            100.0,
            1.0,
            ParameterUnit::Percent,
        ),
        bool_parameter(MIX_MASTER_SUM, "Master sum", mixer, Some(false)),
    ]
}

/// The knobs of a freshly added split: every spec's default.
pub fn default_split_params() -> ParameterSet {
    let mut params = ParameterSet::default();
    for spec in split_param_specs() {
        if let Some(value) = spec.default_value {
            params.insert(spec.path, value);
        }
    }
    params
}

/// Fill the missing knobs with their defaults and reject a value outside its
/// range. Lenient on unknown keys (kept with a warning), like a saved model's
/// params, so a knob added by a newer build does not make this one refuse the
/// file.
pub fn normalize_split_params(params: ParameterSet) -> Result<ParameterSet, String> {
    params.normalized_against(&split_schema())
}

/// The split's knobs as descriptors addressed on `block_id`
/// (`<block_id>::<knob>`), the shape every block editor and MCP read.
pub fn split_param_descriptors(
    block_id: &BlockId,
    params: &ParameterSet,
) -> Result<Vec<BlockParameterDescriptor>, String> {
    let schema = split_schema();
    let normalized = params.normalized_against(&schema)?;
    let ctx = MaterializeContext {
        block_id,
        effect_type: SPLIT_SCHEMA_ID,
        model: SPLIT_SCHEMA_ID,
        audio_mode: schema.audio_mode,
    };
    Ok(schema
        .parameters
        .iter()
        .map(|spec| {
            let current_value = normalized
                .get(&spec.path)
                .cloned()
                .or_else(|| spec.default_value.clone())
                .unwrap_or(ParameterValue::Null);
            spec.materialize(&ctx, current_value)
        })
        .collect())
}

fn split_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: SPLIT_SCHEMA_ID.to_string(),
        model: SPLIT_SCHEMA_ID.to_string(),
        display_name: "Split".to_string(),
        audio_mode: ModelAudioMode::TrueStereo,
        parameters: split_param_specs(),
    }
}
