//! Responsibility: decides which battery numbers fail the quality bar (#1106).

use super::measure::ModelReport;

/// Mean offset a model may leave on the nominal tone, dBFS.
pub const MAX_DC_DBFS: f32 = -60.0;
/// Non-harmonic energy a saturating model may make from the hot tone, dB.
pub const MAX_ALIASING_DB: f32 = -60.0;
/// How far a non-saturating model may move the nominal level at default
/// knobs, dB either way.
pub const MAX_LEVEL_SHIFT_DB: f32 = 3.0;
/// How far the level at 96 kHz may drift from 48 kHz, dB either way.
pub const MAX_RATE_DRIFT_DB: f32 = 1.0;
/// Loudest peak any single knob extreme may reach on the nominal
/// programme, dBFS.
pub const MAX_EXTREME_PEAK_DBFS: f32 = 6.0;
/// Self-noise with silence in, dBFS.
pub const MAX_SILENCE_DBFS: f32 = -90.0;
/// What is left 11.5 s after a burst at default knobs, dBFS.
pub const MAX_TAIL_DBFS: f32 = -60.0;

/// How far a model whose DSP #1106 rebuilt may move from the loudness the
/// release measured at default knobs, dB either way.
pub const MAX_RELEASE_LEVEL_DRIFT_DB: f32 = 1.0;

/// How far the fully dry setting (`mix` at minimum) may sit from the
/// input, dB either way: dry is bypass.
pub const MAX_DRY_LEVEL_DB: f32 = 0.5;

/// Families judged on the guitar DI: their gain follows the spectrum, and
/// a guitar's mid-heavy spectrum is what they are voiced for.
const GUITAR_LEVEL_FAMILIES: &[&str] = &["Wah"];

/// Families judged on their passband gain: their default cuts content on
/// purpose, so the full-range programme level would count the cut itself.
const PASSBAND_LEVEL_FAMILIES: &[&str] = &["Filter"];

/// Default-knob programme level of the models whose DSP #1106 rebuilt, as
/// the release measured it, dB. A rebuild keeps the user's volume.
const RELEASE_LEVEL_DB: &[(&str, f32)] = &[
    ("blackface_clean", 8.7),
    ("chime", 8.9),
    ("tweed_breakup", 12.4),
    ("american_clean", 12.0),
    ("brit_crunch", 16.6),
    ("modern_high_gain", 18.1),
    ("fuzz_ge", 14.5),
    ("fuzz_si", 15.3),
    ("half_wave_rectifier", 9.1),
    ("tape_saturation", 4.3),
    ("transformer_saturation", 5.2),
    ("tube_saturation", 15.0),
    ("wavefolder", 8.6),
];

/// The same models' default-knob level on the guitar DI, as the release
/// measured it, dB. Saturation is level-dependent, so the quieter real DI
/// is pinned too.
const RELEASE_GUITAR_LEVEL_DB: &[(&str, f32)] = &[
    ("blackface_clean", 10.5),
    ("chime", 11.5),
    ("tweed_breakup", 16.3),
    ("american_clean", 13.5),
    ("brit_crunch", 19.9),
    ("modern_high_gain", 22.6),
    ("fuzz_ge", 24.1),
    ("fuzz_si", 27.0),
    ("half_wave_rectifier", 12.2),
    ("tape_saturation", 5.3),
    ("transformer_saturation", 6.5),
    ("tube_saturation", 18.8),
    ("wavefolder", 10.4),
];

/// Saturating models whose non-harmonic output is the effect itself.
const ALIASING_IS_THE_EFFECT: &[(&str, &str)] = &[
    ("bitcrusher", "sample-rate reduction folds by design"),
    (
        "sub_octave",
        "the octave below is not a harmonic of the input",
    ),
];

/// Every bar `report` misses, one line each.
pub fn failures(report: &ModelReport) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(e) = &report.build_error {
        out.push(format!("does not run: {e}"));
        return out;
    }
    out.extend(report.broken_at.iter().map(|b| format!("broken at {b}")));
    let mut over = |what: &str, value: f32, bar: f32| {
        if value.is_nan() || value > bar {
            out.push(format!("{what} {value:.1} > {bar:.1}"));
        }
    };
    over("self-noise dBFS", report.silence_dbfs, MAX_SILENCE_DBFS);
    over("tail dBFS", report.tail_dbfs, MAX_TAIL_DBFS);
    over("DC dBFS", report.dc_dbfs, MAX_DC_DBFS);
    over("all-max growth dB", report.all_max_growth_db, 0.0);
    over(
        &format!("extreme peak dBFS ({})", report.extreme_peak_at),
        report.extreme_peak_dbfs,
        MAX_EXTREME_PEAK_DBFS,
    );
    over(
        "96 kHz level drift dB",
        report.sr96_level_delta_db.abs(),
        MAX_RATE_DRIFT_DB,
    );
    if let Some((_, release)) = RELEASE_LEVEL_DB.iter().find(|(id, _)| *id == report.id) {
        over(
            "level drift from release dB",
            (report.level_db - release).abs(),
            MAX_RELEASE_LEVEL_DRIFT_DB,
        );
    }
    if let Some((_, release)) = RELEASE_GUITAR_LEVEL_DB
        .iter()
        .find(|(id, _)| *id == report.id)
    {
        over(
            "guitar level drift from release dB",
            (report.guitar_level_db - release).abs(),
            MAX_RELEASE_LEVEL_DRIFT_DB,
        );
    }
    if !report.dry_level_db.is_nan() {
        over(
            "dry (mix=min) level dB",
            report.dry_level_db.abs(),
            MAX_DRY_LEVEL_DB,
        );
    }
    if report.nonlinear {
        if !ALIASING_IS_THE_EFFECT
            .iter()
            .any(|(id, _)| *id == report.id)
        {
            over("aliasing dB", report.inharmonic_db, MAX_ALIASING_DB);
        }
    } else {
        let family = report.family.as_str();
        let level = if PASSBAND_LEVEL_FAMILIES.contains(&family) {
            report.tone_level_db
        } else if GUITAR_LEVEL_FAMILIES.contains(&family) {
            report.guitar_level_db
        } else {
            report.level_db
        };
        over("default level shift dB", level.abs(), MAX_LEVEL_SHIFT_DB);
    }
    out
}
