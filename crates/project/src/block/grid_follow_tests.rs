use super::*;
use domain::value_objects::ParameterValue as P;
use plugin_loader::manifest::ParameterValue as M;
use std::collections::BTreeMap;

fn axis(name: &str, values: Vec<M>) -> GridParameter {
    GridParameter {
        name: name.into(),
        display_name: None,
        values,
    }
}

fn capture(values: &[(&str, M)], file: &str) -> GridCapture {
    GridCapture {
        values: values
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect::<BTreeMap<_, _>>(),
        file: file.into(),
        output_gain_db: None,
        noise_gate: None,
    }
}

fn params(values: &[(&str, P)]) -> ParameterSet {
    let mut set = ParameterSet::default();
    for (k, v) in values {
        set.insert(*k, v.clone());
    }
    set
}

fn text(s: &str) -> M {
    M::Text(s.into())
}

/// The installed grid: one `preset` axis naming each capture.
fn preset_grid() -> (Vec<GridParameter>, Vec<GridCapture>) {
    (
        vec![axis("preset", vec![text("Cap"), text("Cone")])],
        vec![
            capture(&[("preset", text("Cap"))], "captures/000.wav"),
            capture(&[("preset", text("Cone"))], "captures/001.wav"),
        ],
    )
}

/// The same captures after the user named them: a `mic` choice and a
/// `distance` knob.
fn named_grid() -> (Vec<GridParameter>, Vec<GridCapture>) {
    (
        vec![
            axis("mic", vec![text("sm57")]),
            axis("distance", vec![M::Number(0.0), M::Number(2.0)]),
        ],
        vec![
            capture(
                &[("mic", text("sm57")), ("distance", M::Number(0.0))],
                "captures/000.wav",
            ),
            capture(
                &[("mic", text("sm57")), ("distance", M::Number(2.0))],
                "captures/001.wav",
            ),
        ],
    )
}

#[test]
fn a_block_follows_its_capture_file_into_the_new_grid() {
    let (from_p, from_c) = preset_grid();
    let (to_p, to_c) = named_grid();
    let block = params(&[
        ("preset", P::String("Cone".into())),
        ("output_db", P::Float(-3.0)),
    ]);

    let moved = follow_capture(&from_p, &from_c, &to_p, &to_c, &block).unwrap();

    assert_eq!(moved.get("preset"), None, "the old axis is gone");
    assert_eq!(moved.get("distance"), Some(&P::Float(2.0)));
    assert_eq!(moved.get("mic"), Some(&P::String("sm57".into())));
    assert_eq!(
        moved.get("output_db"),
        Some(&P::Float(-3.0)),
        "controls that are not capture axes stay as the user set them"
    );
}

#[test]
fn a_block_whose_file_left_the_grid_does_not_follow() {
    let (from_p, from_c) = preset_grid();
    let (to_p, mut to_c) = named_grid();
    to_c.retain(|c| c.file.to_str() != Some("captures/001.wav"));
    let block = params(&[("preset", P::String("Cone".into()))]);

    assert!(follow_capture(&from_p, &from_c, &to_p, &to_c, &block).is_none());
}

#[test]
fn a_switch_axis_follows_as_a_switch() {
    let from_p = vec![axis("preset", vec![text("Off"), text("On")])];
    let from_c = vec![
        capture(&[("preset", text("Off"))], "a.nam"),
        capture(&[("preset", text("On"))], "b.nam"),
    ];
    let to_p = vec![axis("boost", vec![M::Bool(false), M::Bool(true)])];
    let to_c = vec![
        capture(&[("boost", M::Bool(false))], "a.nam"),
        capture(&[("boost", M::Bool(true))], "b.nam"),
    ];
    let block = params(&[("preset", P::String("On".into()))]);

    let moved = follow_capture(&from_p, &from_c, &to_p, &to_c, &block).unwrap();
    assert_eq!(moved.get("boost"), Some(&P::Bool(true)));
}

#[test]
fn numbers_on_a_choice_axis_follow_as_their_text() {
    let (from_p, from_c) = preset_grid();
    let to_p = vec![axis("channel", vec![M::Number(1.0), text("lead")])];
    let to_c = vec![
        capture(&[("channel", M::Number(1.0))], "captures/000.wav"),
        capture(&[("channel", text("lead"))], "captures/001.wav"),
    ];
    let block = params(&[("preset", P::String("Cap".into()))]);

    let moved = follow_capture(&from_p, &from_c, &to_p, &to_c, &block).unwrap();
    assert_eq!(moved.get("channel"), Some(&P::String("1".into())));
}

#[test]
fn exact_capture_finds_the_capture_the_values_name() {
    let (p, c) = named_grid();
    let block = params(&[
        ("mic", P::String("sm57".into())),
        ("distance", P::Float(2.0)),
    ]);
    assert_eq!(
        exact_capture(&p, &c, &block).map(|c| c.file.clone()),
        Some("captures/001.wav".into())
    );
}

#[test]
fn exact_capture_refuses_values_of_another_grid() {
    let (p, c) = named_grid();
    let block = params(&[("preset", P::String("Cone".into()))]);
    assert!(exact_capture(&p, &c, &block).is_none());
}

#[test]
fn exact_capture_refuses_a_value_between_two_captures() {
    let (p, c) = named_grid();
    let block = params(&[
        ("mic", P::String("sm57".into())),
        ("distance", P::Float(1.0)),
    ]);
    assert!(exact_capture(&p, &c, &block).is_none());
}

#[test]
fn an_axis_with_one_value_is_not_asked_for() {
    // `mic` holds a single value, so the block never stored it.
    let (p, c) = named_grid();
    let block = params(&[("distance", P::Float(0.0))]);
    assert_eq!(
        exact_capture(&p, &c, &block).map(|c| c.file.clone()),
        Some("captures/000.wav".into())
    );
}
