//! `ElasticBuffer` / `AudioFrame` direct-invariant tests (issue #792 —
//! extracted from runtime_audio_frame.rs so tests live in a sibling file).

use super::*;

fn mono(s: f32) -> AudioFrame {
    AudioFrame::Mono(s)
}
fn stereo(l: f32, r: f32) -> AudioFrame {
    AudioFrame::Stereo([l, r])
}
fn unwrap_mono(f: AudioFrame) -> f32 {
    match f {
        AudioFrame::Mono(s) => s,
        _ => panic!(),
    }
}
fn unwrap_stereo(f: AudioFrame) -> (f32, f32) {
    match f {
        AudioFrame::Stereo([l, r]) => (l, r),
        _ => panic!(),
    }
}

#[test]
fn r01_push_pop_single_mono_frame_round_trips() {
    let b = ElasticBuffer::new(16, AudioChannelLayout::Mono);
    b.push(mono(0.42));
    assert_eq!(unwrap_mono(b.pop()), 0.42);
}
#[test]
fn r02_push_pop_single_stereo_frame_round_trips() {
    let b = ElasticBuffer::new(16, AudioChannelLayout::Stereo);
    b.push(stereo(0.3, -0.4));
    assert_eq!(unwrap_stereo(b.pop()), (0.3, -0.4));
}
#[test]
fn r03_initial_pop_returns_silent_mono() {
    let b = ElasticBuffer::new(16, AudioChannelLayout::Mono);
    assert_eq!(unwrap_mono(b.pop()), 0.0);
}
#[test]
fn r04_initial_pop_returns_silent_stereo() {
    let b = ElasticBuffer::new(16, AudioChannelLayout::Stereo);
    assert_eq!(unwrap_stereo(b.pop()), (0.0, 0.0));
}
#[test]
fn r05_push_n_pop_n_preserves_fifo_order() {
    let b = ElasticBuffer::new(16, AudioChannelLayout::Mono);
    for i in 0..10 {
        b.push(mono(i as f32 * 0.1));
    }
    for i in 0..10 {
        assert!(
            (unwrap_mono(b.pop()) - i as f32 * 0.1).abs() < 1e-6,
            "i={i}"
        );
    }
}
#[test]
fn r06_len_starts_zero() {
    assert_eq!(ElasticBuffer::new(16, AudioChannelLayout::Mono).len(), 0);
}
#[test]
fn r07_len_after_push_is_one() {
    let b = ElasticBuffer::new(16, AudioChannelLayout::Mono);
    b.push(mono(1.0));
    assert_eq!(b.len(), 1);
}

// BUG SURFACE: underrun returns last pushed frame (REPEATS).
#[test]
fn r08_underrun_should_not_repeat_last_mono_indefinitely() {
    let b = ElasticBuffer::new(16, AudioChannelLayout::Mono);
    b.push(mono(0.7));
    assert_eq!(unwrap_mono(b.pop()), 0.7);
    for i in 0..10 {
        let v = unwrap_mono(b.pop());
        assert!(
            v.abs() < 1e-6,
            "underrun frame {i} = {v} (repeated last; should be silence/faded)"
        );
    }
}
#[test]
fn r09_underrun_should_not_repeat_last_stereo_indefinitely() {
    let b = ElasticBuffer::new(16, AudioChannelLayout::Stereo);
    b.push(stereo(0.4, -0.6));
    b.pop();
    for i in 0..10 {
        let (l, r) = unwrap_stereo(b.pop());
        assert!(
            l.abs() < 1e-6 && r.abs() < 1e-6,
            "stereo underrun frame {i} = ({l},{r}) (repeated); broadband noise source"
        );
    }
}

#[test]
fn r10_underrun_in_middle_of_sine_creates_plateau() {
    let b = ElasticBuffer::new(16, AudioChannelLayout::Mono);
    let sr = 48_000.0_f32;
    let pushed: Vec<f32> = (0..3)
        .map(|i| (2.0 * std::f32::consts::PI * 1_000.0 * i as f32 / sr).sin())
        .collect();
    for &s in &pushed {
        b.push(mono(s));
    }
    let mut popped = Vec::new();
    for _ in 0..10 {
        popped.push(unwrap_mono(b.pop()));
    }
    for (i, &p) in popped.iter().enumerate().skip(3) {
        assert!(
            (p - pushed[2]).abs() > 1e-6,
            "frame {i} repeats last pushed ({}); flat-top harmonic-injection bug",
            p
        );
    }
}
#[test]
fn r11_dc_input_then_underrun_extends_dc_indefinitely() {
    let b = ElasticBuffer::new(16, AudioChannelLayout::Mono);
    b.push(mono(0.5));
    b.pop();
    for _ in 0..50 {
        let v = unwrap_mono(b.pop());
        assert!(v.abs() < 1e-6, "DC plateau extension: {v}");
    }
}
#[test]
fn r12_after_seed_underrun_returns_silence_not_seeded() {
    let a = ElasticBuffer::new(16, AudioChannelLayout::Mono);
    a.push(mono(0.8));
    a.pop();
    let b = ElasticBuffer::new(16, AudioChannelLayout::Mono);
    b.seed_last_frame_from(&a);
    let v = unwrap_mono(b.pop());
    assert!(
        v.abs() < 1e-6,
        "after seed, underrun should produce silence (not {v})"
    );
}
#[test]
fn r13_seed_does_not_inject_into_ring() {
    let a = ElasticBuffer::new(16, AudioChannelLayout::Mono);
    a.push(mono(0.3));
    a.pop();
    let b = ElasticBuffer::new(16, AudioChannelLayout::Mono);
    b.seed_last_frame_from(&a);
    assert_eq!(b.len(), 0);
}

