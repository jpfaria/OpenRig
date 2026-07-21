//! #791 — Tone Doctor GUI wiring: turn an offline diagnosis into the panel's
//! view fields, and turn its suggested fix into a `Command`.
//!
//! The heavy lifting (the ablation, the suggestion) lives in `engine`; this
//! module is the thin, pure adapter between that and the `ToneDoctorState`
//! Slint global. Kept pure (no window, no dispatcher) so it is unit-testable —
//! the actual closure wiring in `compact_chain_callbacks` just calls these and
//! copies the fields onto the global.

use application::command::Command;
use domain::ids::ChainId;
use engine::tone_doctor::diagnose_with_limits;
use engine::tone_doctor_fix::measure_fix_with_limits;
use engine::tone_doctor_suggestion::Suggestion;
use feature_dsp::tone_descriptors::{Symptom, SymptomLimits};
use project::chain::Chain;

/// The panel's result fields, mirroring `ToneDoctorState` in Slint. Carries the
/// three raw measurements + their healthy limits so the panel can show the
/// meters (the user sees the data behind the verdict), not just a word.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ToneDoctorView {
    pub running: bool,
    pub has_result: bool,
    pub symptom_level: i32,
    pub symptom_text: String,
    pub culprit_label: String,
    pub suggestion_text: String,
    pub has_suggestion: bool,
    // Measurements (value, limit) for the three meters.
    pub fizz_value: f32,
    pub fizz_limit: f32,
    pub mud_value: f32,
    pub mud_limit: f32,
    pub harsh_value: f32,
    pub harsh_limit: f32,
    pub boom_value: f32,
    pub boom_limit: f32,
    pub clip_value: f32,
    pub clip_limit: f32,
}

/// Run the offline diagnosis over `input` and produce the panel view plus the
/// measured correction to cache for a later Apply. Renders (via `diagnose` +
/// `measure_fix`), so run it off the GUI thread.
pub fn diagnose_to_view(
    chain: &Chain,
    input: &[[f32; 2]],
    sample_rate: f32,
    block_size: usize,
    limits: SymptomLimits,
) -> (ToneDoctorView, Option<Suggestion>) {
    let diagnosis = match diagnose_with_limits(chain, sample_rate, input, block_size, &limits) {
        Ok(d) => d,
        Err(_) => return (ToneDoctorView::default(), None),
    };
    // Measured, not guessed: prove the fix actually clears the symptom.
    let suggestion = measure_fix_with_limits(chain, sample_rate, input, block_size, &diagnosis, &limits)
        .ok()
        .flatten();

    // A readable "effect:model" label (e.g. "gain:fuzz_si"), not the internal
    // model_identity ("core:gain/fuzz_si").
    let culprit_label = diagnosis
        .culprit
        .and_then(|i| chain.blocks.get(i))
        .and_then(|b| b.model_ref())
        .map(|m| format!("{}:{}", m.effect_type, m.model))
        .unwrap_or_default();

    let suggestion_text = suggestion
        .as_ref()
        .map(|s| {
            // Prefix the gate we turn on, e.g. "EQ on · Treble 7 → 4".
            let gate = s
                .enable_path
                .as_deref()
                .and_then(|p| p.strip_suffix(".enabled"))
                .map(|g| format!("{} on · ", g.to_uppercase()))
                .unwrap_or_default();
            format!(
                "{gate}{} {} → {}",
                s.param_label,
                trim_num(s.current),
                trim_num(s.suggested)
            )
        })
        .unwrap_or_default();

    let d = &diagnosis.full_descriptors;
    let view = ToneDoctorView {
        running: false,
        has_result: true,
        symptom_level: symptom_level(diagnosis.full_symptom),
        symptom_text: symptom_text(diagnosis.full_symptom).to_string(),
        culprit_label,
        has_suggestion: suggestion.is_some(),
        suggestion_text,
        fizz_value: d.fizz_ratio,
        fizz_limit: limits.fizz,
        mud_value: d.mud_ratio,
        mud_limit: limits.mud,
        harsh_value: d.harsh_ratio,
        harsh_limit: limits.harsh,
        boom_value: d.boom_ratio,
        boom_limit: limits.boom,
        clip_value: d.clip_fraction,
        clip_limit: limits.clip,
    };
    (view, suggestion)
}

/// Traffic-light severity for a symptom: green (0), amber (1), red (2).
fn symptom_level(s: Symptom) -> i32 {
    match s {
        Symptom::Ok => 0,
        Symptom::Mud | Symptom::Boomy | Symptom::Thin | Symptom::Squash => 1,
        Symptom::Fizz | Symptom::Harsh | Symptom::Clipping => 2,
    }
}

/// Short label for a symptom. Technical terms kept as-is (they double as the
/// descriptor names); dynamic localisation of these is a follow-up.
fn symptom_text(s: Symptom) -> &'static str {
    match s {
        Symptom::Ok => "OK",
        Symptom::Fizz => "Fizz",
        Symptom::Mud => "Mud",
        Symptom::Harsh => "Harsh",
        Symptom::Boomy => "Boomy",
        Symptom::Thin => "Thin",
        Symptom::Squash => "Squash",
        Symptom::Clipping => "Clipping",
    }
}

/// Format a knob value without a trailing `.0` (e.g. `70` not `70.0`, but
/// `5.5` stays `5.5`).
fn trim_num(v: f32) -> String {
    if (v - v.round()).abs() < 0.05 {
        format!("{}", v.round() as i64)
    } else {
        format!("{v:.1}")
    }
}

/// The `Command`s that apply a suggestion to its block: enable the gating bool
/// first (e.g. `eq.enabled` for a NAM's `eq.treble`) when present, then set the
/// number. Empty when the block index is stale.
pub fn apply_commands(chain: &Chain, chain_id: &ChainId, suggestion: &Suggestion) -> Vec<Command> {
    let Some(block) = chain.blocks.get(suggestion.block_index) else {
        return Vec::new();
    };
    let mut cmds = Vec::new();
    if let Some(enable) = &suggestion.enable_path {
        cmds.push(Command::SetBlockParameterBool {
            chain: chain_id.clone(),
            block: block.id.clone(),
            path: enable.clone(),
            value: true,
        });
    }
    cmds.push(Command::SetBlockParameterNumber {
        chain: chain_id.clone(),
        block: block.id.clone(),
        path: suggestion.param_path.clone(),
        value: suggestion.suggested as f64,
    });
    cmds
}

#[cfg(test)]
#[path = "tone_doctor_wiring_tests.rs"]
mod tests;
