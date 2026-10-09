//! Responsibility: measures one native model against every battery criterion.

use std::sync::OnceLock;
use std::time::Instant;

use block_core::param::{ParameterDomain, ParameterSet};
use domain::value_objects::ParameterValue;

use super::catalog::NativeModel;
use super::guitar::guitar_di;
use super::render::{default_params, render, Rendered};
use super::signals::{bin_hz, db_to_lin, noise_burst, pink_multisine, sine};
use super::spectrum::{dc_dbfs, inharmonic_db, peak_dbfs, power_spectrum, tail_rms_dbfs, thd_n_db};
use super::variants::{all_max, one_at_a_time};

pub const SR: f32 = 48_000.0;
/// 999.0 Hz at 48 kHz: on a bin, so the tone does not leak.
pub const TONE_BIN: usize = 341;
/// Nominal DI level of the tone, dBFS peak.
pub const TONE_DBFS: f32 = -18.0;
/// 5001.0 Hz at 48 kHz. An odd bin, so a folded harmonic never lands on
/// a harmonic bin.
pub const HOT_BIN: usize = 1_707;
pub const HOT_DBFS: f32 = -6.0;
/// 20 kHz: aliasing is counted in the audible band only.
pub const AUDIBLE_TOP_BIN: usize = 6_826;
/// RMS of the nominal programme (peaks near -12 dBFS, a hot DI).
pub const PROGRAMME_DBFS: f32 = -24.0;
const PROGRAMME_SECS: f32 = 6.0;
/// The level window: the last seconds of the programme, past any build-up.
const LEVEL_SECS: f32 = 3.0;
/// Knobs set for the aliasing tone only: pitch modulation spreads every
/// harmonic into sidebands that would read as aliasing.
const ALIASING_TONE_KNOBS: &[(&str, &str, f32)] = &[("tape_saturation", "wow", 0.0)];

/// What the battery measured on one model. Every number is the worse of
/// the two output channels.
#[derive(Debug, Clone)]
pub struct ModelReport {
    pub id: String,
    pub family: String,
    pub nonlinear: bool,
    pub time_varying: bool,
    pub build_error: Option<String>,
    /// Output RMS minus input RMS for the nominal programme, dB.
    pub level_db: f32,
    /// Output RMS minus input RMS for the nominal tone, dB: the passband gain.
    pub tone_level_db: f32,
    /// Output RMS minus input RMS for the guitar DI, whole take, dB.
    pub guitar_level_db: f32,
    /// Programme level with the `mix` knob at its minimum, dB: fully dry.
    /// NaN for models without a `mix` knob.
    pub dry_level_db: f32,
    pub thd_n_db: f32,
    pub dc_dbfs: f32,
    /// Non-harmonic energy for a hot 5 kHz tone, dB re fundamental.
    pub inharmonic_db: f32,
    /// Output with digital silence in from a fresh instance, dBFS.
    pub silence_dbfs: f32,
    /// Output 11.5 s after a short noise burst, dBFS.
    pub tail_dbfs: f32,
    /// Loudest peak over every one-knob extreme with the nominal programme
    /// in, dBFS, and which knob.
    pub extreme_peak_dbfs: f32,
    pub extreme_peak_at: String,
    /// Knob settings that produced NaN/Inf or failed to build.
    pub broken_at: Vec<String>,
    /// Output 9.5 s after a burst with every knob at max, dBFS.
    pub all_max_tail_dbfs: f32,
    /// That tail minus the loudest earlier half second, dB: above 0 it grows.
    pub all_max_growth_db: f32,
    /// Level at 96 kHz minus level at 48 kHz for the same programme, dB.
    pub sr96_level_delta_db: f32,
    pub sr44_finite: bool,
}

fn worst<F: Fn(&[f32]) -> f32>(r: &Rendered, f: F) -> f32 {
    r.channels()
        .iter()
        .map(|c| f(c))
        .fold(f32::NEG_INFINITY, f32::max)
}

fn secs(s: f32, sr: f32) -> usize {
    (s * sr) as usize
}

/// The nominal programme at `sr`, generated once per rate.
fn programme(sr: f32) -> &'static [f32] {
    static CACHE: [OnceLock<Vec<f32>>; 3] = [OnceLock::new(), OnceLock::new(), OnceLock::new()];
    let slot = match sr as u32 {
        44_100 => 0,
        48_000 => 1,
        96_000 => 2,
        other => panic!("no programme cached for {other} Hz"),
    };
    CACHE[slot].get_or_init(|| pink_multisine(secs(PROGRAMME_SECS, sr), sr, PROGRAMME_DBFS))
}

