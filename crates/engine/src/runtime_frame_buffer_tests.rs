//! Engine runtime tests (issue #792 split from runtime_tests.rs).
//! Grouped by responsibility; shared fixtures live in `runtime_tests.rs`.
#![allow(unused_imports)]
use super::*;
use super::tests::*;


// --- ElasticBuffer tests ---

#[test]
fn elastic_buffer_push_pop_basic() {
    let buf = ElasticBuffer::new(256, AudioChannelLayout::Mono);
    buf.push(AudioFrame::Mono(0.5));
    buf.push(AudioFrame::Mono(0.7));
    assert_eq!(buf.len(), 2);
    let f1 = buf.pop();
    assert!(matches!(f1, AudioFrame::Mono(v) if (v - 0.5).abs() < 1e-6));
    let f2 = buf.pop();
    assert!(matches!(f2, AudioFrame::Mono(v) if (v - 0.7).abs() < 1e-6));
}


#[test]
fn elastic_buffer_underrun_returns_silence_not_last_frame() {
    // Issue #496: was `..._repeats_last_frame` and pinned the
    // "brief sustain on underrun" form, which measurably injected
    // broadband noise (swarm-of-bees artefact) into every chain.
    // The new contract is silence on underrun — standard DAW.
    let buf = ElasticBuffer::new(256, AudioChannelLayout::Mono);
    buf.push(AudioFrame::Mono(0.42));
    let _ = buf.pop();
    let next = buf.pop();
    assert!(matches!(next, AudioFrame::Mono(v) if v.abs() < 1e-6));
}


#[test]
fn elastic_buffer_underrun_before_any_push_returns_silence() {
    let buf = ElasticBuffer::new(256, AudioChannelLayout::Stereo);
    let frame = buf.pop();
    assert!(matches!(frame, AudioFrame::Stereo([l, r]) if l.abs() < 1e-6 && r.abs() < 1e-6));
}


#[test]
fn elastic_buffer_overrun_drops_newest() {
    // The lock-free SPSC ring drops the newest frame when full (rather
    // than advancing the tail from the producer side, which would break
    // the single-producer invariant). Ring capacity is the next power of
    // two at or above target_level * 2.
    let target: usize = 4;
    let capacity = (target * 2).next_power_of_two();
    let buf = ElasticBuffer::new(target, AudioChannelLayout::Mono);
    for i in 0..(capacity + 4) {
        buf.push(AudioFrame::Mono(i as f32));
    }
    assert_eq!(buf.len(), capacity);
    // Oldest frames are retained — first pop returns the very first push.
    assert!(matches!(buf.pop(), AudioFrame::Mono(v) if v == 0.0));
}


#[test]
fn elastic_buffer_stabilizes_around_target() {
    let target = 256;
    let buf = ElasticBuffer::new(target, AudioChannelLayout::Mono);
    // Simulate: push slightly faster than pop
    for _ in 0..10000 {
        buf.push(AudioFrame::Mono(1.0));
        buf.push(AudioFrame::Mono(1.0)); // 2 pushes
        let _ = buf.pop(); // 1 pop — simulates input faster than output
    }
    // Should not have grown unbounded
    assert!(buf.len() <= target * 2);
}


// ── ElasticBuffer edge cases ─────────────────────────────────────────────

#[test]
fn elastic_buffer_target_one_limits_to_two() {
    let buf = ElasticBuffer::new(1, AudioChannelLayout::Mono);
    buf.push(AudioFrame::Mono(1.0));
    buf.push(AudioFrame::Mono(2.0));
    buf.push(AudioFrame::Mono(3.0)); // should discard oldest
    assert!(
        buf.len() <= 2,
        "buffer with target=1 should hold at most 2 frames"
    );
}


#[test]
fn elastic_buffer_stereo_push_pop_preserves_channels() {
    let buf = ElasticBuffer::new(256, AudioChannelLayout::Stereo);
    buf.push(AudioFrame::Stereo([0.3, 0.7]));
    let frame = buf.pop();
    match frame {
        AudioFrame::Stereo([l, r]) => {
            assert!((l - 0.3).abs() < 1e-6);
            assert!((r - 0.7).abs() < 1e-6);
        }
        _ => panic!("expected stereo frame"),
    }
}


