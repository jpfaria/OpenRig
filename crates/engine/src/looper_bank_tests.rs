//! Issue #323 — the bank's op queue, slot bookkeeping and status mirror.

use super::*;
use crate::looper::LooperState;

const SR: f32 = 48_000.0;
const UID: u64 = 42;

fn shared() -> LooperShared {
    LooperShared::new(SR)
}

fn bank(shared: &LooperShared) -> LooperBank {
    // A short buffer keeps the tests cheap; the real bank is sized from the
    // live sample rate.
    let _ = shared;
    LooperBank::new(8)
}

fn layer() -> Box<[f32]> {
    vec![0.0f32; 8 * 2].into_boxed_slice()
}

fn stereo(frames: &[[f32; 2]]) -> Vec<AudioFrame> {
    frames.iter().map(|f| AudioFrame::Stereo(*f)).collect()
}

/// `AudioFrame` is a hot-path type with no `PartialEq`; compare the samples.
fn lr(frame: AudioFrame) -> [f32; 2] {
    match frame {
        AudioFrame::Stereo(v) => v,
        AudioFrame::Mono(s) => [s, s],
    }
}

#[test]
fn max_frames_follows_the_live_sample_rate() {
    assert_eq!(LooperShared::new(48_000.0).max_frames(), 48_000 * 60);
    assert_eq!(LooperShared::new(44_100.0).max_frames(), 44_100 * 60);
}

#[test]
fn create_claims_a_slot_and_publishes_an_empty_status() {
    let sh = shared();
    let mut b = bank(&sh);
    sh.push(LooperOp::Create { uid: UID }).unwrap();

    b.drain_ops(&sh);
    b.publish(&sh);

    let st = sh.status(UID).expect("the looper exists");
    assert_eq!(st.state, LooperState::Empty);
    assert_eq!(st.layers, 0);
    assert!(!b.is_idle(), "a claimed slot takes the bank out of idle");
}

#[test]
fn an_empty_bank_is_idle_and_leaves_the_frames_untouched() {
    let sh = shared();
    let mut b = bank(&sh);
    assert!(b.is_idle());

    let mut frames = stereo(&[[0.25, -0.25]]);
    b.process(&mut frames, AudioChannelLayout::Stereo);
    assert_eq!(lr(frames[0]), [0.25, -0.25]);
}

#[test]
fn recorded_material_is_summed_back_into_the_chain_input() {
    let sh = shared();
    let mut b = bank(&sh);
    sh.push(LooperOp::Create { uid: UID }).unwrap();
    sh.push(LooperOp::TapRecord {
        uid: UID,
        buffer: Some(layer()),
    })
    .unwrap();
    b.drain_ops(&sh);

    let mut rec = stereo(&[[0.5, 0.5], [0.25, 0.25]]);
    b.process(&mut rec, AudioChannelLayout::Stereo);
    assert_eq!(lr(rec[0]), [0.5, 0.5], "dry passes through untouched");

    sh.push(LooperOp::TapRecord {
        uid: UID,
        buffer: None,
    })
    .unwrap();
    b.drain_ops(&sh);

    // Silent input now: what comes out is the loop.
    let mut play = stereo(&[[0.0, 0.0], [0.0, 0.0]]);
    b.process(&mut play, AudioChannelLayout::Stereo);
    assert_eq!(lr(play[0]), [0.5, 0.5]);
    assert_eq!(lr(play[1]), [0.25, 0.25]);
}

#[test]
fn two_loopers_record_the_same_dry_signal_not_each_other() {
    let sh = shared();
    let mut b = bank(&sh);
    for uid in [1u64, 2] {
        sh.push(LooperOp::Create { uid }).unwrap();
        sh.push(LooperOp::TapRecord {
            uid,
            buffer: Some(layer()),
        })
        .unwrap();
    }
    b.drain_ops(&sh);

    let mut rec = stereo(&[[1.0, 1.0]]);
    b.process(&mut rec, AudioChannelLayout::Stereo);
    for uid in [1u64, 2] {
        sh.push(LooperOp::TapRecord { uid, buffer: None }).unwrap();
    }
    b.drain_ops(&sh);

    let mut play = stereo(&[[0.0, 0.0]]);
    b.process(&mut play, AudioChannelLayout::Stereo);
    assert_eq!(
        lr(play[0]),
        [2.0, 2.0],
        "each looper captured the dry 1.0 — neither recorded the other's playback"
    );
}

