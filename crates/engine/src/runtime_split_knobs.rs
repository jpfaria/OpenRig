//! Responsibility: holds a split's knob values where the audio thread reads them.
//!
//! Same pattern as `ChainRuntimeState::volume_pct_bits`: every knob is an
//! atomic (f32 bits), stored off the audio thread and loaded with Relaxed
//! ordering once per callback, so a knob value is replaced in place without
//! rebuilding any path (#328, spec §4.1).

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use block_core::param::ParameterSet;
use project::block::split_params::{
    default_split_params, BALANCE_A, BALANCE_B, LEVEL_TO_A, LEVEL_TO_B, MIX_B_POLARITY,
    MIX_LEVEL_A, MIX_LEVEL_B, MIX_MASTER, MIX_MASTER_SUM, MIX_PAN_A, MIX_PAN_B, POLARITY_INVERT,
    SPLIT_MODE, SPLIT_MODE_DUAL_MONO,
};

use crate::runtime_split::mix::SplitKnobValues;

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

#[derive(Default)]
pub(crate) struct SplitKnobs {
    dual_mono: AtomicBool,
    level_to_a: AtomicU32,
    level_to_b: AtomicU32,
    balance_a: AtomicU32,
    balance_b: AtomicU32,
    mix_level_a: AtomicU32,
    mix_level_b: AtomicU32,
    mix_pan_a: AtomicU32,
    mix_pan_b: AtomicU32,
    mix_b_invert: AtomicBool,
    mix_master: AtomicU32,
    mix_master_sum: AtomicBool,
}

impl SplitKnobs {
    pub(crate) fn from_params(params: &ParameterSet) -> Self {
        let knobs = Self::default();
        knobs.store(params);
        knobs
    }

    /// Replace every knob with the value in `params`, falling back to the
    /// Ampero default for a missing key. Off the audio thread.
    pub(crate) fn store(&self, params: &ParameterSet) {
        let defaults = default_split_params();
        let put = |slot: &AtomicU32, value: f32| slot.store(value.to_bits(), Ordering::Relaxed);
        self.dual_mono.store(
            text(params, &defaults, SPLIT_MODE) == SPLIT_MODE_DUAL_MONO,
            Ordering::Relaxed,
        );
        put(
            &self.level_to_a,
            number(params, &defaults, LEVEL_TO_A) / PERCENT,
        );
        put(
            &self.level_to_b,
            number(params, &defaults, LEVEL_TO_B) / PERCENT,
        );
        put(&self.balance_a, number(params, &defaults, BALANCE_A));
        put(&self.balance_b, number(params, &defaults, BALANCE_B));
        put(
            &self.mix_level_a,
            number(params, &defaults, MIX_LEVEL_A) / PERCENT,
        );
        put(
            &self.mix_level_b,
            number(params, &defaults, MIX_LEVEL_B) / PERCENT,
        );
        put(&self.mix_pan_a, number(params, &defaults, MIX_PAN_A));
        put(&self.mix_pan_b, number(params, &defaults, MIX_PAN_B));
        self.mix_b_invert.store(
            text(params, &defaults, MIX_B_POLARITY) == POLARITY_INVERT,
            Ordering::Relaxed,
        );
        put(
            &self.mix_master,
            number(params, &defaults, MIX_MASTER) / PERCENT,
        );
        self.mix_master_sum
            .store(flag(params, &defaults, MIX_MASTER_SUM), Ordering::Relaxed);
    }

    /// The values for this callback. A Y split (`mixes == false`) meets its
    /// paths at unity, so its mixer knobs read neutral.
    #[inline]
    pub(crate) fn load(&self, mixes: bool) -> SplitKnobValues {
        let get = |slot: &AtomicU32| f32::from_bits(slot.load(Ordering::Relaxed));
        let values = SplitKnobValues {
            dual_mono: self.dual_mono.load(Ordering::Relaxed),
            level_to_a: get(&self.level_to_a),
            level_to_b: get(&self.level_to_b),
            balance_a: get(&self.balance_a),
            balance_b: get(&self.balance_b),
            mix_level_a: get(&self.mix_level_a),
            mix_level_b: get(&self.mix_level_b),
            mix_pan_a: get(&self.mix_pan_a),
            mix_pan_b: get(&self.mix_pan_b),
            mix_b_invert: self.mix_b_invert.load(Ordering::Relaxed),
            mix_master: get(&self.mix_master),
            mix_master_sum: self.mix_master_sum.load(Ordering::Relaxed),
        };
        if mixes {
            values
        } else {
            values.with_neutral_mixer()
        }
    }
}

#[cfg(test)]
#[path = "runtime_split_knobs_tests.rs"]
mod tests;