#[test]
fn elastic_buffer_multiple_pops_on_empty_return_silence() {
    // Issue #496: was `..._repeat_last`. Pinned the buggy form.
    let buf = ElasticBuffer::new(256, AudioChannelLayout::Mono);
    buf.push(AudioFrame::Mono(0.99));
    let _ = buf.pop();
    for _ in 0..5 {
        let f = buf.pop();
        assert!(matches!(f, AudioFrame::Mono(v) if v.abs() < 1e-6));
    }
}


// ── FadeState transition tests ───────────────────────────────────────────

#[test]
fn fade_in_completes_to_active_after_enough_frames() {
    let counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut block = counting_block_node(counter.clone());
    block.fade_state = FadeState::FadingIn {
        frames_remaining: 16,
    };

    let error_queue = ArrayQueue::<BlockError>::new(ERROR_QUEUE_CAPACITY);
    let mut frames = vec![AudioFrame::Stereo([1.0, 1.0]); 16];

    process_audio_block(&mut block, &mut frames, &error_queue);

    assert_eq!(
        block.fade_state,
        FadeState::Active,
        "fade-in should complete to Active when frames_remaining reaches 0"
    );
}


#[test]
fn fade_in_partial_keeps_fading_in() {
    let counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut block = counting_block_node(counter.clone());
    block.fade_state = FadeState::FadingIn {
        frames_remaining: 64,
    };

    let error_queue = ArrayQueue::<BlockError>::new(ERROR_QUEUE_CAPACITY);
    let mut frames = vec![AudioFrame::Stereo([1.0, 1.0]); 16];

    process_audio_block(&mut block, &mut frames, &error_queue);

    match block.fade_state {
        FadeState::FadingIn { frames_remaining } => {
            assert_eq!(
                frames_remaining, 48,
                "should have consumed 16 frames of fade"
            );
        }
        other => panic!("expected FadingIn, got {:?}", other),
    }
}


#[test]
fn fade_out_completes_to_bypassed_after_enough_frames() {
    let counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut block = counting_block_node(counter.clone());
    block.fade_state = FadeState::FadingOut {
        frames_remaining: 16,
    };

    let error_queue = ArrayQueue::<BlockError>::new(ERROR_QUEUE_CAPACITY);
    let mut frames = vec![AudioFrame::Stereo([1.0, 1.0]); 16];

    process_audio_block(&mut block, &mut frames, &error_queue);

    assert_eq!(
        block.fade_state,
        FadeState::Bypassed,
        "fade-out should complete to Bypassed when frames_remaining reaches 0"
    );
}


#[test]
fn fade_out_partial_keeps_fading_out() {
    let counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut block = counting_block_node(counter.clone());
    block.fade_state = FadeState::FadingOut {
        frames_remaining: 64,
    };

    let error_queue = ArrayQueue::<BlockError>::new(ERROR_QUEUE_CAPACITY);
    let mut frames = vec![AudioFrame::Stereo([1.0, 1.0]); 16];

    process_audio_block(&mut block, &mut frames, &error_queue);

    match block.fade_state {
        FadeState::FadingOut { frames_remaining } => {
            assert_eq!(
                frames_remaining, 48,
                "should have consumed 16 frames of fade"
            );
        }
        other => panic!("expected FadingOut, got {:?}", other),
    }
}


#[test]
fn fade_out_applies_processing_during_transition() {
    let counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut block = counting_block_node(counter.clone());
    block.fade_state = FadeState::FadingOut {
        frames_remaining: FADE_IN_FRAMES,
    };

    let error_queue = ArrayQueue::<BlockError>::new(ERROR_QUEUE_CAPACITY);
    let mut frames = vec![AudioFrame::Stereo([1.0, 1.0]); 16];

    process_audio_block(&mut block, &mut frames, &error_queue);

    assert!(
        counter.load(std::sync::atomic::Ordering::SeqCst) > 0,
        "fading-out block should still call process_sample during transition"
    );
}


// ── ElasticBuffer push/pop FIFO order ───────────────────────────────────

#[test]
fn elastic_buffer_fifo_order() {
    let buf = ElasticBuffer::new(256, AudioChannelLayout::Mono);
    for i in 0..10 {
        buf.push(AudioFrame::Mono(i as f32 * 0.1));
    }
    for i in 0..10 {
        let frame = buf.pop();
        let expected = i as f32 * 0.1;
        assert!(
            matches!(frame, AudioFrame::Mono(v) if (v - expected).abs() < 1e-6),
            "frame {i}: expected {expected}"
        );
    }
}

