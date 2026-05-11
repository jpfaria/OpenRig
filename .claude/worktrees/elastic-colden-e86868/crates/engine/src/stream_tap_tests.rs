use super::*;

#[test]
fn new_returns_two_rings() {
    let (_tap, handles) = StreamTap::new(0, 256);
    assert_eq!(handles.len(), 2);
}

#[test]
fn rings_share_state_between_handles_and_tap() {
    let (tap, handles) = StreamTap::new(3, 256);
    assert!(tap.l_ring.push(0.5));
    assert!(tap.r_ring.push(-0.25));
    assert_eq!(Arc::as_ptr(&tap.l_ring), Arc::as_ptr(&handles[0]));
    assert_eq!(Arc::as_ptr(&tap.r_ring), Arc::as_ptr(&handles[1]));
}

#[test]
fn stream_index_is_preserved() {
    let (tap, _) = StreamTap::new(7, 64);
    assert_eq!(tap.stream_index, 7);
}