#[test]
fn a_mono_chain_gets_the_loop_mixed_down_at_unity() {
    let sh = shared();
    let mut b = bank(&sh);
    sh.push(LooperOp::Create { uid: UID }).unwrap();
    sh.push(LooperOp::TapRecord {
        uid: UID,
        buffer: Some(layer()),
    })
    .unwrap();
    b.drain_ops(&sh);

    let mut rec = vec![AudioFrame::Mono(0.5)];
    b.process(&mut rec, AudioChannelLayout::Mono);
    assert_eq!(lr(rec[0]), [0.5, 0.5]);

    sh.push(LooperOp::TapRecord {
        uid: UID,
        buffer: None,
    })
    .unwrap();
    b.drain_ops(&sh);

    let mut play = vec![AudioFrame::Mono(0.0)];
    b.process(&mut play, AudioChannelLayout::Mono);
    assert_eq!(lr(play[0]), [0.5, 0.5]);
}

#[test]
fn an_op_for_an_unclaimed_uid_hands_its_buffer_back() {
    let sh = shared();
    let mut b = bank(&sh);
    sh.push(LooperOp::TapRecord {
        uid: 999,
        buffer: Some(layer()),
    })
    .unwrap();

    b.drain_ops(&sh);
    b.publish(&sh);

    assert_eq!(sh.drain_retired().len(), 1);
    assert!(sh.status(999).is_none());
}

#[test]
fn remove_frees_the_slot_and_returns_its_layers() {
    let sh = shared();
    let mut b = bank(&sh);
    sh.push(LooperOp::Create { uid: UID }).unwrap();
    sh.push(LooperOp::TapRecord {
        uid: UID,
        buffer: Some(layer()),
    })
    .unwrap();
    b.drain_ops(&sh);
    b.process(&mut stereo(&[[1.0, 1.0]]), AudioChannelLayout::Stereo);

    sh.push(LooperOp::Remove { uid: UID }).unwrap();
    b.drain_ops(&sh);
    b.publish(&sh);

    assert!(sh.status(UID).is_none(), "the slot is free again");
    assert!(b.is_idle());
    assert_eq!(sh.drain_retired().len(), 1);
}

#[test]
fn statuses_lists_every_live_looper_in_slot_order() {
    let sh = shared();
    let mut b = bank(&sh);
    sh.push(LooperOp::Create { uid: 5 }).unwrap();
    sh.push(LooperOp::Create { uid: 9 }).unwrap();
    b.drain_ops(&sh);
    b.publish(&sh);

    let uids: Vec<u64> = sh.statuses().iter().map(|s| s.uid).collect();
    assert_eq!(uids, vec![5, 9]);
}

#[test]
fn params_reach_the_slot() {
    let sh = shared();
    let mut b = bank(&sh);
    sh.push(LooperOp::Create { uid: UID }).unwrap();
    sh.push(LooperOp::TapRecord {
        uid: UID,
        buffer: Some(layer()),
    })
    .unwrap();
    b.drain_ops(&sh);
    b.process(&mut stereo(&[[1.0, 1.0]]), AudioChannelLayout::Stereo);
    sh.push(LooperOp::TapRecord {
        uid: UID,
        buffer: None,
    })
    .unwrap();
    sh.push(LooperOp::SetMix {
        uid: UID,
        value: 0.5,
    })
    .unwrap();
    b.drain_ops(&sh);

    let mut play = stereo(&[[0.0, 0.0]]);
    b.process(&mut play, AudioChannelLayout::Stereo);
    assert_eq!(lr(play[0]), [0.5, 0.5]);
}

#[test]
fn a_restored_layer_lands_stopped_and_plays_on_demand() {
    let sh = shared();
    let mut b = bank(&sh);
    sh.push(LooperOp::Create { uid: UID }).unwrap();
    let mut buf = layer();
    buf[0] = 0.75;
    buf[1] = 0.75;
    sh.push(LooperOp::LoadLayer {
        uid: UID,
        buffer: buf,
        len_frames: 1,
    })
    .unwrap();
    b.drain_ops(&sh);
    b.publish(&sh);
    assert_eq!(sh.status(UID).unwrap().state, LooperState::Stopped);

    let mut silent = stereo(&[[0.0, 0.0]]);
    b.process(&mut silent, AudioChannelLayout::Stereo);
    assert_eq!(lr(silent[0]), [0.0, 0.0], "stopped is silent");

    sh.push(LooperOp::Play { uid: UID }).unwrap();
    b.drain_ops(&sh);
    let mut play = stereo(&[[0.0, 0.0]]);
    b.process(&mut play, AudioChannelLayout::Stereo);
    assert_eq!(lr(play[0]), [0.75, 0.75]);
}
