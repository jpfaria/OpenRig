//! #328 spec §1.2 / §7 "Engine math": the split's input stage and the
//! mixer's per-sample law, on plain numbers.

use super::{mix_frame, pan_gains, split_inputs, SplitKnobValues};

fn ampero_defaults() -> SplitKnobValues {
    SplitKnobValues {
        dual_mono: false,
        level_to_a: 1.0,
        level_to_b: 1.0,
        balance_a: 0.0,
        balance_b: 0.0,
        mix_level_a: 1.0,
        mix_level_b: 1.0,
        mix_pan_a: 0.0,
        mix_pan_b: 0.0,
        mix_b_invert: false,
        mix_master: 0.5,
        mix_master_sum: false,
    }
}

fn assert_close(got: [f32; 2], want: [f32; 2]) {
    assert!(
        (got[0] - want[0]).abs() < 1e-6 && (got[1] - want[1]).abs() < 1e-6,
        "got {got:?}, want {want:?}"
    );
}

#[test]
fn centre_pan_is_unity_on_both_sides() {
    assert_eq!(pan_gains(0.0), (1.0, 1.0));
}

#[test]
fn hard_pans_silence_the_opposite_side() {
    assert_eq!(pan_gains(-50.0), (1.0, 0.0), "hard left keeps L only");
    assert_eq!(pan_gains(50.0), (0.0, 1.0), "hard right keeps R only");
}

#[test]
fn the_opposite_side_falls_linearly() {
    let (l, r) = pan_gains(25.0);
    assert!((l - 0.5).abs() < 1e-6 && r == 1.0, "pan +25 → ({l}, {r})");
    let (l, r) = pan_gains(-10.0);
    assert!(l == 1.0 && (r - 0.8).abs() < 1e-6, "pan -10 → ({l}, {r})");
}

#[test]
fn identical_paths_at_the_defaults_come_out_at_unity() {
    let knobs = ampero_defaults();
    let input = [0.3, -0.2];
    let (a, b) = split_inputs(input, &knobs);
    assert_eq!(
        (a, b),
        (input, input),
        "Mode I at level 100 feeds both paths the bus"
    );
    assert_eq!(
        mix_frame(a, b, &knobs),
        input,
        "master 50 halves the doubled sum"
    );
}

#[test]
fn hard_pans_keep_each_path_on_its_side() {
    let knobs = SplitKnobValues {
        mix_pan_a: -50.0,
        mix_pan_b: 50.0,
        mix_master: 1.0,
        ..ampero_defaults()
    };
    assert_eq!(
        mix_frame([0.4, 0.4], [0.0, 0.0], &knobs),
        [0.4, 0.0],
        "path A only on L"
    );
    assert_eq!(
        mix_frame([0.0, 0.0], [0.7, 0.7], &knobs),
        [0.0, 0.7],
        "path B only on R"
    );
}

#[test]
fn inverted_b_cancels_an_identical_path() {
    let knobs = SplitKnobValues {
        mix_b_invert: true,
        ..ampero_defaults()
    };
    assert_eq!(mix_frame([0.3, -0.2], [0.3, -0.2], &knobs), [0.0, 0.0]);
}

#[test]
fn master_sum_folds_the_output_to_dual_mono() {
    let knobs = SplitKnobValues {
        mix_master: 1.0,
        mix_master_sum: true,
        ..ampero_defaults()
    };
    assert_eq!(mix_frame([0.8, 0.0], [0.0, 0.0], &knobs), [0.4, 0.4]);
}

#[test]
fn mode_ii_feeds_each_path_the_channel_its_balance_picks() {
    let knobs = SplitKnobValues {
        dual_mono: true,
        balance_a: -50.0,
        balance_b: 50.0,
        ..ampero_defaults()
    };
    assert_eq!(split_inputs([0.9, 0.1], &knobs), ([0.9, 0.9], [0.1, 0.1]));
    let centre = SplitKnobValues {
        dual_mono: true,
        ..ampero_defaults()
    };
    assert_close(split_inputs([0.9, 0.1], &centre).0, [0.5, 0.5]);
}

#[test]
fn levels_scale_what_enters_and_leaves_each_path() {
    let knobs = SplitKnobValues {
        level_to_b: 0.5,
        mix_level_a: 0.0,
        mix_master: 1.0,
        ..ampero_defaults()
    };
    let (a, b) = split_inputs([0.8, 0.4], &knobs);
    assert_eq!(a, [0.8, 0.4]);
    assert_eq!(b, [0.4, 0.2]);
    assert_eq!(
        mix_frame(a, b, &knobs),
        [0.4, 0.2],
        "mix level 0 mutes path A"
    );
}

#[test]
fn y_meets_its_paths_at_unity() {
    let knobs = SplitKnobValues {
        mix_pan_a: -50.0,
        mix_b_invert: true,
        mix_master: 0.1,
        mix_master_sum: true,
        ..ampero_defaults()
    }
    .with_neutral_mixer();
    assert_close(mix_frame([0.3, 0.1], [0.2, 0.4], &knobs), [0.5, 0.5]);
}
