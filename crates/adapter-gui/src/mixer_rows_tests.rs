//! #1007: the strips the panel draws, split by side and pre-formatted.

use application::mixer_view::MixerStripView;
use domain::mixer_strip::MixerDirection;

use super::*;
use crate::mixer_fader_law::position_from_db;

fn view(
    id: &str,
    direction: MixerDirection,
    channels: Vec<usize>,
    gain_db: f32,
    muted: bool,
) -> MixerStripView {
    MixerStripView {
        id: id.into(),
        direction,
        name: format!("{id} name"),
        device_id: "dev".into(),
        channels,
        gain_db,
        muted,
        soloed: false,
    }
}

#[test]
fn strips_split_by_side_in_order() {
    let (inputs, outputs) = mixer_rows(&[
        view("in:0@dev", MixerDirection::Input, vec![0], 0.0, false),
        view(
            "out:0,1@dev",
            MixerDirection::Output,
            vec![0, 1],
            -6.0,
            true,
        ),
        view("in:1@dev", MixerDirection::Input, vec![1], 0.0, false),
    ]);
    let ids = |rows: &[MixerStripRow]| rows.iter().map(|r| r.id.to_string()).collect::<Vec<_>>();
    assert_eq!(ids(&inputs), vec!["in:0@dev", "in:1@dev"]);
    assert_eq!(ids(&outputs), vec!["out:0,1@dev"]);
    assert!(inputs.iter().all(|r| r.is_input));
    assert!(!outputs[0].is_input);
}

#[test]
fn a_row_carries_the_setting_pre_computed() {
    let (_, outputs) = mixer_rows(&[view(
        "out:0,1@dev",
        MixerDirection::Output,
        vec![0, 1],
        -6.0,
        true,
    )]);
    let row = &outputs[0];
    assert_eq!(row.name.as_str(), "out:0,1@dev name");
    assert_eq!(row.gain_label.as_str(), "-6.0 dB");
    assert!((row.position - position_from_db(-6.0)).abs() < 1e-6);
    assert!(row.muted);
}

#[test]
fn the_detail_names_the_side_and_channels_one_based() {
    let (inputs, outputs) = mixer_rows(&[
        view("in:14@dev", MixerDirection::Input, vec![14], 0.0, false),
        view(
            "out:0,1@dev",
            MixerDirection::Output,
            vec![0, 1],
            0.0,
            false,
        ),
        view(
            "out:0,2@dev",
            MixerDirection::Output,
            vec![0, 2],
            0.0,
            false,
        ),
    ]);
    // #1007 review: the user could not tell inputs from outputs — the side
    // travels with the channels, like the chain meters (#1006).
    assert_eq!(inputs[0].detail.as_str(), "IN 15");
    assert_eq!(outputs[0].detail.as_str(), "OUT 1,2");
    assert_eq!(outputs[1].detail.as_str(), "OUT 1,3");
}

fn soloed(mut strip: MixerStripView) -> MixerStripView {
    strip.soloed = true;
    strip
}

#[test]
fn a_soloed_strip_shows_its_solo_and_dims_the_rest_of_its_side() {
    let (inputs, outputs) = mixer_rows(&[
        view("in:0@dev", MixerDirection::Input, vec![0], 0.0, false),
        soloed(view(
            "out:0,1@dev",
            MixerDirection::Output,
            vec![0, 1],
            0.0,
            false,
        )),
        view(
            "out:2,3@dev",
            MixerDirection::Output,
            vec![2, 3],
            0.0,
            false,
        ),
    ]);
    assert!(outputs[0].soloed && !outputs[0].solo_silenced);
    assert!(!outputs[1].soloed && outputs[1].solo_silenced);
    assert!(
        !inputs[0].solo_silenced,
        "an output solo leaves the inputs alone"
    );
}
