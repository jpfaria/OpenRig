use crate::{CurveEditorPoint, MultiSliderPoint};
use project::block::schema_for_block_model;
use project::param::{CurveEditorRole, ParameterDomain, ParameterSet, ParameterWidget};

pub(crate) const BAND_COLORS: &[slint::Color] = &[
    slint::Color::from_argb_u8(255, 232, 77, 77),  // red
    slint::Color::from_argb_u8(255, 77, 184, 232), // cyan
    slint::Color::from_argb_u8(255, 119, 232, 77), // green
    slint::Color::from_argb_u8(255, 232, 184, 77), // orange
    slint::Color::from_argb_u8(255, 184, 77, 232), // purple
    slint::Color::from_argb_u8(255, 77, 232, 184), // teal
    slint::Color::from_argb_u8(255, 232, 77, 184), // pink
    slint::Color::from_argb_u8(255, 184, 232, 77), // lime
];

pub(crate) fn build_multi_slider_points(
    effect_type: &str,
    model_id: &str,
    params: &ParameterSet,
) -> Vec<MultiSliderPoint> {
    let Ok(schema) = schema_for_block_model(effect_type, model_id) else {
        return Vec::new();
    };
    schema
        .parameters
        .iter()
        .filter(|spec| matches!(spec.widget, ParameterWidget::MultiSlider))
        .map(|spec| {
            let current = params
                .get(&spec.path)
                .and_then(|v| v.as_f32())
                .or_else(|| spec.default_value.as_ref().and_then(|v| v.as_f32()))
                .unwrap_or(0.0);
            let (min, max, step) = match &spec.domain {
                ParameterDomain::FloatRange { min, max, step } => (*min, *max, *step),
                _ => (0.0, 1.0, 0.0),
            };
            MultiSliderPoint {
                path: spec.path.clone().into(),
                label: spec.label.clone().into(),
                value: current,
                min_val: min,
                max_val: max,
                step,
            }
        })
        .collect()
}

pub(crate) fn build_curve_editor_points(
    effect_type: &str,
    model_id: &str,
    params: &ParameterSet,
) -> Vec<CurveEditorPoint> {
    let Ok(schema) = schema_for_block_model(effect_type, model_id) else {
        return Vec::new();
    };

    // Group CurveEditor params by group name
    let mut groups: Vec<String> = Vec::new();
    for spec in &schema.parameters {
        if let ParameterWidget::CurveEditor { .. } = &spec.widget {
            let group = spec.group.clone().unwrap_or_default();
            if !groups.contains(&group) {
                groups.push(group);
            }
        }
    }

    groups
        .iter()
        .enumerate()
        .map(|(i, group)| {
            let band_color = BAND_COLORS[i % BAND_COLORS.len()];
            let mut point = CurveEditorPoint {
                group: group.clone().into(),
                band_color,
                y_path: "".into(),
                y_value: 0.0,
                y_min: 0.0,
                y_max: 0.0,
                y_step: 0.0,
                y_label: "".into(),
                has_x: false,
                x_path: "".into(),
                x_value: 0.0,
                x_min: 0.0,
                x_max: 0.0,
                x_step: 0.0,
                x_label: "".into(),
                has_width: false,
                width_path: "".into(),
                width_value: 0.0,
                width_min: 0.0,
                width_max: 0.0,
                width_step: 0.0,
            };

            for spec in &schema.parameters {
                let spec_group = spec.group.as_deref().unwrap_or("");
                if spec_group != group {
                    continue;
                }
                let ParameterWidget::CurveEditor { role } = &spec.widget else {
                    continue;
                };
                let current = params
                    .get(&spec.path)
                    .and_then(|v| v.as_f32())
                    .or_else(|| spec.default_value.as_ref().and_then(|v| v.as_f32()))
                    .unwrap_or(0.0);
                let (min, max, step) = match &spec.domain {
                    ParameterDomain::FloatRange { min, max, step } => (*min, *max, *step),
                    _ => (0.0, 1.0, 0.0),
                };
                match role {
                    CurveEditorRole::Y => {
                        point.y_path = spec.path.clone().into();
                        point.y_value = current;
                        point.y_min = min;
                        point.y_max = max;
                        point.y_step = step;
                    }
                    CurveEditorRole::X => {
                        point.has_x = true;
                        point.x_path = spec.path.clone().into();
                        point.x_value = current;
                        point.x_min = min;
                        point.x_max = max;
                        point.x_step = step;
                    }
                    CurveEditorRole::Width => {
                        point.has_width = true;
                        point.width_path = spec.path.clone().into();
                        point.width_value = current;
                        point.width_min = min;
                        point.width_max = max;
                        point.width_step = step;
                    }
                }
            }
            // Compute display labels
            point.y_label = if point.y_value >= 0.0 {
                format!("+{:.1}", point.y_value).into()
            } else {
                format!("{:.1}", point.y_value).into()
            };
            point.x_label = if point.has_x {
                if point.x_value >= 1000.0 {
                    format!("{:.1}k", point.x_value / 1000.0).into()
                } else {
                    format!("{}Hz", point.x_value as i32).into()
                }
            } else {
                "".into()
            };
            point
        })
        .collect()
}

