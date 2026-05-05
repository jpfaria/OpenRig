use super::*;


#[test]
fn new_returns_one_ring_per_subscribed_channel() {
    let (tap, handles) = InputTap::new(0, 4, &[0, 2], 256);
    assert_eq!(handles.len(), 2);
    // Tap stores Some only for subscribed channels
    assert!(tap.channel_rings[0].is_some());
    assert!(tap.channel_rings[1].is_none());
    assert!(tap.channel_rings[2].is_some());
    assert!(tap.channel_rings[3].is_none());
}

#[test]
fn new_skips_out_of_range_channels() {
    let (tap, handles) = InputTap::new(0, 2, &[0, 5], 256);
    assert_eq!(handles.len(), 1);
    assert!(tap.channel_rings[0].is_some());
    assert!(tap.channel_rings[1].is_none());
}

#[test]
fn rings_share_state_between_handles_and_tap() {
    let (tap, handles) = InputTap::new(0, 1, &[0], 256);
    let producer = tap.channel_rings[0].as_ref().unwrap();
    assert!(producer.push(1.5));
    // Consumer side reads through their handle
    let consumer = &handles[0];
    assert_eq!(Arc::as_ptr(producer), Arc::as_ptr(consumer));
}
