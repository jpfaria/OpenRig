//! Responsibility: rewrites the tempo-synced params of blocks to a BPM.
//! The global tempo reaches a synced block as a plain param value.
//!
//! A block whose `time_sync` / `rate_sync` select is not `off` gets its
//! `time_ms` / `rate_hz` recomputed from the BPM and clamped to the model's
//! own range. This runs on the control thread; the audio runtime only ever
//! sees the resulting number, exactly as if the knob had been turned.

use block_core::param::ParameterDomain;
use block_core::tempo_sync::{
    sync_beats, synced_rate_hz, synced_time_ms, RATE_PATH, RATE_SYNC_PATH, TIME_PATH,
    TIME_SYNC_PATH,
};
use domain::value_objects::ParameterValue;

use crate::block::{schema_for_block_model, AudioBlock, AudioBlockKind, CoreBlock};

/// A rewrite smaller than this is float noise, not a tempo change.
const EPSILON: f32 = 1e-3;

/// `(sync select, plain param, note value → plain value)`.
const SYNCED: [(&str, &str, fn(f32, f32) -> f32); 2] = [
    (TIME_SYNC_PATH, TIME_PATH, synced_time_ms),
    (RATE_SYNC_PATH, RATE_PATH, synced_rate_hz),
];

/// Retime every block of `blocks`, split paths and select options included.
/// `true` when any value changed.
pub fn retime_blocks(blocks: &mut [AudioBlock], bpm: f32) -> bool {
    blocks
        .iter_mut()
        .fold(false, |changed, block| retime_block(block, bpm) | changed)
}

/// Retime one block (and what it contains). `true` when any value changed.
pub fn retime_block(block: &mut AudioBlock, bpm: f32) -> bool {
    match &mut block.kind {
        AudioBlockKind::Core(core) => retime_core(core, bpm),
        AudioBlockKind::Select(select) => retime_blocks(&mut select.options, bpm),
        AudioBlockKind::Split(split) => split
            .paths
            .iter_mut()
            .fold(false, |changed, path| retime_blocks(path, bpm) | changed),
        _ => false,
    }
}

fn retime_core(core: &mut CoreBlock, bpm: f32) -> bool {
    if !(bpm.is_finite() && bpm > 0.0) {
        return false;
    }
    let mut changed = false;
    for (sync_path, value_path, derive) in SYNCED {
        let Some(beats) = core.params.get_string(sync_path).and_then(sync_beats) else {
            continue;
        };
        let target = clamp_to_range(core, value_path, derive(bpm, beats));
        let current = core.params.get_f32(value_path);
        if current.is_some_and(|value| (value - target).abs() <= EPSILON) {
            continue;
        }
        core.params
            .insert(value_path, ParameterValue::Float(target));
        changed = true;
    }
    changed
}

/// The model's own range for `path`; unclamped when the schema has none.
fn clamp_to_range(core: &CoreBlock, path: &str, value: f32) -> f32 {
    let range = schema_for_block_model(&core.effect_type, &core.model)
        .ok()
        .and_then(|schema| {
            schema
                .parameters
                .into_iter()
                .find(|spec| spec.path == path)
                .and_then(|spec| match spec.domain {
                    ParameterDomain::FloatRange { min, max, .. } => Some((min, max)),
                    _ => None,
                })
        });
    match range {
        Some((min, max)) => value.clamp(min, max),
        None => value,
    }
}

#[cfg(test)]
#[path = "tempo_retime_tests.rs"]
mod tests;
