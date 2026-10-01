//! #328 spec §1.2 / §7 "Engine math" / §11.2: the split's input stage and the
//! mixer's per-sample law, on plain numbers, for any number of paths.

use super::{accumulate_path, finish_mix, pan_gains, path_input, MixKnobs, PathKnobs};

fn ampero_mix() -> MixKnobs {
    MixKnobs {
        dual_mono: false,
        master: 0.5,
        master_sum: false,
    }
}

fn unity_mix() -> MixKnobs {
    MixKnobs {
        master: 1.0,
        ..ampero_mix()
    }
}

/// What the mixer outputs for `outs`, one per path, under `paths` and `mix`.
fn mixed(outs: &[[f32; 2]], paths: &[PathKnobs], mix: &MixKnobs) -> [f32; 2] {
    let mut acc = [0.0, 0.0];
    for (out, path) in outs.iter().zip(paths) {
        accumulate_path(&mut acc, *out, path);
    }
    finish_mix(acc, mix)
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
    let paths = [PathKnobs::neutral(); 2];
    let mix = ampero_mix();
    let input = [0.3, -0.2];
    let outs: Vec<[f32; 2]> = paths.iter().map(|p| path_input(input, p, &mix)).collect();
    assert_eq!(
        outs,
        vec![input, input],
        "Mode I at level 100 feeds every path the bus"
    );
    assert_eq!(
        mixed(&outs, &paths, &mix),
        input,
        "master 50 halves the doubled sum"
    );
}

#[test]
fn three_paths_sum_on_one_master() {
    let paths = [PathKnobs::neutral(); 3];
    assert_close(
        mixed(&[[0.1, 0.2], [0.2, 0.1], [0.3, 0.3]], &paths, &ampero_mix()),
        [0.3, 0.3],
    );
}

#[test]
fn hard_pans_keep_each_path_on_its_side() {
    let paths = [
        PathKnobs {
            mix_pan: -50.0,
            ..PathKnobs::neutral()
        },
        PathKnobs {
            mix_pan: 50.0,
            ..PathKnobs::neutral()
        },
    ];
    assert_eq!(
        mixed(&[[0.4, 0.4], [0.0, 0.0]], &paths, &unity_mix()),
        [0.4, 0.0],
        "path A only on L"
    );
    assert_eq!(
        mixed(&[[0.0, 0.0], [0.7, 0.7]], &paths, &unity_mix()),
        [0.0, 0.7],
        "path B only on R"
    );
}

#[test]
fn an_inverted_path_cancels_an_identical_one() {
    let paths = [
        PathKnobs::neutral(),
        PathKnobs {
            invert: true,
            ..PathKnobs::neutral()
        },
    ];
    assert_eq!(
        mixed(&[[0.3, -0.2], [0.3, -0.2]], &paths, &ampero_mix()),
        [0.0, 0.0]
    );
}

#[test]
fn polarity_is_per_path() {
    let paths = [
        PathKnobs::neutral(),
        PathKnobs::neutral(),
        PathKnobs {
            invert: true,
            ..PathKnobs::neutral()
        },
    ];
    assert_close(
        mixed(&[[0.2, 0.2], [0.2, 0.2], [0.2, 0.2]], &paths, &unity_mix()),
        [0.2, 0.2],
    );
}

#[test]
fn master_sum_folds_the_output_to_dual_mono() {
    let mix = MixKnobs {
        master_sum: true,
        ..unity_mix()
    };
    assert_eq!(
        mixed(&[[0.8, 0.0], [0.0, 0.0]], &[PathKnobs::neutral(); 2], &mix),
        [0.4, 0.4]
    );
}

#[test]
fn mode_ii_feeds_each_path_the_channel_its_balance_picks() {
    let mix = MixKnobs {
        dual_mono: true,
        ..ampero_mix()
    };
    let left = PathKnobs {
        balance: -50.0,
        ..PathKnobs::neutral()
    };
    let right = PathKnobs {
        balance: 50.0,
        ..PathKnobs::neutral()
    };
    assert_eq!(path_input([0.9, 0.1], &left, &mix), [0.9, 0.9]);
    assert_eq!(path_input([0.9, 0.1], &right, &mix), [0.1, 0.1]);
    assert_close(
        path_input([0.9, 0.1], &PathKnobs::neutral(), &mix),
        [0.5, 0.5],
    );
}

#[test]
fn levels_scale_what_enters_and_leaves_each_path() {
    let paths = [
        PathKnobs {
            mix_level: 0.0,
            ..PathKnobs::neutral()
        },
        PathKnobs {
            level_to: 0.5,
            ..PathKnobs::neutral()
        },
    ];
    let mix = unity_mix();
    let a = path_input([0.8, 0.4], &paths[0], &mix);
    let b = path_input([0.8, 0.4], &paths[1], &mix);
    assert_eq!(a, [0.8, 0.4]);
    assert_eq!(b, [0.4, 0.2]);
    assert_eq!(
        mixed(&[a, b], &paths, &mix),
        [0.4, 0.2],
        "mix level 0 mutes path A"
    );
}

#[test]
fn y_meets_its_paths_at_unity() {
    let paths = [
        PathKnobs {
            mix_pan: -50.0,
            ..PathKnobs::neutral()
        }
        .with_neutral_mixer(),
        PathKnobs {
            invert: true,
            ..PathKnobs::neutral()
        }
        .with_neutral_mixer(),
    ];
    let mix = MixKnobs {
        master: 0.1,
        master_sum: true,
        ..ampero_mix()
    }
    .with_neutral_mixer();
    assert_close(mixed(&[[0.3, 0.1], [0.2, 0.4]], &paths, &mix), [0.5, 0.5]);
}
