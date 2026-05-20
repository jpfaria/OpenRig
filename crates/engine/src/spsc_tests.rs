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

// ── Q. SpscRing direct invariants (issue #496, 30 tests) ────────

#[test] fn q01_new_capacity_rounds_5_to_8() { assert_eq!(SpscRing::<u32>::new(5, 0).capacity(), 8); }
#[test] fn q02_new_capacity_rounds_9_to_16() { assert_eq!(SpscRing::<u32>::new(9, 0).capacity(), 16); }
#[test] fn q03_new_capacity_rounds_17_to_32() { assert_eq!(SpscRing::<u32>::new(17, 0).capacity(), 32); }
#[test] fn q04_new_capacity_already_power_of_two_stays() { assert_eq!(SpscRing::<u32>::new(16, 0).capacity(), 16); }
#[test] fn q05_new_one_clamps_to_two() { assert_eq!(SpscRing::<u32>::new(1, 0).capacity(), 2); }

#[test] fn q06_len_starts_zero() { assert_eq!(SpscRing::<u32>::new(8, 0).len(), 0); }
#[test] fn q07_len_after_push_is_one() { let r = SpscRing::<u32>::new(8, 0); r.push(7); assert_eq!(r.len(), 1); }
#[test] fn q08_len_after_pop_is_zero() { let r = SpscRing::<u32>::new(8, 0); r.push(7); r.pop(); assert_eq!(r.len(), 0); }
#[test] fn q09_len_equals_pushed_minus_popped() { let r = SpscRing::<u32>::new(16, 0); for i in 0..10u32 { r.push(i); } for _ in 0..3 { r.pop(); } assert_eq!(r.len(), 7); }
#[test] fn q10_len_never_exceeds_capacity() { let r = SpscRing::<u32>::new(8, 0); for i in 0..20u32 { r.push(i); } assert!(r.len() <= 8); }

#[test] fn q11_pop_empty_returns_none() { assert_eq!(SpscRing::<u32>::new(8, 0).pop(), None); }
#[test] fn q12_pop_twice_on_one_push_second_returns_none() { let r = SpscRing::<u32>::new(8, 0); r.push(1); assert_eq!(r.pop(), Some(1)); assert_eq!(r.pop(), None); }
#[test] fn q13_push_when_full_drops_silently() { let r = SpscRing::<u32>::new(2, 0); r.push(1); r.push(2); assert!(!r.push(99)); assert_eq!(r.pop(), Some(1)); assert_eq!(r.pop(), Some(2)); assert_eq!(r.pop(), None); }
#[test] fn q14_drop_newest_loses_payload() { let r = SpscRing::<u32>::new(2, 0); r.push(10); r.push(20); r.push(30); assert_eq!(r.pop(), Some(10)); assert_eq!(r.pop(), Some(20)); assert_eq!(r.pop(), None); }
#[test] fn q15_interleaved_push_pop_preserves_order() { let r = SpscRing::<u32>::new(4, 0); r.push(1); assert_eq!(r.pop(), Some(1)); r.push(2); r.push(3); assert_eq!(r.pop(), Some(2)); r.push(4); assert_eq!(r.pop(), Some(3)); assert_eq!(r.pop(), Some(4)); }

#[test] fn q16_wraparound_indices_work() { let r = SpscRing::<u32>::new(4, 0); for round in 0..100 { for i in 0..4u32 { assert!(r.push(round * 10 + i)); } for i in 0..4u32 { assert_eq!(r.pop(), Some(round * 10 + i)); } } }
#[test] fn q17_partial_fill_then_drain_repeatedly() { let r = SpscRing::<u32>::new(8, 0); for round in 0..1_000u32 { r.push(round); assert_eq!(r.pop(), Some(round)); } assert_eq!(r.len(), 0); }
#[test] fn q18_burst_fill_burst_drain() { let r = SpscRing::<u32>::new(64, 0); for i in 0..64u32 { r.push(i); } for i in 0..64u32 { assert_eq!(r.pop(), Some(i)); } }
#[test] fn q19_drop_newest_when_overfilled_keeps_oldest() { let r = SpscRing::<u32>::new(4, 0); for i in 0..10u32 { r.push(i); } let mut got = Vec::new(); while let Some(v) = r.pop() { got.push(v); } assert_eq!(got, vec![0, 1, 2, 3]); }
#[test] fn q20_pop_after_repeated_overflow_only_returns_drops() { let r = SpscRing::<u32>::new(2, 0); for i in 0..6u32 { r.push(i); } assert_eq!(r.pop(), Some(0)); assert_eq!(r.pop(), Some(1)); assert_eq!(r.pop(), None); }

#[test] fn q21_f32_audio_sample_round_trip() { let r = SpscRing::<f32>::new(8, 0.0); r.push(0.7); r.push(-0.3); assert_eq!(r.pop(), Some(0.7)); assert_eq!(r.pop(), Some(-0.3)); }
#[test] fn q22_stereo_pair_round_trip() { let r = SpscRing::<(f32, f32)>::new(8, (0.0, 0.0)); r.push((0.4, -0.4)); assert_eq!(r.pop(), Some((0.4, -0.4))); }
#[test] fn q23_pop_when_empty_then_push_returns_pushed() { let r = SpscRing::<u32>::new(8, 0); assert_eq!(r.pop(), None); r.push(99); assert_eq!(r.pop(), Some(99)); }
#[test] fn q24_drop_newest_does_not_advance_tail() { let r = SpscRing::<u32>::new(2, 0); r.push(1); r.push(2); r.push(3); assert_eq!(r.len(), 2); }
#[test] fn q25_after_full_drain_can_refill_to_capacity() { let r = SpscRing::<u32>::new(8, 0); for i in 0..8u32 { r.push(i); } while r.pop().is_some() {} for i in 100..108u32 { assert!(r.push(i)); } for i in 100..108u32 { assert_eq!(r.pop(), Some(i)); } }

#[test] fn q26_min_capacity_two_can_hold_two() { let r = SpscRing::<u32>::new(2, 0); assert!(r.push(1)); assert!(r.push(2)); assert!(!r.push(3)); }
#[test] fn q27_large_capacity_works() { let r = SpscRing::<u32>::new(8192, 0); for i in 0..8192u32 { r.push(i); } assert!(!r.push(99999)); for i in 0..8192u32 { assert_eq!(r.pop(), Some(i)); } }
#[test] fn q28_init_value_does_not_leak_through_push_pop() { let r = SpscRing::<u32>::new(8, 0xDEAD); r.push(0xBEEF); assert_eq!(r.pop(), Some(0xBEEF)); }
#[test] fn q29_drop_newest_on_overflow_loses_count_visible_via_len() { let r = SpscRing::<u32>::new(4, 0); for i in 0..10u32 { r.push(i); } assert!(r.len() <= 4); }
#[test] fn q30_underrun_then_recover_pattern() { let r = SpscRing::<u32>::new(8, 0); for round in 0..50 { assert_eq!(r.pop(), None); r.push(round); assert_eq!(r.pop(), Some(round)); } }
