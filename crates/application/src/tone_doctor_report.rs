//! #791 — the Tone Doctor's verdict as transport-agnostic data.
//!
//! `engine` owns the DSP (the ablation and the measured correction); this
//! module turns its result into one serializable report every transport can
//! carry — the GUI panel renders it, MCP returns it from the tool call, gRPC
//! will send it over the wire — and into the `Command`s that apply the fix.
//!
//! Block identity travels as `BlockId`, never as the chain-relative index the
//! engine works with: an index is meaningless to a remote caller and goes stale
//! the moment a block moves.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use domain::ids::{BlockId, ChainId};
use engine::tone_doctor::diagnose_with_limits;
use engine::tone_doctor_fix::measure_fix_with_limits;
use feature_dsp::tone_descriptors::{Symptom, SymptomLimits};
use project::chain::Chain;

use crate::command::{BlockCommand, Command};

/// Fixed block size for the offline diagnosis render.
pub const DIAGNOSE_BLOCK: usize = 512;

/// The measured correction: which knob, on which block, from what to what.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ToneFix {
    pub block: BlockId,
    pub param_path: String,
    pub param_label: String,
    pub current: f32,
    pub suggested: f32,
    /// The group's `*.enabled` path to switch on first, when the knob sits in a
    /// group that is currently off.
    pub enable_path: Option<String>,
    pub rationale: String,
}

/// One descriptor's measured value against the limit it is judged by, so a
/// caller can show the data behind the verdict instead of only the word.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ToneMeasurement {
    pub value: f32,
    pub limit: f32,
}

/// What the doctor found for one chain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ToneReport {
    /// `Ok`, `Fizz`, `Mud`, `Boomy`, `Thin`, `Squash` or `Clipping`.
    pub symptom: String,
    /// Traffic light: 0 healthy, 1 warning, 2 bad.
    pub severity: i32,
    /// The block blamed for the symptom, when one stands out.
    pub culprit: Option<BlockId>,
    /// Its human-readable model name (e.g. "Ibanez TS808"), empty when healthy.
    pub culprit_label: String,
    pub fizz: ToneMeasurement,
    pub mud: ToneMeasurement,
    pub boom: ToneMeasurement,
    pub clip: ToneMeasurement,
    pub suggestion: Option<ToneFix>,
}

/// Run the ablation over `input` and measure a correction for what it finds.
///
/// Renders the chain once per block, so never call this on the audio thread or
/// on a UI thread — the dispatcher runs it on its own task.
pub fn diagnose(
    chain: &Chain,
    input: &[[f32; 2]],
    sample_rate: f32,
    block_size: usize,
    limits: &SymptomLimits,
) -> Result<ToneReport, String> {
    let diagnosis = diagnose_with_limits(chain, sample_rate, input, block_size, limits)
        .map_err(|e| format!("tone diagnosis failed: {e}"))?;

    // Measured, not guessed: the correction is only reported once a re-render
    // proves it clears the symptom.
    let suggestion =
        measure_fix_with_limits(chain, sample_rate, input, block_size, &diagnosis, limits)
            .ok()
            .flatten();

    let culprit_block = diagnosis
        .culprit
        .and_then(|i| chain.blocks.get(i))
        .map(|b| b.id.clone());

    let fix = suggestion.and_then(|s| {
        chain.blocks.get(s.block_index).map(|b| ToneFix {
            block: b.id.clone(),
            param_path: s.param_path,
            param_label: s.param_label,
            current: s.current,
            suggested: s.suggested,
            enable_path: s.enable_path,
            rationale: s.rationale,
        })
    });

    let d = &diagnosis.full_descriptors;
    Ok(ToneReport {
        symptom: symptom_name(diagnosis.full_symptom).to_string(),
        severity: severity(diagnosis.full_symptom),
        culprit: culprit_block,
        culprit_label: culprit_label(chain, diagnosis.culprit),
        fizz: ToneMeasurement {
            value: d.fizz_ratio,
            limit: limits.fizz,
        },
        mud: ToneMeasurement {
            value: d.mud_ratio,
            limit: limits.mud,
        },
        boom: ToneMeasurement {
            value: d.boom_ratio,
            limit: limits.boom,
        },
        clip: ToneMeasurement {
            value: d.clip_fraction,
            limit: limits.clip,
        },
        suggestion: fix,
    })
}

/// The commands that apply `fix`: switch on the gating group first when the
/// knob sits in one, then set the measured value.
pub fn fix_commands(chain: &ChainId, fix: &ToneFix) -> Vec<Command> {
    let mut cmds = Vec::new();
    if let Some(enable) = &fix.enable_path {
        cmds.push(Command::Block(BlockCommand::SetBlockParameterBool {
            chain: chain.clone(),
            block: fix.block.clone(),
            path: enable.clone(),
            value: true,
        }));
    }
    cmds.push(Command::Block(BlockCommand::SetBlockParameterNumber {
        chain: chain.clone(),
        block: fix.block.clone(),
        path: fix.param_path.clone(),
        value: fix.suggested as f64,
    }));
    cmds
}

/// The culprit block's human-readable model name, empty when the chain is
/// healthy or the block exposes no model.
fn culprit_label(chain: &Chain, culprit: Option<usize>) -> String {
    culprit
        .and_then(|i| chain.blocks.get(i))
        .and_then(|b| b.model_ref())
        .map(|m| project::catalog::model_display_name(m.effect_type, m.model))
        .unwrap_or_default()
}

/// Traffic-light severity: green (0), amber (1), red (2).
fn severity(s: Symptom) -> i32 {
    match s {
        Symptom::Ok => 0,
        Symptom::Mud | Symptom::Boomy | Symptom::Thin | Symptom::Squash => 1,
        Symptom::Fizz | Symptom::Clipping => 2,
    }
}

/// Stable wire name for a symptom — the GUI localises it, MCP reads it as-is.
fn symptom_name(s: Symptom) -> &'static str {
    match s {
        Symptom::Ok => "Ok",
        Symptom::Fizz => "Fizz",
        Symptom::Mud => "Mud",
        Symptom::Boomy => "Boomy",
        Symptom::Thin => "Thin",
        Symptom::Squash => "Squash",
        Symptom::Clipping => "Clipping",
    }
}
