//! Measured (closed-loop) correction for the Tone Doctor (#791).
//!
//! The static `tone_doctor_suggestion::suggest` guesses a knob + a fixed 25 %
//! nudge without checking the result. This module instead PROVES the fix: for
//! the culprit block it sweeps each candidate parameter downward, re-renders
//! the chain, and measures the offending descriptor. It returns the gentlest
//! value that actually brings the symptom back under its healthy limit.
//!
//! If no parameter on the culprit can — e.g. mud baked into a fixed NAM capture
//! has no knob that removes it — it returns `None`. That honest "no fix on this
//! block" is the whole point: never suggest a change that does nothing.

use anyhow::Result;

use feature_dsp::tone_descriptors::Symptom;
use project::block::param_writer::{set_parameter_bool, set_parameter_number};
use project::block::schema_for_block_model;
use project::chain::Chain;
use project::param::{ModelParameterSchema, ParameterDomain};

use crate::tone_doctor::{symptom_metric, Diagnosis};
use crate::tone_doctor_suggestion::{rationale, Suggestion};

/// Tail frames appended to each render (matches the diagnosis path).
const DIAGNOSE_TAIL_FRAMES: usize = 4_096;

/// Reduction fractions to try, gentlest first, of the way from the current
/// value toward the parameter's minimum. The first one that reaches health is
/// suggested — the smallest change that fixes the tone. Kept coarse (3 steps)
/// because each render rebuilds every block and reloads the NAM from disk, so
/// the whole diagnosis has to stay within a handful of passes.
const SWEEP_FRACTIONS: [f32; 4] = [0.25, 0.5, 0.75, 1.0];

/// Cap on re-renders so a heavy NAM chain can't run for minutes.
const MAX_RENDERS: usize = 8;

/// Keywords, most-relevant first, whose param path OR label marks a knob as a
/// candidate for correcting a symptom. Fuzzy so it matches `eq.treble` /
/// "Treble" as well as a native block's bare `treble`.
fn symptom_keywords(symptom: Symptom) -> &'static [&'static str] {
    match symptom {
        Symptom::Fizz => &["presence", "treble", "tone", "bright", "high", "fuzz", "drive", "gain"],
        Symptom::Mud => &["bass", "low", "body", "middle", "mid"],
        Symptom::Clipping => &["output", "level", "master", "volume", "makeup", "gain"],
        Symptom::Ok => &[],
    }
}

/// A float knob to try lowering, plus the gating bool (if any) that must be on
/// for it to take effect.
struct Candidate {
    path: String,
    label: String,
    min: f32,
    current: f32,
    enable_path: Option<String>,
}

/// Collect the culprit's float parameters that plausibly affect `symptom`,
/// ordered by keyword relevance. Each carries its gating `<group>.enabled` bool
/// when one exists (e.g. a NAM's `eq.treble` is gated by `eq.enabled`).
fn candidates(
    schema: &ModelParameterSchema,
    params: &project::param::ParameterSet,
    symptom: Symptom,
) -> Vec<Candidate> {
    let keywords = symptom_keywords(symptom);
    let mut scored: Vec<(usize, Candidate)> = Vec::new();
    for spec in &schema.parameters {
        let ParameterDomain::FloatRange { min, .. } = spec.domain else {
            continue;
        };
        let hay = format!("{} {}", spec.path, spec.label).to_lowercase();
        let Some(score) = keywords.iter().position(|k| hay.contains(k)) else {
            continue;
        };
        let current = params
            .get_f32(&spec.path)
            .or_else(|| spec.default_value.as_ref().and_then(|v| v.as_f32()));
        let Some(current) = current else { continue };
        // A `group.knob` param may be gated by a `group.enabled` bool.
        let enable_path = spec.path.rsplit_once('.').and_then(|(group, _)| {
            let candidate = format!("{group}.enabled");
            schema
                .parameters
                .iter()
                .any(|p| p.path == candidate && matches!(p.domain, ParameterDomain::Bool))
                .then_some(candidate)
        });
        scored.push((
            score,
            Candidate {
                path: spec.path.clone(),
                label: spec.label.clone(),
                min,
                current,
                enable_path,
            },
        ));
    }
    scored.sort_by_key(|(s, _)| *s);
    scored.into_iter().map(|(_, c)| c).collect()
}

/// Find a PROVEN correction for the diagnosed symptom, or `None` when no knob
/// on the culprit brings the tone back to health.
pub fn measure_fix(
    chain: &Chain,
    sample_rate: f32,
    input: &[[f32; 2]],
    block_size: usize,
    diagnosis: &Diagnosis,
) -> Result<Option<Suggestion>> {
    let symptom = diagnosis.full_symptom;
    let Some(culprit) = diagnosis.culprit else {
        return Ok(None);
    };
    let (_, limit) = match symptom_metric(symptom, &diagnosis.full_descriptors) {
        Some(m) => m,
        None => return Ok(None),
    };
    let Some(model) = chain.blocks.get(culprit).and_then(|b| b.model_ref()) else {
        return Ok(None);
    };
    let Ok(schema) = schema_for_block_model(model.effect_type, model.model) else {
        return Ok(None);
    };
    let cands = candidates(&schema, model.params, symptom);

    let mut renders = 0usize;
    // Reuse the built processors across trials: only the culprit block (whose
    // one param we sweep) rebuilds; an unrelated NAM keeps its loaded model.
    let mut base = None;
    for cand in &cands {
        for &frac in &SWEEP_FRACTIONS {
            if renders >= MAX_RENDERS {
                return Ok(None);
            }
            let value = cand.current - frac * (cand.current - cand.min);
            if (value - cand.current).abs() < f32::EPSILON {
                continue;
            }
            // Build the trial: enable the gate (if any), then lower the knob.
            let mut variant = chain.clone();
            let block = &mut variant.blocks[culprit];
            if let Some(enable) = &cand.enable_path {
                set_parameter_bool(block, enable, true)?;
            }
            set_parameter_number(block, &cand.path, value as f64)?;

            let (samples, nodes) = crate::offline::render_reusing(
                &variant,
                sample_rate,
                input,
                block_size,
                DIAGNOSE_TAIL_FRAMES,
                base.take(),
            )?;
            base = Some(nodes);
            let desc = feature_dsp::tone_descriptors::analyze(&samples, sample_rate);
            renders += 1;
            let healthy = symptom_metric(symptom, &desc)
                .map(|(v, _)| v < limit)
                .unwrap_or(false);
            if healthy {
                return Ok(Some(Suggestion {
                    block_index: culprit,
                    param_path: cand.path.clone(),
                    param_label: cand.label.clone(),
                    current: cand.current,
                    suggested: value,
                    enable_path: cand.enable_path.clone(),
                    rationale: rationale(symptom, &schema.display_name, &cand.label),
                }));
            }
        }
    }
    // No single knob on this block brings the tone back to health — honest
    // "no fix on this block" (e.g. the symptom is baked into a fixed capture).
    Ok(None)
}

#[cfg(test)]
#[path = "tone_doctor_fix_tests.rs"]
mod tests;
