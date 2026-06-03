use super::*;
use crate::manifest::ParameterValue as V;
use std::collections::BTreeMap;

/// Capture with numeric axis values.
fn ncap(values: &[(&str, f64)], file: &str) -> GridCapture {
    GridCapture {
        values: values
            .iter()
            .map(|(k, v)| ((*k).to_string(), V::Number(*v)))
            .collect(),
        file: file.into(),
        output_gain_db: None,
    }
}

/// Capture with text axis values.
fn tcap(values: &[(&str, &str)], file: &str) -> GridCapture {
    GridCapture {
        values: values
            .iter()
            .map(|(k, v)| ((*k).to_string(), V::Text((*v).to_string())))
            .collect(),
        file: file.into(),
        output_gain_db: None,
    }
}

fn axis(name: &str, values: Vec<V>) -> GridParameter {
    GridParameter {
        name: name.into(),
        display_name: None,
        values,
    }
}

#[test]
fn drops_single_value_axis() {
    // mesa_boogie_dc_5: one `preset` axis with a single declared value and a
    // single capture — nothing to choose. The axis must not be rendered.
    let parameters = vec![axis("preset", vec![V::Text("black_shadow_sm57".into())])];
    let captures = vec![tcap(&[("preset", "black_shadow_sm57")], "bs.nam")];
    assert!(effective_grid_axes(&parameters, &captures).is_empty());
}

#[test]
fn keeps_only_capture_backed_values() {
    // analog_man_sun_face: `compression` declares 0..=5 but only 3 and 5 have
    // a backing capture. The rendered axis keeps just those two, in declared
    // order; the unbacked declared values are dropped.
    let parameters = vec![axis(
        "compression",
        (0..=5).map(|n| V::Number(f64::from(n))).collect(),
    )];
    let captures = vec![
        ncap(&[("compression", 3.0)], "c3.nam"),
        ncap(&[("compression", 5.0)], "c5.nam"),
    ];
    let axes = effective_grid_axes(&parameters, &captures);
    assert_eq!(axes.len(), 1);
    assert_eq!(
        axes[0].values,
        vec![V::Number(3.0), V::Number(5.0)],
        "axis must keep only capture-backed values, in declared order"
    );
}

#[test]
fn drops_overdeclared_axis_with_single_backed_value() {
    // An over-declared axis where only ONE declared value is captured is still
    // a dead control — drop it like the single-value case.
    let parameters = vec![axis(
        "volume",
        (0..=10).map(|n| V::Number(f64::from(n))).collect(),
    )];
    let captures = vec![ncap(&[("volume", 7.0)], "v7.nam")];
    assert!(effective_grid_axes(&parameters, &captures).is_empty());
}

#[test]
fn keeps_full_multi_value_axis() {
    // A genuine multi-value axis: 4 mic positions, all captured. All four must
    // survive.
    let parameters = vec![axis(
        "mic",
        vec![
            V::Text("sm57".into()),
            V::Text("md421".into()),
            V::Text("r121".into()),
            V::Text("u87".into()),
        ],
    )];
    let captures = vec![
        tcap(&[("mic", "sm57")], "a.nam"),
        tcap(&[("mic", "md421")], "b.nam"),
        tcap(&[("mic", "r121")], "c.nam"),
        tcap(&[("mic", "u87")], "d.nam"),
    ];
    let axes = effective_grid_axes(&parameters, &captures);
    assert_eq!(axes.len(), 1);
    assert_eq!(axes[0].values.len(), 4);
}

#[test]
fn keeps_live_axis_and_drops_dead_one_in_multi_axis() {
    // Multi-axis plugin: `gain` has 2 backed values (kept), `preset` has 1
    // (dropped). Only the live axis survives.
    let parameters = vec![
        axis("gain", vec![V::Number(10.0), V::Number(20.0)]),
        axis("preset", vec![V::Text("only".into())]),
    ];
    let captures = vec![
        cell(
            &[
                ("gain", V::Number(10.0)),
                ("preset", V::Text("only".into())),
            ],
            "g10.nam",
        ),
        cell(
            &[
                ("gain", V::Number(20.0)),
                ("preset", V::Text("only".into())),
            ],
            "g20.nam",
        ),
    ];
    let axes = effective_grid_axes(&parameters, &captures);
    assert_eq!(axes.len(), 1);
    assert_eq!(axes[0].name, "gain");
}

/// Multi-axis capture cell with mixed value types.
fn cell(values: &[(&str, V)], file: &str) -> GridCapture {
    let map: BTreeMap<String, V> = values
        .iter()
        .map(|(k, v)| ((*k).to_string(), v.clone()))
        .collect();
    GridCapture {
        values: map,
        file: file.into(),
        output_gain_db: None,
    }
}