#[test]
fn r14_overrun_drops_newest_silently() {
    let b = ElasticBuffer::new(2, AudioChannelLayout::Mono);
    for i in 0..10 {
        b.push(mono(i as f32 * 0.1));
    }
    let mut got = Vec::new();
    while b.len() > 0 {
        got.push(unwrap_mono(b.pop()));
    }
    assert_eq!(got.len(), 4);
    for (i, v) in got.iter().enumerate() {
        assert!((v - (i as f32 * 0.1)).abs() < 1e-6, "i={i}");
    }
}
#[test]
fn r15_overrun_does_not_corrupt_existing_frames() {
    let b = ElasticBuffer::new(2, AudioChannelLayout::Mono);
    b.push(mono(0.1));
    b.push(mono(0.2));
    for _ in 0..50 {
        b.push(mono(99.0));
    }
    assert!((unwrap_mono(b.pop()) - 0.1).abs() < 1e-6);
}

#[test]
fn r16_capacity_1_mono() {
    let b = ElasticBuffer::new(1, AudioChannelLayout::Mono);
    b.push(mono(0.5));
    assert_eq!(unwrap_mono(b.pop()), 0.5);
}
#[test]
fn r17_capacity_4_mono() {
    let b = ElasticBuffer::new(4, AudioChannelLayout::Mono);
    for i in 0..8 {
        b.push(mono(i as f32 / 10.0));
    }
    let mut got = Vec::new();
    while b.len() > 0 {
        got.push(unwrap_mono(b.pop()));
    }
    assert_eq!(got.len(), 8);
}
#[test]
fn r18_capacity_256_mono() {
    let b = ElasticBuffer::new(256, AudioChannelLayout::Mono);
    for i in 0..256 {
        b.push(mono(i as f32 / 256.0));
    }
    let mut got = Vec::new();
    while b.len() > 0 {
        got.push(unwrap_mono(b.pop()));
    }
    assert_eq!(got.len(), 256);
}
#[test]
fn r19_capacity_1024_mono() {
    let b = ElasticBuffer::new(1024, AudioChannelLayout::Mono);
    for i in 0..1024 {
        b.push(mono(i as f32 / 1024.0));
    }
    assert_eq!(b.len(), 1024);
}

#[test]
fn r20_alternating_push_pop_steady_state() {
    let b = ElasticBuffer::new(16, AudioChannelLayout::Mono);
    for i in 0..100 {
        b.push(mono(i as f32 / 100.0));
        assert!((unwrap_mono(b.pop()) - i as f32 / 100.0).abs() < 1e-6);
    }
    assert_eq!(b.len(), 0);
}
#[test]
fn r21_push_burst_then_drain() {
    let b = ElasticBuffer::new(64, AudioChannelLayout::Mono);
    for i in 0..32 {
        b.push(mono(i as f32 / 100.0));
    }
    for i in 0..32 {
        assert!((unwrap_mono(b.pop()) - i as f32 / 100.0).abs() < 1e-6);
    }
}
#[test]
fn r22_consumer_ahead_pops_silence_then_recovers() {
    let b = ElasticBuffer::new(16, AudioChannelLayout::Mono);
    b.push(mono(0.4));
    assert_eq!(unwrap_mono(b.pop()), 0.4);
    for i in 0..5 {
        let v = unwrap_mono(b.pop());
        assert!(v.abs() < 1e-6, "consumer-ahead frame {i} = {v}");
    }
    b.push(mono(0.7));
    assert_eq!(unwrap_mono(b.pop()), 0.7);
}

