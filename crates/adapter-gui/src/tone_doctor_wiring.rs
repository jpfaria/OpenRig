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
use engine::tone_doctor::diagnose;
use engine::tone_doctor_suggestion::{suggest, Suggestion};
use feature_dsp::tone_descriptors::Symptom;
use project::chain::Chain;

/// The panel's result fields, mirroring `ToneDoctorState` in Slint.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ToneDoctorView {
    pub running: bool,
    pub has_result: bool,
    pub symptom_level: i32,
    pub symptom_text: String,
    pub culprit_label: String,
    pub suggestion_text: String,
    pub has_suggestion: bool,
}

/// Run the offline diagnosis over `input` and produce the panel view plus the
/// suggestion to cache for a later Apply. Pure — no window, no I/O.
pub fn diagnose_to_view(
    chain: &Chain,
    input: &[[f32; 2]],
    sample_rate: f32,
    block_size: usize,
) -> (ToneDoctorView, Option<Suggestion>) {
    let diagnosis = match diagnose(chain, sample_rate, input, block_size) {
        Ok(d) => d,
        Err(_) => return (ToneDoctorView::default(), None),
    };
    let suggestion = suggest(chain, &diagnosis);

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
            format!(
                "{} {} → {}",
                s.param_label,
                trim_num(s.current),
                trim_num(s.suggested)
            )
        })
        .unwrap_or_default();

    let view = ToneDoctorView {
        running: false,
        has_result: true,
        symptom_level: symptom_level(diagnosis.full_symptom),
        symptom_text: symptom_text(diagnosis.full_symptom).to_string(),
        culprit_label,
        has_suggestion: suggestion.is_some(),
        suggestion_text,
    };
    (view, suggestion)
}

/// Traffic-light severity for a symptom: green (0), amber (1), red (2).
fn symptom_level(s: Symptom) -> i32 {
    match s {
        Symptom::Ok => 0,
        Symptom::Mud => 1,
        Symptom::Fizz | Symptom::Clipping => 2,
    }
}

/// Short label for a symptom. Technical terms kept as-is (they double as the
/// descriptor names); dynamic localisation of these is a follow-up.
fn symptom_text(s: Symptom) -> &'static str {
    match s {
        Symptom::Ok => "OK",
        Symptom::Fizz => "Fizz",
        Symptom::Mud => "Mud",
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

/// The `Command` that applies a suggestion to its block, or `None` if the block
/// index is stale.
pub fn apply_command(chain: &Chain, chain_id: &ChainId, suggestion: &Suggestion) -> Option<Command> {
    let block = chain.blocks.get(suggestion.block_index)?;
    Some(Command::SetBlockParameterNumber {
        chain: chain_id.clone(),
        block: block.id.clone(),
        path: suggestion.param_path.clone(),
        value: suggestion.suggested as f64,
    })
}

#[cfg(test)]
#[path = "tone_doctor_wiring_tests.rs"]
mod tests;
