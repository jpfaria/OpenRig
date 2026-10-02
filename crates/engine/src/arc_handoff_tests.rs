//! The handoff's contract: the audio side gets the newest value, and the
//! audio side never releases the last reference to anything.

use std::sync::{Arc, Weak};

use super::ArcHandoff;

#[test]
fn take_latest_returns_the_newest_value_sent() {
    let handoff = ArcHandoff::new();
    handoff.send(Arc::new(1));
    handoff.send(Arc::new(2));
    handoff.send(Arc::new(3));

    assert_eq!(handoff.take_latest().as_deref(), Some(&3));
    assert!(handoff.take_latest().is_none(), "a value is delivered once");
}

#[test]
fn a_value_dropped_on_the_audio_side_is_still_held_by_the_control_side() {
    let handoff = ArcHandoff::new();
    let first = Arc::new(String::from("first"));
    let weak: Weak<String> = Arc::downgrade(&first);
    handoff.send(first);

    let on_audio = handoff.take_latest().expect("sent");
    handoff.send(Arc::new(String::from("second")));
    drop(on_audio);

    assert!(
        weak.upgrade().is_some(),
        "the audio side must never free a value; the control side releases it"
    );
}

#[test]
fn collect_releases_values_only_the_handoff_holds() {
    let handoff = ArcHandoff::new();
    let first = Arc::new(String::from("first"));
    let weak = Arc::downgrade(&first);
    handoff.send(first);
    drop(handoff.take_latest());
    handoff.send(Arc::new(String::from("second")));
    drop(handoff.take_latest());

    handoff.collect();

    assert!(
        weak.upgrade().is_none(),
        "an old value is released by collect"
    );
    assert_eq!(
        handoff.latest().as_deref().map(String::as_str),
        Some("second")
    );
}

#[test]
fn collect_keeps_a_value_the_audio_side_still_uses() {
    let handoff = ArcHandoff::new();
    let value = Arc::new(7);
    let weak = Arc::downgrade(&value);
    handoff.send(value);
    let on_audio = handoff.take_latest().expect("sent");
    handoff.send(Arc::new(8));

    handoff.collect();

    assert!(weak.upgrade().is_some());
    drop(on_audio);
}

#[test]
fn retained_values_are_held_without_being_delivered() {
    let handoff = ArcHandoff::new();
    let value = Arc::new(5);
    let weak = Arc::downgrade(&value);
    handoff.retain(Arc::clone(&value));

    assert!(handoff.take_latest().is_none());
    drop(value);
    assert!(weak.upgrade().is_some(), "retain keeps a reference");
    handoff.collect();
    assert!(weak.upgrade().is_none());
}

#[test]
fn latest_is_none_before_anything_is_sent() {
    let handoff: ArcHandoff<u32> = ArcHandoff::new();
    assert!(handoff.latest().is_none());
}
