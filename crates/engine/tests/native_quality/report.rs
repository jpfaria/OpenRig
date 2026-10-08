//! Responsibility: prints the battery results as a per-model table.

use super::measure::ModelReport;

fn f(x: f32) -> String {
    if x.is_nan() {
        "—".into()
    } else if x.is_infinite() {
        if x > 0.0 {
            "+inf".into()
        } else {
            "-inf".into()
        }
    } else {
        format!("{x:.1}")
    }
}

/// The family, marked `nl` when saturating and `tv` when time-varying.
fn family_label(r: &ModelReport) -> String {
    match (r.nonlinear, r.time_varying) {
        (true, _) => format!("{} nl", r.family),
        (_, true) => format!("{} tv", r.family),
        _ => r.family.clone(),
    }
}

/// Markdown table, one row per model.
pub fn table(reports: &[ModelReport], cpu: &[(String, f32)]) -> String {
    let mut out = String::from(
        "| model | family | level dB | THD+N dB | DC dBFS | inharm. dB | silence dBFS | tail dBFS | extreme peak dBFS (at) | all-max tail dBFS | 96k Δ dB | CPU % | broken |\n\
         |---|---|---|---|---|---|---|---|---|---|---|---|---|\n",
    );
    for r in reports {
        let cpu_pct = cpu
            .iter()
            .find(|(id, _)| id == &r.id)
            .map(|(_, rtf)| f(rtf * 100.0))
            .unwrap_or_else(|| "—".into());
        let broken = match &r.build_error {
            Some(e) => format!("BUILD: {e}"),
            None => r.broken_at.join("; "),
        };
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} ({}) | {} | {} | {} | {} |\n",
            r.id,
            family_label(r),
            f(r.level_db),
            f(r.thd_n_db),
            f(r.dc_dbfs),
            f(r.inharmonic_db),
            f(r.silence_dbfs),
            f(r.tail_dbfs),
            f(r.extreme_peak_dbfs),
            r.extreme_peak_at,
            f(r.all_max_tail_dbfs),
            f(r.sr96_level_delta_db),
            cpu_pct,
            broken,
        ));
    }
    out
}
