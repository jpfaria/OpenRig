//! What the wiring keeps and what it lets go once the audio frees memory.
//!
//! Measured on the owner's rig: a chain with CloudReverb holds ~700 MB of
//! audio memory; turned off, the audio zone dropped to 0.1 MB in use but the
//! process stayed at ~1.07 GB, because the pages it had wired stayed wired.

use super::{split_wired, Split};

const MB: u64 = 1 << 20;

#[test]
fn a_range_the_audio_still_holds_stays_wired() {
    assert_eq!(
        split_wired(&[(10 * MB, 4 * MB)], &[(8 * MB, 8 * MB)]),
        Split {
            held: vec![(10 * MB, 4 * MB)],
            released: vec![],
        }
    );
}

#[test]
fn a_range_the_audio_freed_is_released_whole() {
    assert_eq!(
        split_wired(&[(10 * MB, 4 * MB)], &[]),
        Split {
            held: vec![],
            released: vec![(10 * MB, 4 * MB)],
        }
    );
}

#[test]
fn only_the_part_outside_the_audio_is_released() {
    assert_eq!(
        split_wired(&[(10 * MB, 10 * MB)], &[(13 * MB, 2 * MB)]),
        Split {
            held: vec![(13 * MB, 2 * MB)],
            released: vec![(10 * MB, 3 * MB), (15 * MB, 5 * MB)],
        }
    );
}

#[test]
fn spans_in_any_order_each_keep_their_piece() {
    assert_eq!(
        split_wired(
            &[(20 * MB, 10 * MB), (0, 4 * MB)],
            &[(26 * MB, 8 * MB), (0, 1 * MB), (18 * MB, 4 * MB)],
        ),
        Split {
            held: vec![(0, MB), (20 * MB, 2 * MB), (26 * MB, 4 * MB)],
            released: vec![(MB, 3 * MB), (22 * MB, 4 * MB)],
        }
    );
}

#[test]
fn nothing_wired_releases_nothing() {
    assert_eq!(split_wired(&[], &[(0, MB)]), Split::default());
}
