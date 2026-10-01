//! Responsibility: declares the knob schema of a split block.
//!
//! #328 (spec §1.2, §11.2): the knobs of the split node and mixer node, one
//! set per path plus the split-wide ones. They live in `SplitBlock.params` and
//! are edited through the ordinary `SetBlockParameter*` commands, so MIDI
//! mapping, scenes and MCP reach them with no split-specific path. The schema
//! depends on how many paths the split has. A split is not a catalog effect
//! type: this schema never reaches the model registry.

use block_core::ModelAudioMode;
use domain::ids::BlockId;
use domain::value_objects::ParameterValue;

use super::path_ref::path_letter;
use super::split_param_keys::{
    balance, level_to, migrate_legacy_split_keys, mix_level, mix_pan, mix_polarity,
};
use crate::param::{
    bool_parameter, enum_parameter, float_parameter, BlockParameterDescriptor, MaterializeContext,
    ModelParameterSchema, ParameterSet, ParameterSpec, ParameterUnit,
};

pub const SPLIT_MODE: &str = "split_mode";
pub const MIX_MASTER: &str = "mix_master";
pub const MIX_MASTER_SUM: &str = "mix_master_sum";

/// `split_mode`: Ampero Mode I — every path gets the bus.
pub const SPLIT_MODE_SAME: &str = "same";
/// `split_mode`: Ampero Mode II — each path gets a dual-mono feed picked by its balance.
pub const SPLIT_MODE_DUAL_MONO: &str = "dual_mono";
pub const POLARITY_NORMAL: &str = "normal";
pub const POLARITY_INVERT: &str = "invert";
/// Effect type and model name the split's descriptors and errors carry.
pub const SPLIT_SCHEMA_ID: &str = "split";

/// The groups double as the compact row's tab labels, cased like every
/// other block's groups ("Output", "EQ").
pub const SPLIT_GROUP: &str = "Split";
pub const MIXER_GROUP: &str = "Mixer";

fn percent(path: &str, label: &str, group: Option<&str>) -> ParameterSpec {
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
}

fn side(path: &str, label: &str, group: Option<&str>) -> ParameterSpec {
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
}

/// Every split and mixer knob of a split with `path_count` paths. The split
/// editor shows the `Split` group, the mixer editor the `Mixer` group.
pub fn split_param_specs(path_count: usize) -> Vec<ParameterSpec> {
    let split = Some(SPLIT_GROUP);
    let mixer = Some(MIXER_GROUP);
    let mut specs = vec![enum_parameter(
        SPLIT_MODE,
        "Mode",
        split,
        Some(SPLIT_MODE_SAME),
        &[
            (SPLIT_MODE_SAME, "Same"),
            (SPLIT_MODE_DUAL_MONO, "Dual mono"),
        ],
    )];
    for i in 0..path_count {
        let letter = path_letter(i);
        specs.push(percent(&level_to(i), &format!("Level to {letter}"), split));
        specs.push(side(&balance(i), &format!("Balance {letter}"), split));
    }
    for i in 0..path_count {
        let letter = path_letter(i);
        specs.push(percent(&mix_level(i), &format!("Level {letter}"), mixer));
        specs.push(side(&mix_pan(i), &format!("Pan {letter}"), mixer));
        specs.push(enum_parameter(
            &mix_polarity(i),
            &format!("{letter} polarity"),
            mixer,
            Some(POLARITY_NORMAL),
            &[(POLARITY_NORMAL, "Normal"), (POLARITY_INVERT, "Invert")],
        ));
    }
    specs.push(float_parameter(
        MIX_MASTER,
        "Master",
        mixer,
        Some(50.0),
        0.0,
        100.0,
        1.0,
        ParameterUnit::Percent,
    ));
    specs.push(bool_parameter(
        MIX_MASTER_SUM,
        "Master sum",
        mixer,
        Some(false),
    ));
    specs
}

/// The knobs of a freshly added split: every spec's default.
pub fn default_split_params(path_count: usize) -> ParameterSet {
    let mut params = ParameterSet::default();
    for spec in split_param_specs(path_count) {
        if let Some(value) = spec.default_value {
            params.insert(spec.path, value);
        }
    }
    params
}

/// Rename legacy keys, fill the missing knobs with their defaults and reject
/// a value outside its range. Lenient on unknown keys (kept with a warning),
/// like a saved model's params, so a knob added by a newer build does not
/// make this one refuse the file.
pub fn normalize_split_params(
    params: ParameterSet,
    path_count: usize,
) -> Result<ParameterSet, String> {
    migrate_legacy_split_keys(params).normalized_against(&split_schema(path_count))
}

/// Refuse `value` for the knob `path` when the schema does (unknown knob, a
/// path the split does not have, out of range, unknown option), so a command
/// never stores a knob the split cannot load.
pub fn check_split_knob(
    path: &str,
    value: ParameterValue,
    path_count: usize,
) -> Result<(), String> {
    if !split_param_specs(path_count)
        .iter()
        .any(|spec| spec.path == path)
    {
        return Err(format!(
            "'{path}' is not a knob of a split with {path_count} paths"
        ));
    }
    let mut params = ParameterSet::default();
    params.insert(path, value);
    normalize_split_params(params, path_count).map(|_| ())
}

/// The split's knobs as descriptors addressed on `block_id`
/// (`<block_id>::<knob>`), the shape every block editor and MCP read.
pub fn split_param_descriptors(
    block_id: &BlockId,
    params: &ParameterSet,
    path_count: usize,
) -> Result<Vec<BlockParameterDescriptor>, String> {
    let schema = split_schema(path_count);
    let normalized = normalize_split_params(params.clone(), path_count)?;
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

fn split_schema(path_count: usize) -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: SPLIT_SCHEMA_ID.to_string(),
        model: SPLIT_SCHEMA_ID.to_string(),
        display_name: "Split".to_string(),
        audio_mode: ModelAudioMode::TrueStereo,
        parameters: split_param_specs(path_count),
    }
}
