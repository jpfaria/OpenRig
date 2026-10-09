//! Responsibility: holds every native model to the objective quality bar (#1106).
//!
//! Each compiled-in model is built from its own schema and driven, offline,
//! with deterministic signals: a nominal 1 kHz tone (level, THD+N, DC), a
//! hot 5 kHz tone (aliasing), digital silence (self-noise), a noise burst
//! (tail), every knob alone at its extremes and all knobs at max (NaN/Inf,
//! runaway feedback), and 44.1 / 96 kHz (rate independence).
//!
//! The bars are in `native_quality/bars.rs`; every model meets them.
//! `NATIVE_QUALITY_REPORT=1` prints the per-model table.

mod native_quality;

use native_quality::bars::failures;
use native_quality::catalog::native_models;
use native_quality::measure::{cpu_rtf, measure, ModelReport};
use native_quality::report::table;

fn measure_all() -> Vec<ModelReport> {
    let models = native_models();
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let chunk = models.len().div_ceil(threads).max(1);
    let mut reports: Vec<ModelReport> = std::thread::scope(|scope| {
        models
            .chunks(chunk)
            .map(|batch| scope.spawn(move || batch.iter().map(measure).collect::<Vec<_>>()))
            .collect::<Vec<_>>()
            .into_iter()
            .flat_map(|h| h.join().expect("battery thread panicked"))
            .collect()
    });
    reports.sort_by(|a, b| a.family.cmp(&b.family).then(a.id.cmp(&b.id)));
    reports
}

#[test]
fn every_native_model_meets_the_quality_bar() {
    let reports = measure_all();
    assert!(
        reports.len() > 50,
        "only {} natives registered",
        reports.len()
    );
    if std::env::var_os("NATIVE_QUALITY_REPORT").is_some() {
        let cpu: Vec<(String, f32)> = native_models()
            .iter()
            .map(|m| (m.id.clone(), cpu_rtf(m)))
            .collect();
        println!("{}", table(&reports, &cpu));
    }
    let failing: Vec<String> = reports
        .iter()
        .filter_map(|r| {
            let f = failures(r);
            (!f.is_empty()).then(|| format!("{} ({}): {}", r.id, r.family, f.join("; ")))
        })
        .collect();
    assert!(
        failing.is_empty(),
        "{} native models miss the quality bar:\n{}",
        failing.len(),
        failing.join("\n")
    );
}
