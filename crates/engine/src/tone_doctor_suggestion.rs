//! Symptom → parameter suggestion for the Tone Doctor (#791).
//!
//! Once [`super::diagnose`] has named the guilty block, this maps the symptom
//! to a concrete knob on that block and proposes a new value — the "what do I
//! turn" half of the feature. The suggestion is a pure recommendation; applying
//! it is the caller's job (a `SetBlockParameterNumber` command), so this module
//! stays free of the command bus and is unit-testable in isolation.
//!
//! The mapping is deliberately conservative: for each symptom it walks a
//! priority list of parameter paths and picks the first the culprit block
//! actually exposes, then nudges it toward health by a fixed fraction of its
//! range. It never invents a parameter a block does not have.

use project::block::schema_for_block_model;
use project::chain::Chain;
use project::param::ParameterDomain;

use feature_dsp::tone_descriptors::Symptom;

use crate::tone_doctor::Diagnosis;

/// Fraction of a parameter's range to move when nudging it toward health.
const NUDGE_FRACTION: f32 = 0.25;

/// A concrete, applyable recommendation.
#[derive(Debug, Clone, PartialEq)]
pub struct Suggestion {
    /// Index into `chain.blocks` of the block to adjust (the culprit).
    pub block_index: usize,
    /// Parameter path to change (e.g. `presence`, `tone`, `level`).
    pub param_path: String,
    /// Human-readable label of that parameter, for the UI.
    pub param_label: String,
    /// Current value.
    pub current: f32,
    /// Proposed value.
    pub suggested: f32,
    /// A gating bool that must be turned on for `param_path` to take effect
    /// (e.g. `eq.enabled` for a NAM's `eq.treble`). `None` when the knob is
    /// always live. The caller enables it before setting the number.
    pub enable_path: Option<String>,
    /// Why, in one line.
    pub rationale: String,
}

/// Parameter paths to try, in order, for each symptom. Lowering the first one
/// the block exposes moves the tone toward health.
pub(crate) fn priority_paths(symptom: Symptom) -> &'static [&'static str] {
    match symptom {
        // Cut the highs: a dedicated presence/treble/tone first, drive last.
        Symptom::Fizz => &[
            "presence", "treble", "tone", "highs", "high", "bright", "fuzz", "drive", "gain",
        ],
        // Cut the low-mids / body.
        Symptom::Mud => &["mids", "mid", "bass", "low", "lows", "body"],
        // Cut the brilliance / ice-pick highs above the presence band.
        Symptom::Harsh => &["presence", "treble", "highs", "high", "tone", "bright", "air"],
        // Cut the sub / low-end rumble.
        Symptom::Boomy => &["bass", "low", "lows", "body", "sub"],
        // Deficit symptoms need a knob *raised*, not lowered — the auto-fix
        // engine only lowers, so it offers no path for these (reported only).
        Symptom::Thin | Symptom::Squash => &[],
        // Pull the block's output down off the rail.
        Symptom::Clipping => &[
            "level", "master", "output", "output_level", "volume", "makeup_gain", "gain",
        ],
        Symptom::Ok => &[],
    }
}

/// Propose a knob to turn for the diagnosed symptom, or `None` when the chain
/// is healthy, the culprit is unresolved, or the culprit exposes no relevant
/// float parameter.
pub fn suggest(chain: &Chain, diagnosis: &Diagnosis) -> Option<Suggestion> {
    let symptom = diagnosis.full_symptom;
    let block_index = diagnosis.culprit?;
    let block = chain.blocks.get(block_index)?;
    let model = block.model_ref()?;
    let schema = schema_for_block_model(model.effect_type, model.model).ok()?;

    // Pick the first priority parameter the culprit actually exposes as a
    // float range, in priority order.
    for wanted in priority_paths(symptom) {
        let Some(spec) = schema.parameters.iter().find(|p| p.path == *wanted) else {
            continue;
        };
        let ParameterDomain::FloatRange { min, max, .. } = spec.domain else {
            continue;
        };
        // Current value: the block's own, falling back to the schema default.
        let current = model
            .params
            .get_f32(&spec.path)
            .or_else(|| spec.default_value.as_ref().and_then(|v| v.as_f32()))?;

        // Nudge toward health: lower the knob by a fixed fraction of its range,
        // clamped so we never leave the valid range.
        let delta = NUDGE_FRACTION * (max - min);
        let suggested = (current - delta).clamp(min, max);
        if (suggested - current).abs() < f32::EPSILON {
            // Already at the floor — nothing useful to propose here.
            continue;
        }

        return Some(Suggestion {
            block_index,
            param_path: spec.path.clone(),
            param_label: spec.label.clone(),
            current,
            suggested,
            enable_path: None,
            rationale: rationale(symptom, &schema.display_name, &spec.label),
        });
    }
    None
}

/// One-line explanation for the UI.
pub(crate) fn rationale(symptom: Symptom, model_name: &str, param_label: &str) -> String {
    match symptom {
        Symptom::Fizz => {
            format!("Fizz traced to {model_name}; lowering '{param_label}' cuts presence-band energy")
        }
        Symptom::Mud => {
            format!("Mud traced to {model_name}; lowering '{param_label}' clears the low-mids")
        }
        Symptom::Harsh => {
            format!("Harshness traced to {model_name}; lowering '{param_label}' tames the brilliance band")
        }
        Symptom::Boomy => {
            format!("Boom traced to {model_name}; lowering '{param_label}' tightens the low end")
        }
        Symptom::Thin => {
            "Thin for this genre — the chain lacks low-mid body; add/raise low-end, no single knob to lower".to_string()
        }
        Symptom::Squash => {
            "Squashed for this genre — dynamics are flattened; back off compression/gain to restore crest".to_string()
        }
        Symptom::Clipping => {
            format!("Clipping traced to {model_name}; lowering '{param_label}' pulls it off the rail")
        }
        Symptom::Ok => String::new(),
    }
}

#[cfg(test)]
#[path = "tone_doctor_suggestion_tests.rs"]
mod tests;