#[test]
fn r23_zero_frame_round_trips() {
    let b = ElasticBuffer::new(16, AudioChannelLayout::Mono);
    b.push(mono(0.0));
    assert_eq!(unwrap_mono(b.pop()), 0.0);
}
#[test]
fn r24_positive_full_scale_round_trips() {
    let b = ElasticBuffer::new(16, AudioChannelLayout::Mono);
    b.push(mono(1.0));
    assert_eq!(unwrap_mono(b.pop()), 1.0);
}
#[test]
fn r25_negative_full_scale_round_trips() {
    let b = ElasticBuffer::new(16, AudioChannelLayout::Mono);
    b.push(mono(-1.0));
    assert_eq!(unwrap_mono(b.pop()), -1.0);
}
#[test]
fn r26_subnormal_value_round_trips() {
    let b = ElasticBuffer::new(16, AudioChannelLayout::Mono);
    let x = f32::MIN_POSITIVE / 2.0;
    b.push(mono(x));
    assert_eq!(unwrap_mono(b.pop()), x);
}
#[test]
fn r27_negative_zero_treated_as_zero() {
    let b = ElasticBuffer::new(16, AudioChannelLayout::Mono);
    b.push(mono(-0.0));
    assert_eq!(unwrap_mono(b.pop()), 0.0);
}

#[test]
fn r28_stereo_l_r_order_preserved() {
    let b = ElasticBuffer::new(16, AudioChannelLayout::Stereo);
    b.push(stereo(0.1, 0.9));
    assert_eq!(unwrap_stereo(b.pop()), (0.1, 0.9));
}
#[test]
fn r29_stereo_underrun_should_be_silence_not_last_pushed() {
    let b = ElasticBuffer::new(16, AudioChannelLayout::Stereo);
    b.push(stereo(0.2, -0.7));
    b.pop();
    for i in 0..20 {
        let (l, r) = unwrap_stereo(b.pop());
        assert!(
            l.abs() < 1e-6 && r.abs() < 1e-6,
            "stereo underrun frame {i} = ({l},{r})"
        );
    }
}
#[test]
fn r30_stereo_multi_push_preserves_each_pair() {
    let b = ElasticBuffer::new(16, AudioChannelLayout::Stereo);
    let pairs = [(0.1, 0.2), (0.3, 0.4), (0.5, 0.6), (0.7, 0.8)];
    for &(l, r) in &pairs {
        b.push(stereo(l, r));
    }
    for &(l, r) in &pairs {
        assert_eq!(unwrap_stereo(b.pop()), (l, r));
    }
}

// ── The underrun COUNTER (not just the silent frame). A starved output
//    pop is the single-chain crackle; pin that it is COUNTED, every time.
#[test]
fn r31_empty_pop_increments_underrun_count_each_time() {
    let b = ElasticBuffer::new(16, AudioChannelLayout::Mono);
    assert_eq!(b.underrun_count(), 0);
    for i in 1..=6u64 {
        let _ = b.pop();
        assert_eq!(b.underrun_count(), i, "each empty pop counts one underrun");
    }
}
#[test]
fn r32_pop_with_data_does_not_increment_underrun_count() {
    let b = ElasticBuffer::new(16, AudioChannelLayout::Mono);
    b.push(mono(0.5));
    let _ = b.pop();
    assert_eq!(b.underrun_count(), 0, "a fed pop is not an underrun");
}
#[test]
fn r33_consumer_ahead_of_producer_underruns_exactly_the_deficit() {
    // Producer delivered 2 frames; consumer pops 5 → 3 underruns. This is
    // the elastic STARVE that the user hears as crackle on a single chain.
    let b = ElasticBuffer::new(16, AudioChannelLayout::Mono);
    b.push(mono(0.5));
    b.push(mono(0.6));
    for _ in 0..5 {
        let _ = b.pop();
    }
    assert_eq!(
        b.underrun_count(),
        3,
        "exactly the popped-minus-pushed deficit"
    );
}
#[test]
fn r34_primed_cushion_drains_as_silence_without_underrun() {
    // Priming pre-fills with silence so early pops are NOT underruns — the
    // #592/#670 cold-start cushion that protects an IR chain's first
    // partitions from starving.
    let b = ElasticBuffer::new(64, AudioChannelLayout::Mono);
    b.prime(10);
    assert_eq!(b.len(), 10);
    for _ in 0..10 {
        assert_eq!(unwrap_mono(b.pop()), 0.0);
    }
    assert_eq!(
        b.underrun_count(),
        0,
        "draining the primed cushion is not a starve"
    );
    assert_eq!(b.len(), 0);
}

// ── elastic_target_for_buffer: the device→capacity sizing. Pure, was
//    untested. Floor is ELASTIC_TARGET_FLOOR (64).
#[test]
fn elastic_target_floors_and_scales_by_multiplier() {
    // CPAL multiplier 2; JACK multiplier 8.
    assert_eq!(elastic_target_for_buffer(0, 2), ELASTIC_TARGET_FLOOR); // floor
    assert_eq!(elastic_target_for_buffer(32, 2), 64); // 64 == floor
    assert_eq!(elastic_target_for_buffer(64, 2), 128); // common cpal case
    assert_eq!(elastic_target_for_buffer(256, 2), 512);
    assert_eq!(elastic_target_for_buffer(64, 8), 512); // jack
                                                       // Pathological huge buffer saturates instead of overflowing.
    assert!(elastic_target_for_buffer(u32::MAX, 8) >= ELASTIC_TARGET_FLOOR);
}
