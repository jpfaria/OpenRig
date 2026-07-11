//! #780: synthesise an OpenRig parameter schema for a VST3 block from the
//! plugin's own parameters (read off its `IEditController`, cached).
//!
//! The light discovery scan leaves `entry.info.params` empty, so a catalog VST3
//! has no manifest-authored knobs. Here each real parameter becomes an OpenRig
//! control chosen by its `step_count`:
//!
//! * `0`  → continuous knob (0–100 %),
//! * `1`  → on/off toggle,
//! * `>=2`→ select, with one option per step (labels read from the plugin).
//!
//! Every parameter is stored under `p{id}`; the engine converts each value back
//! to a VST3 normalized 0..1 (`stereo::try_in_place_update` /
//! `runtime_block_core`), so the standard SetBlockParameter path drives a VST3
//! exactly like any other block.

use block_core::param::ParameterUnit;
use block_core::param::{bool_parameter, enum_parameter, float_parameter, ParameterSpec};

/// Build the parameter specs for a VST3 `model`, or an empty vec if the plugin
/// exposes none / cannot be read.
pub fn vst3_parameters(model: &str) -> Vec<ParameterSpec> {
    vst3_host::catalog_params(model)
        .iter()
        .map(|p| {
            let path = format!("p{}", p.id);
            let label = if p.title.is_empty() {
                p.short_title.clone()
            } else {
                p.title.clone()
            };
            if p.step_count == 1 {
                bool_parameter(&path, &label, None, Some(p.default_normalized >= 0.5))
            } else if p.step_count >= 2 {
                let options: Vec<(&str, &str)> = p
                    .enum_options
                    .iter()
                    .map(|(v, l)| (v.as_str(), l.as_str()))
                    .collect();
                let default_val = format!("{}", p.default_normalized * 100.0);
                enum_parameter(&path, &label, None, Some(&default_val), &options)
            } else {
                float_parameter(
                    &path,
                    &label,
                    None,
                    Some((p.default_normalized * 100.0) as f32),
                    0.0,
                    100.0,
                    1.0,
                    ParameterUnit::Percent,
                )
            }
        })
        .collect()
}
