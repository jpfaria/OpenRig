use super::*;


#[test]
fn new_rounds_capacity_up_to_power_of_two() {
    let ring = SpscRing::<u32>::new(3, 0);
    assert_eq!(ring.capacity(), 4);
}

#[test]
fn new_clamps_zero_capacity_to_minimum_two() {
    let ring = SpscRing::<u32>::new(0, 0);
    assert_eq!(ring.capacity(), 2);
}

#[test]
fn push_pop_single_frame_round_trips() {
    let ring = SpscRing::<u32>::new(4, 0);
    assert!(ring.push(42));
    assert_eq!(ring.pop(), Some(42));
    assert_eq!(ring.pop(), None);
}

#[test]
fn push_returns_false_when_full() {
    let ring = SpscRing::<u32>::new(2, 0);
    assert!(ring.push(1));
    assert!(ring.push(2));
    assert!(!ring.push(3));
}

#[test]
fn pop_after_push_drains_in_fifo_order() {
    let ring = SpscRing::<u32>::new(4, 0);
    for i in 0..4 {
        ring.push(i);
    }
    for i in 0..4 {
        assert_eq!(ring.pop(), Some(i));
    }
    assert_eq!(ring.pop(), None);
}

#[test]
fn push_after_pop_reuses_slots() {
    let ring = SpscRing::<u32>::new(2, 0);
    ring.push(1);
    ring.push(2);
    assert!(!ring.push(3));
    ring.pop();
    assert!(ring.push(3));
}

#[test]
fn concurrent_push_pop_preserves_ordering() {
    use std::sync::Arc;
    use std::thread;
    let ring = Arc::new(SpscRing::<u32>::new(128, 0));
    let producer = ring.clone();
    let t = thread::spawn(move || {
        for i in 0..10_000u32 {
            while !producer.push(i) {
                std::hint::spin_loop();
            }
        }
    });
    let mut expected = 0u32;
    while expected < 10_000 {
        if let Some(v) = ring.pop() {
            assert_eq!(v, expected);
            expected += 1;
        }
    }
    t.join().unwrap();
}