/// Number of frequency points for EQ curve rendering (20Hz–20kHz).
pub(crate) const EQ_CURVE_POINTS: usize = 200;
/// Sample rate assumed for EQ visualization.
pub(crate) const EQ_VIZ_SAMPLE_RATE: f32 = 48_000.0;
/// SVG viewbox width (must match Slint CurveEditorControl viewbox).
pub(crate) const EQ_SVG_W: f32 = 1000.0;
/// SVG viewbox height.
pub(crate) const EQ_SVG_H: f32 = 200.0;
/// Frequency range.
pub(crate) const EQ_FREQ_MIN: f32 = 20.0;
pub(crate) const EQ_FREQ_MAX: f32 = 20_000.0;
/// Gain range in dB (symmetric around 0).
pub(crate) const EQ_GAIN_MIN: f32 = -24.0;
pub(crate) const EQ_GAIN_MAX: f32 = 24.0;

pub(crate) fn freq_to_x(freq: f32) -> f32 {
    let norm = (freq / EQ_FREQ_MIN).log(EQ_FREQ_MAX / EQ_FREQ_MIN);
    (norm.clamp(0.0, 1.0) * EQ_SVG_W).round()
}

pub(crate) fn gain_to_y(gain_db: f32) -> f32 {
    let norm = 1.0 - (gain_db - EQ_GAIN_MIN) / (EQ_GAIN_MAX - EQ_GAIN_MIN);
    (norm.clamp(0.0, 1.0) * EQ_SVG_H).round()
}

pub(crate) fn db_to_linear(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}
pub(crate) fn linear_to_db(lin: f32) -> f32 {
    20.0 * lin.max(1e-10).log10()
}

pub(crate) fn biquad_kind_for_group(group: &str) -> block_core::BiquadKind {
    let lower = group.to_lowercase();
    if lower.contains("low") {
        block_core::BiquadKind::LowShelf
    } else if lower.contains("high") {
        block_core::BiquadKind::HighShelf
    } else {
        block_core::BiquadKind::Peak
    }
}

/// Log-spaced frequency points for the curve.
pub(crate) fn eq_frequencies() -> Vec<f32> {
    (0..EQ_CURVE_POINTS)
        .map(|i| {
            let t = i as f32 / (EQ_CURVE_POINTS - 1) as f32;
            EQ_FREQ_MIN * (EQ_FREQ_MAX / EQ_FREQ_MIN).powf(t)
        })
        .collect()
}

pub(crate) fn db_vec_to_svg_path(dbs: &[f32]) -> String {
    let freqs = eq_frequencies();
    let mut path = String::with_capacity(dbs.len() * 12);
    for (i, (&db, &freq)) in dbs.iter().zip(freqs.iter()).enumerate() {
        let x = freq_to_x(freq);
        let y = gain_to_y(db);
        if i == 0 {
            path.push_str(&format!("M {x} {y}"));
        } else {
            path.push_str(&format!(" L {x} {y}"));
        }
    }
    path
}

/// Compute band and total SVG path strings for CurveEditor EQ blocks.
/// Returns (total_curve, band_curves).
pub(crate) fn compute_eq_curves(
    effect_type: &str,
    model_id: &str,
    params: &ParameterSet,
) -> (String, Vec<String>) {
    let Ok(schema) = schema_for_block_model(effect_type, model_id) else {
        return (String::new(), Vec::new());
    };

    // Collect groups in order
    let mut groups: Vec<String> = Vec::new();
    for spec in &schema.parameters {
        if let ParameterWidget::CurveEditor { .. } = &spec.widget {
            let group = spec.group.clone().unwrap_or_default();
            if !groups.contains(&group) {
                groups.push(group);
            }
        }
    }
    if groups.is_empty() {
        return (String::new(), Vec::new());
    }

    let freqs = eq_frequencies();
    let mut total_linear = vec![1.0_f32; EQ_CURVE_POINTS];
    let mut band_paths = Vec::with_capacity(groups.len());

    for group in &groups {
        // Extract Y (gain), X (freq), Width (Q) for this group
        let mut gain_db = 0.0_f32;
        let mut freq_hz = 1000.0_f32;
        let mut q = 1.0_f32;

        for spec in &schema.parameters {
            if spec.group.as_deref().unwrap_or("") != group {
                continue;
            }
            let ParameterWidget::CurveEditor { role } = &spec.widget else {
                continue;
            };
            let val = params
                .get(&spec.path)
                .and_then(|v| v.as_f32())
                .or_else(|| spec.default_value.as_ref().and_then(|v| v.as_f32()))
                .unwrap_or(0.0);
            match role {
                CurveEditorRole::Y => gain_db = val,
                CurveEditorRole::X => freq_hz = val,
                CurveEditorRole::Width => q = val,
            }
        }

        let kind = biquad_kind_for_group(group);
        let filter =
            block_core::BiquadFilter::new(kind, freq_hz, gain_db, q.max(0.01), EQ_VIZ_SAMPLE_RATE);

        let band_dbs: Vec<f32> = freqs
            .iter()
            .map(|&f| filter.magnitude_db(f, EQ_VIZ_SAMPLE_RATE))
            .collect();

        // Accumulate linear magnitudes for total curve
        for (lin, &db) in total_linear.iter_mut().zip(band_dbs.iter()) {
            *lin *= db_to_linear(db);
        }

        band_paths.push(db_vec_to_svg_path(&band_dbs));
    }

    let total_dbs: Vec<f32> = total_linear.iter().map(|&lin| linear_to_db(lin)).collect();
    let total_path = db_vec_to_svg_path(&total_dbs);

    (total_path, band_paths)
}

#[cfg(test)]
#[path = "eq_tests.rs"]
mod tests;