fn level_db(r: &Rendered, input: &[f32], sr: f32) -> f32 {
    let window = secs(LEVEL_SECS, sr);
    worst(r, |c| tail_rms_dbfs(c, window)) - tail_rms_dbfs(input, window)
}

pub fn measure(model: &NativeModel) -> ModelReport {
    let mut report = ModelReport {
        id: model.id.clone(),
        family: format!("{:?}", model.block_type),
        nonlinear: model.is_nonlinear(),
        time_varying: model.is_time_varying(),
        build_error: None,
        level_db: f32::NAN,
        tone_level_db: f32::NAN,
        guitar_level_db: f32::NAN,
        dry_level_db: f32::NAN,
        thd_n_db: f32::NAN,
        dc_dbfs: f32::NAN,
        inharmonic_db: f32::NAN,
        silence_dbfs: f32::NAN,
        tail_dbfs: f32::NAN,
        extreme_peak_dbfs: f32::NAN,
        extreme_peak_at: String::new(),
        broken_at: Vec::new(),
        all_max_tail_dbfs: f32::NAN,
        all_max_growth_db: f32::NAN,
        sr96_level_delta_db: f32::NAN,
        sr44_finite: false,
    };
    let defaults = match default_params(model) {
        Ok(p) => p,
        Err(e) => {
            report.build_error = Some(format!("defaults: {e}"));
            return report;
        }
    };
    let run = |params: &ParameterSet, sr: f32, input: &[f32]| render(model, params, sr, input);

    // Nominal programme: level.
    match run(&defaults, SR, programme(SR)) {
        Ok(r) => report.level_db = level_db(&r, programme(SR), SR),
        Err(e) => {
            report.build_error = Some(e.to_string());
            return report;
        }
    }

    // Guitar DI: level as a player hears it.
    let (di, di_sr) = guitar_di();
    match run(&defaults, *di_sr, di) {
        Ok(r) => {
            report.guitar_level_db =
                worst(&r, |c| tail_rms_dbfs(c, c.len())) - tail_rms_dbfs(di, di.len())
        }
        Err(e) => report.broken_at.push(format!("guitar DI: {e}")),
    }

    // Mix at its minimum: fully dry must be the input.
    if let Some(mix) = model.schema.parameters.iter().find(|p| p.path == "mix") {
        if let ParameterDomain::FloatRange { min, .. } = mix.domain {
            let mut dry = defaults.clone();
            dry.insert("mix", ParameterValue::Float(min));
            match run(&dry, SR, programme(SR)) {
                Ok(r) => report.dry_level_db = level_db(&r, programme(SR), SR),
                Err(e) => report.broken_at.push(format!("mix=min: {e}")),
            }
        }
    }

    // Nominal tone: THD+N, DC.
    let tone = sine(
        secs(4.0, SR),
        bin_hz(TONE_BIN, SR),
        SR,
        db_to_lin(TONE_DBFS),
    );
    match run(&defaults, SR, &tone) {
        Ok(r) => {
            if !r.all_finite() {
                report.broken_at.push("default: tone".into());
            }
            let window = secs(1.0, SR);
            report.tone_level_db =
                worst(&r, |c| tail_rms_dbfs(c, window)) - tail_rms_dbfs(&tone, window);
            report.thd_n_db = worst(&r, |c| thd_n_db(&power_spectrum(c), TONE_BIN));
            report.dc_dbfs = worst(&r, dc_dbfs);
        }
        Err(e) => {
            report.build_error = Some(e.to_string());
            return report;
        }
    }

    // Hot high tone: aliasing.
    let hot = sine(secs(3.0, SR), bin_hz(HOT_BIN, SR), SR, db_to_lin(HOT_DBFS));
    let mut hot_params = defaults.clone();
    for (_, knob, value) in ALIASING_TONE_KNOBS
        .iter()
        .filter(|(id, ..)| *id == model.id)
    {
        hot_params.insert(*knob, ParameterValue::Float(*value));
    }
    match run(&hot_params, SR, &hot) {
        Ok(r) => {
            report.inharmonic_db = worst(&r, |c| {
                inharmonic_db(&power_spectrum(c), HOT_BIN, AUDIBLE_TOP_BIN)
            })
        }
        Err(e) => report.broken_at.push(format!("hot tone: {e}")),
    }

    // Silence in from a fresh instance.
    match run(&defaults, SR, &vec![0.0; secs(2.0, SR)]) {
        Ok(r) => report.silence_dbfs = worst(&r, |c| tail_rms_dbfs(c, secs(1.0, SR))),
        Err(e) => report.broken_at.push(format!("silence: {e}")),
    }

    // Tail after a burst.
    let burst = noise_burst(secs(12.0, SR), secs(0.5, SR), db_to_lin(-12.0), 7);
    match run(&defaults, SR, &burst) {
        Ok(r) => report.tail_dbfs = worst(&r, |c| tail_rms_dbfs(c, secs(0.5, SR))),
        Err(e) => report.broken_at.push(format!("burst: {e}")),
    }

    // One knob at a time at its extremes: finite on a hot burst, and the
    // peak on the nominal programme.
    let short = noise_burst(secs(2.0, SR), secs(0.25, SR), db_to_lin(HOT_DBFS), 11);
    let nominal = &programme(SR)[..secs(2.0, SR)];
    report.extreme_peak_dbfs = f32::NEG_INFINITY;
    for variant in one_at_a_time(model, &defaults) {
        match run(&variant.params, SR, &short) {
            Ok(r) if r.all_finite() => {}
            Ok(_) => {
                report
                    .broken_at
                    .push(format!("{}: non-finite", variant.label));
                continue;
            }
            Err(e) => {
                report.broken_at.push(format!("{}: {e}", variant.label));
                continue;
            }
        }
        match run(&variant.params, SR, nominal) {
            Ok(r) if r.all_finite() => {
                let peak = worst(&r, peak_dbfs);
                if peak > report.extreme_peak_dbfs {
                    report.extreme_peak_dbfs = peak;
                    report.extreme_peak_at = variant.label;
                }
            }
            Ok(_) => report
                .broken_at
                .push(format!("{}: non-finite", variant.label)),
            Err(e) => report.broken_at.push(format!("{}: {e}", variant.label)),
        }
    }

    // Everything at max: runaway feedback / resonance.
    let long = noise_burst(secs(10.0, SR), secs(0.5, SR), db_to_lin(-12.0), 13);
    match run(&all_max(model, &defaults), SR, &long) {
        Ok(r) if r.all_finite() => {
            report.all_max_tail_dbfs = worst(&r, |c| tail_rms_dbfs(c, secs(0.5, SR)));
            let window = secs(0.5, SR);
            let loudest_before = worst(&r, |c| {
                c[..c.len() - window]
                    .chunks_exact(window)
                    .map(|w| tail_rms_dbfs(w, window))
                    .fold(f32::NEG_INFINITY, f32::max)
            });
            report.all_max_growth_db = report.all_max_tail_dbfs - loudest_before;
        }
        Ok(_) => report.broken_at.push("all-max: non-finite".into()),
        Err(e) => report.broken_at.push(format!("all-max: {e}")),
    }

    // Other sample rates.
    let mut broken_rates = Vec::new();
    let mut at = |sr: f32| {
        let input = programme(sr);
        match run(&defaults, sr, input) {
            Ok(r) => Some((r.all_finite(), level_db(&r, input, sr))),
            Err(e) => {
                broken_rates.push(format!("{sr} Hz: {e}"));
                None
            }
        }
    };
    let base = at(SR);
    if let (Some((true, l48)), Some((f96, l96))) = (base, at(96_000.0)) {
        report.sr96_level_delta_db = if f96 { l96 - l48 } else { f32::INFINITY };
        if !f96 {
            report.broken_at.push("96 kHz: non-finite".into());
        }
    }
    report.sr44_finite = matches!(at(44_100.0), Some((true, _)));
    report.broken_at.extend(broken_rates);
    if !report.sr44_finite {
        report.broken_at.push("44.1 kHz: non-finite".into());
    }
    report
}

/// Wall time to render 10 s at 48 kHz with default knobs, over 10 s.
/// Run on its own thread with nothing else measuring, or it lies.
pub fn cpu_rtf(model: &NativeModel) -> f32 {
    let Ok(defaults) = default_params(model) else {
        return f32::NAN;
    };
    let input = noise_burst(secs(10.0, SR), secs(10.0, SR), db_to_lin(-18.0), 17);
    let Ok(processor) = model.build(&defaults, SR) else {
        return f32::NAN;
    };
    let started = Instant::now();
    let rendered = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        super::render::run(processor, &input)
    }));
    let elapsed = started.elapsed().as_secs_f32();
    match rendered {
        Ok(r) => {
            std::hint::black_box(r);
            elapsed / 10.0
        }
        Err(_) => f32::NAN,
    }
}
