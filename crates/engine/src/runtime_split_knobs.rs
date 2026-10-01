//! Responsibility: holds a split's knob values where the audio thread reads them.
//!
//! Same pattern as `ChainRuntimeState::volume_pct_bits`: every knob is an
//! atomic (f32 bits), stored off the audio thread and loaded with Relaxed
//! ordering once per callback, so a knob value is replaced in place without
//! rebuilding any path (#328, spec §4.1). One set of atomics per path; the
//! path count is fixed at build (a path added or removed rebuilds the split).

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use block_core::param::ParameterSet;
use project::block::split_param_keys::{balance, level_to, mix_level, mix_pan, mix_polarity};
use project::block::split_params::{
    default_split_params, MIX_MASTER, MIX_MASTER_SUM, POLARITY_INVERT, SPLIT_MODE,
    SPLIT_MODE_DUAL_MONO,
};

use crate::runtime_split::mix::{MixKnobs, PathKnobs};

/// Percent knobs (`level_to_*`, `mix_level_*`, `mix_master`) are `x/100`.
const PERCENT: f32 = 100.0;

fn number(params: &ParameterSet, defaults: &ParameterSet, key: &str) -> f32 {
    params
        .get_f32(key)
        .or_else(|| defaults.get_f32(key))
        .unwrap_or(0.0)
}

fn text<'a>(params: &'a ParameterSet, defaults: &'a ParameterSet, key: &str) -> &'a str {
    params
        .get_string(key)
        .or_else(|| defaults.get_string(key))
        .unwrap_or_default()
}

fn flag(params: &ParameterSet, defaults: &ParameterSet, key: &str) -> bool {
    params
        .get_bool(key)
        .or_else(|| defaults.get_bool(key))
        .unwrap_or(false)
}

fn put(slot: &AtomicU32, value: f32) {
    slot.store(value.to_bits(), Ordering::Relaxed);
}

#[inline]
fn get(slot: &AtomicU32) -> f32 {
    f32::from_bits(slot.load(Ordering::Relaxed))
}

#[derive(Default)]
struct PathKnobAtomics {
    level_to: AtomicU32,
    balance: AtomicU32,
    mix_level: AtomicU32,
    mix_pan: AtomicU32,
    invert: AtomicBool,
}

#[derive(Default)]
pub(crate) struct SplitKnobs {
    dual_mono: AtomicBool,
    master: AtomicU32,
    master_sum: AtomicBool,
    paths: Vec<PathKnobAtomics>,
}

impl SplitKnobs {
    pub(crate) fn from_params(params: &ParameterSet, path_count: usize) -> Self {
        let knobs = Self {
            paths: (0..path_count)
                .map(|_| PathKnobAtomics::default())
                .collect(),
            ..Self::default()
        };
        knobs.store(params);
        knobs
    }

    /// Replace every knob with the value in `params`, falling back to the
    /// Ampero default for a missing key. Off the audio thread.
    pub(crate) fn store(&self, params: &ParameterSet) {
        let defaults = default_split_params(self.paths.len());
        self.dual_mono.store(
            text(params, &defaults, SPLIT_MODE) == SPLIT_MODE_DUAL_MONO,
            Ordering::Relaxed,
        );
        put(
            &self.master,
            number(params, &defaults, MIX_MASTER) / PERCENT,
        );
        self.master_sum
            .store(flag(params, &defaults, MIX_MASTER_SUM), Ordering::Relaxed);
        for (i, path) in self.paths.iter().enumerate() {
            put(
                &path.level_to,
                number(params, &defaults, &level_to(i)) / PERCENT,
            );
            put(&path.balance, number(params, &defaults, &balance(i)));
            put(
                &path.mix_level,
                number(params, &defaults, &mix_level(i)) / PERCENT,
            );
            put(&path.mix_pan, number(params, &defaults, &mix_pan(i)));
            path.invert.store(
                text(params, &defaults, &mix_polarity(i)) == POLARITY_INVERT,
                Ordering::Relaxed,
            );
        }
    }

    /// The values for this callback, written into the caller's preallocated
    /// `paths` (one per path). A Y split (`mixes == false`) meets its paths
    /// at unity, so its mixer knobs read neutral. No allocation.
    #[inline]
    pub(crate) fn load_into(&self, mixes: bool, paths: &mut [PathKnobs]) -> MixKnobs {
        for (out, path) in paths.iter_mut().zip(self.paths.iter()) {
            let values = PathKnobs {
                level_to: get(&path.level_to),
                balance: get(&path.balance),
                mix_level: get(&path.mix_level),
                mix_pan: get(&path.mix_pan),
                invert: path.invert.load(Ordering::Relaxed),
            };
            *out = if mixes {
                values
            } else {
                values.with_neutral_mixer()
            };
        }
        let mix = MixKnobs {
            dual_mono: self.dual_mono.load(Ordering::Relaxed),
            master: get(&self.master),
            master_sum: self.master_sum.load(Ordering::Relaxed),
        };
        if mixes {
            mix
        } else {
            mix.with_neutral_mixer()
        }
    }
}

#[cfg(test)]
#[path = "runtime_split_knobs_tests.rs"]
mod tests;
