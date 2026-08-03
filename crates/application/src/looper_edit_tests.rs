//! #826 — what a selection MEANS, kept out of every frontend.

use super::*;

#[test]
fn the_selection_ratios_become_frames_of_the_loop_as_it_stands() {
    assert_eq!(
        LoopEdit::from_ratios(LoopEditKind::Trim, 1000, 0.25, 0.75),
        LoopEdit::Trim {
            start: 250,
            end: 750
        }
    );
    assert_eq!(
        LoopEdit::from_ratios(LoopEditKind::Cut, 512, 0.0, 1.0),
        LoopEdit::Cut { start: 0, end: 512 }
    );
}

#[test]
fn a_drag_past_the_edges_is_clamped_to_the_loop() {
    // A handle dragged outside the view means "all the way", never a region
    // the store would have to reject on bounds alone.
    assert_eq!(
        LoopEdit::from_ratios(LoopEditKind::Crop, 1000, -0.4, 1.9),
        LoopEdit::Crop {
            start: 0,
            end: 1000
        }
    );
}

#[test]
fn a_backwards_drag_still_describes_the_same_region() {
    assert_eq!(
        LoopEdit::from_ratios(LoopEditKind::Trim, 1000, 0.8, 0.2),
        LoopEdit::Trim {
            start: 200,
            end: 800
        }
    );
}

#[test]
fn an_empty_loop_yields_an_empty_region_instead_of_dividing_by_zero() {
    assert_eq!(
        LoopEdit::from_ratios(LoopEditKind::Trim, 0, 0.0, 1.0),
        LoopEdit::Trim { start: 0, end: 0 }
    );
}

#[test]
fn an_edit_that_changes_nothing_is_its_own_outcome() {
    // The bug this exists for: FIT on an already-tight take did nothing and
    // said nothing, which reads as broken.
    assert_eq!(EditOutcome::of(4000, Some(3200)), EditOutcome::Applied);
    assert_eq!(EditOutcome::of(4000, Some(4000)), EditOutcome::NoChange);
    assert_eq!(EditOutcome::of(4000, None), EditOutcome::Refused);
}

#[test]
fn the_outcome_travels_as_a_code_so_the_message_stays_translatable() {
    assert_eq!(EditOutcome::Applied.code(), 0);
    assert_eq!(EditOutcome::NoChange.code(), 1);
    assert_eq!(EditOutcome::Refused.code(), 2);
}
